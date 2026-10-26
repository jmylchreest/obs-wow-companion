//! OBS WebSocket v5 client. One actor owns the socket and correlates responses.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, time::Duration};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time::{Instant, timeout},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("OBS connection: {0}")]
    Connection(String),
    #[error("OBS protocol: {0}")]
    Protocol(String),
    #[error("OBS request timed out; reconcile state before retrying")]
    Timeout,
    #[error("OBS {request} failed ({code}): {comment}")]
    Request {
        request: String,
        code: i64,
        comment: String,
    },
}
#[derive(Debug, Clone)]
pub enum Event {
    Obs {
        kind: String,
        data: Value,
        received_ms: i64,
    },
    Disconnected(String),
}
type PendingReply = (String, Instant, oneshot::Sender<Result<Value, Error>>);
struct Request {
    kind: String,
    data: Value,
    reply: oneshot::Sender<Result<Value, Error>>,
}
#[derive(Clone)]
pub struct Client {
    tx: mpsc::Sender<Request>,
    events: broadcast::Sender<Event>,
}

pub fn authentication(password: &str, salt: &str, challenge: &str) -> String {
    let secret = STANDARD.encode(Sha256::digest(format!("{password}{salt}").as_bytes()));
    STANDARD.encode(Sha256::digest(format!("{secret}{challenge}").as_bytes()))
}
/// Reads OBS's greeting without authenticating, subscribing or changing any output.
pub async fn probe(port: u16) -> Result<bool, Error> {
    timeout(Duration::from_secs(2), async {
        let (mut socket, _) = connect_async(format!("ws://127.0.0.1:{port}"))
            .await
            .map_err(|e| Error::Connection(e.to_string()))?;
        let frame = socket
            .next()
            .await
            .ok_or_else(|| Error::Protocol("closed before hello".into()))?
            .map_err(|e| Error::Connection(e.to_string()))?;
        let hello: Value = serde_json::from_str(
            frame
                .to_text()
                .map_err(|e| Error::Protocol(e.to_string()))?,
        )
        .map_err(|e| Error::Protocol(e.to_string()))?;
        if hello["op"] != 0 || hello["d"]["rpcVersion"].as_u64().is_none_or(|v| v < 1) {
            return Err(Error::Protocol(
                "not a supported OBS WebSocket server".into(),
            ));
        }
        Ok(hello["d"].get("authentication").is_some())
    })
    .await
    .map_err(|_| Error::Timeout)?
}
impl Client {
    /// Connects only to the local machine; remote media storage is outside the supported contract.
    #[allow(clippy::too_many_lines)] // Handshake and socket actor share one connection lifecycle.
    pub async fn connect(port: u16, password: &str) -> Result<Self, Error> {
        let (mut socket, _) = timeout(
            Duration::from_secs(5),
            connect_async(format!("ws://127.0.0.1:{port}")),
        )
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(|e| Error::Connection(e.to_string()))?;
        let hello = timeout(Duration::from_secs(5), socket.next())
            .await
            .map_err(|_| Error::Timeout)?
            .ok_or_else(|| Error::Protocol("closed before hello".into()))?
            .map_err(|e| Error::Connection(e.to_string()))?;
        let hello: Value = serde_json::from_str(
            hello
                .to_text()
                .map_err(|e| Error::Protocol(e.to_string()))?,
        )
        .map_err(|e| Error::Protocol(e.to_string()))?;
        if hello["op"] != 0 || hello["d"]["rpcVersion"].as_u64().is_none_or(|v| v < 1) {
            return Err(Error::Protocol("unsupported hello".into()));
        }
        let mut identify = json!({"rpcVersion":1,"eventSubscriptions":1|2|4|64});
        if let Some(auth) = hello["d"].get("authentication") {
            let salt = auth["salt"]
                .as_str()
                .ok_or_else(|| Error::Protocol("missing salt".into()))?;
            let challenge = auth["challenge"]
                .as_str()
                .ok_or_else(|| Error::Protocol("missing challenge".into()))?;
            identify["authentication"] = authentication(password, salt, challenge).into();
        }
        socket
            .send(Message::Text(
                json!({"op":1,"d":identify}).to_string().into(),
            ))
            .await
            .map_err(|e| Error::Connection(e.to_string()))?;
        let ack = timeout(Duration::from_secs(5), socket.next())
            .await
            .map_err(|_| Error::Timeout)?
            .ok_or_else(|| Error::Protocol("closed during authentication".into()))?
            .map_err(|e| Error::Connection(e.to_string()))?;
        let ack: Value = serde_json::from_str(
            ack.to_text()
                .map_err(|_| Error::Protocol("authentication rejected".into()))?,
        )
        .map_err(|e| Error::Protocol(e.to_string()))?;
        if ack["op"] != 2 {
            return Err(Error::Protocol("authentication rejected".into()));
        }
        let (tx, mut rx) = mpsc::channel::<Request>(32);
        let (events, _) = broadcast::channel(256);
        let event_tx = events.clone();
        tokio::spawn(async move {
            let mut pending: HashMap<String, PendingReply> = HashMap::new();
            let mut serial = 0_u64;
            let mut sweep = tokio::time::interval(Duration::from_secs(1));
            let reason = loop {
                tokio::select! {
                    request=rx.recv()=>{
                        let Some(r)=request else {break "client closed".into()};
                        // A timed-out caller must not trigger a mutation later from the queue.
                        if r.reply.is_closed() {continue;}
                        if pending.len()>=32 {let _=r.reply.send(Err(Error::Protocol("request queue full".into())));continue;}
                        serial+=1;let id=serial.to_string();
                        let msg=json!({"op":6,"d":{"requestType":r.kind,"requestId":id,"requestData":r.data}});
                        if let Err(e)=socket.send(Message::Text(msg.to_string().into())).await {let _=r.reply.send(Err(Error::Connection(e.to_string())));break e.to_string();}
                        pending.insert(id,(r.kind,Instant::now(),r.reply));
                    },
                    frame=socket.next()=>{
                        match frame {
                            Some(Ok(Message::Text(text)))=>{
                                let Ok(v)=serde_json::from_str::<Value>(&text) else {break "invalid JSON".into()};
                                match v["op"].as_u64() {
                                    Some(5)=>{
                                        let received_ms=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().and_then(|d|i64::try_from(d.as_millis()).ok()).unwrap_or_default();
                                        let _=event_tx.send(Event::Obs{kind:v["d"]["eventType"].as_str().unwrap_or_default().into(),data:v["d"]["eventData"].clone(),received_ms});
                                    },
                                    Some(7)=>{
                                        if let Some((kind,_,reply))=v["d"]["requestId"].as_str().and_then(|id|pending.remove(id)) {
                                            let result=if v["d"]["requestStatus"]["result"]==true {Ok(v["d"]["responseData"].clone())}
                                                else {Err(Error::Request{request:kind,code:v["d"]["requestStatus"]["code"].as_i64().unwrap_or(0),comment:v["d"]["requestStatus"]["comment"].as_str().unwrap_or_default().into()})};
                                            let _=reply.send(result);
                                        }
                                    },
                                    _=>{}
                                }
                            },
                            Some(Ok(Message::Ping(p)))=>{if let Err(e)=socket.send(Message::Pong(p)).await {break e.to_string();}},
                            Some(Ok(Message::Close(_)))|None=>break "socket closed".into(),
                            Some(Err(e))=>break e.to_string(),
                            _=>{}
                        }
                    },
                    _=sweep.tick()=>{
                        let expired:Vec<_>=pending.iter().filter(|(_,(_,at,_))|at.elapsed()>Duration::from_secs(5)).map(|(id,_)|id.clone()).collect();
                        for id in expired {if let Some((_,_,reply))=pending.remove(&id){let _=reply.send(Err(Error::Timeout));}}
                    }
                }
            };
            for (_, (_, _, reply)) in pending {
                let _ = reply.send(Err(Error::Connection(reason.clone())));
            }
            let _ = event_tx.send(Event::Disconnected(reason));
        });
        Ok(Self { tx, events })
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
    /// Read an output status, treating OBS's unavailable replay buffer as inactive.
    pub async fn output_active(&self, kind: &str) -> Result<bool, Error> {
        output_active_result(kind, self.request(kind, json!({})).await)
    }
    pub async fn request(&self, kind: &str, data: Value) -> Result<Value, Error> {
        let (tx, rx) = oneshot::channel();
        timeout(Duration::from_secs(6), async {
            self.tx
                .send(Request {
                    kind: kind.into(),
                    data,
                    reply: tx,
                })
                .await
                .map_err(|_| Error::Connection("actor stopped".into()))?;
            rx.await
                .map_err(|_| Error::Connection("actor stopped".into()))?
        })
        .await
        .map_err(|_| Error::Timeout)?
    }
}

fn output_active_result(kind: &str, result: Result<Value, Error>) -> Result<bool, Error> {
    match result {
        Ok(value) => value["outputActive"]
            .as_bool()
            .ok_or_else(|| Error::Protocol("missing outputActive status".into())),
        // OBS has no replay output object when the replay buffer is disabled.
        Err(Error::Request {
            request,
            code: 604,
            comment,
        }) if kind == "GetReplayBufferStatus"
            && request == kind
            && comment == "Replay buffer is not available." =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_unavailable_replay_buffer_is_treated_as_inactive() {
        let unavailable = || {
            Err(Error::Request {
                request: "GetReplayBufferStatus".into(),
                code: 604,
                comment: "Replay buffer is not available.".into(),
            })
        };
        assert!(!output_active_result("GetReplayBufferStatus", unavailable()).expect("inactive"));
        assert!(output_active_result("GetRecordStatus", unavailable()).is_err());
        assert!(output_active_result("GetReplayBufferStatus", Err(Error::Timeout)).is_err());
        assert!(output_active_result("GetRecordStatus", Ok(json!({}))).is_err());
        assert!(
            output_active_result("GetReplayBufferStatus", Ok(json!({"outputActive":true})))
                .expect("active")
        );
    }
    #[tokio::test]
    async fn discovery_reads_hello_without_identifying_or_mutating() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listen");
        let port = listener.local_addr().expect("address").port();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept");
            let mut socket = tokio_tungstenite::accept_async(tcp).await.expect("socket");
            socket.send(Message::Text(json!({"op":0,"d":{"rpcVersion":1,"authentication":{"salt":"test","challenge":"test"}}}).to_string().into())).await.expect("hello");
            assert!(!matches!(socket.next().await, Some(Ok(Message::Text(_)))));
        });
        assert!(probe(port).await.expect("discover OBS"));
        server.await.expect("server");
    }
    async fn accept_identified(
        listener: tokio::net::TcpListener,
    ) -> tokio_tungstenite::WebSocketStream<tokio::net::TcpStream> {
        let (tcp, _) = listener.accept().await.expect("accept");
        let mut socket = tokio_tungstenite::accept_async(tcp)
            .await
            .expect("websocket");
        socket
            .send(Message::Text(
                json!({"op":0,"d":{"rpcVersion":1}}).to_string().into(),
            ))
            .await
            .expect("hello");
        socket.next().await.expect("identify").expect("frame");
        socket
            .send(Message::Text(
                json!({"op":2,"d":{"negotiatedRpcVersion":1}})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("identified");
        socket
    }

    #[tokio::test]
    async fn replies_are_correlated_when_reordered() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listen");
        let port = listener.local_addr().expect("address").port();
        let server = tokio::spawn(async move {
            let mut socket = accept_identified(listener).await;
            let mut requests = vec![];
            for _ in 0..2 {
                let frame = socket.next().await.expect("request").expect("frame");
                requests.push(
                    serde_json::from_str::<Value>(frame.to_text().expect("text")).expect("json"),
                );
            }
            for request in requests.iter().rev() {
                socket.send(Message::Text(json!({"op":7,"d":{"requestId":request["d"]["requestId"],"requestStatus":{"result":true,"code":100},"responseData":{"method":request["d"]["requestType"]}}}).to_string().into())).await.expect("reply");
            }
        });
        let client = Client::connect(port, "").await.expect("connect");
        let (first, second) = tokio::join!(
            client.request("First", json!({})),
            client.request("Second", json!({}))
        );
        assert_eq!(first.expect("first")["method"], "First");
        assert_eq!(second.expect("second")["method"], "Second");
        server.await.expect("server");
    }

    #[tokio::test]
    async fn disconnect_fails_pending_request_and_announces_loss() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listen");
        let port = listener.local_addr().expect("address").port();
        let server = tokio::spawn(async move {
            let mut socket = accept_identified(listener).await;
            socket.next().await.expect("request").expect("frame");
            socket.close(None).await.expect("close");
        });
        let client = Client::connect(port, "").await.expect("connect");
        let mut events = client.subscribe();
        assert!(matches!(
            client.request("GetRecordStatus", json!({})).await,
            Err(Error::Connection(_))
        ));
        assert!(matches!(
            events.recv().await.expect("event"),
            Event::Disconnected(_)
        ));
        server.await.expect("server");
    }

    #[tokio::test]
    async fn cancelled_queued_mutation_is_not_sent_to_obs() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listen");
        let port = listener.local_addr().expect("address").port();
        let server = tokio::spawn(async move {
            let mut socket = accept_identified(listener).await;
            let frame = socket.next().await.expect("request").expect("frame");
            let request: Value =
                serde_json::from_str(frame.to_text().expect("text")).expect("request JSON");
            let kind = request["d"]["requestType"].clone();
            socket
                .send(Message::Text(
                    json!({"op":7,"d":{
                        "requestId":request["d"]["requestId"],
                        "requestStatus":{"result":true,"code":100},
                        "responseData":{"outputActive":false}
                    }})
                    .to_string()
                    .into(),
                ))
                .await
                .expect("reply");
            kind
        });
        let client = Client::connect(port, "").await.expect("connect");
        let (reply, cancelled) = oneshot::channel();
        drop(cancelled);
        client
            .tx
            .send(Request {
                kind: "StartRecord".into(),
                data: json!({}),
                reply,
            })
            .await
            .expect("queue cancelled command");
        let result = client.request("GetRecordStatus", json!({})).await;
        assert_eq!(server.await.expect("server"), "GetRecordStatus");
        assert_eq!(
            result.expect("reconciliation response")["outputActive"],
            false
        );
    }

    #[tokio::test]
    async fn silent_peer_has_bounded_request_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listen");
        let port = listener.local_addr().expect("address").port();
        let server = tokio::spawn(async move {
            let mut socket = accept_identified(listener).await;
            socket.next().await.expect("request").expect("frame");
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let client = Client::connect(port, "").await.expect("connect");
        let response = timeout(
            Duration::from_secs(8),
            client.request("StartRecord", json!({})),
        )
        .await
        .expect("bounded timeout");
        assert!(matches!(response, Err(Error::Timeout)));
        server.abort();
    }
    #[tokio::test]
    async fn authenticates_correlates_and_delivers_events() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listen");
        let port = listener.local_addr().expect("addr").port();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept");
            let mut ws = tokio_tungstenite::accept_async(tcp)
                .await
                .expect("websocket");
            ws.send(Message::Text(json!({"op":0,"d":{"rpcVersion":1,"authentication":{"salt":"salt","challenge":"nonce"}}}).to_string().into())).await.expect("hello");
            let identify: Value = serde_json::from_str(
                ws.next()
                    .await
                    .expect("frame")
                    .expect("frame")
                    .to_text()
                    .expect("text"),
            )
            .expect("json");
            assert_eq!(
                identify["d"]["authentication"],
                authentication("password", "salt", "nonce")
            );
            ws.send(Message::Text(
                json!({"op":2,"d":{"negotiatedRpcVersion":1}})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("ack");
            let req: Value = serde_json::from_str(
                ws.next()
                    .await
                    .expect("req")
                    .expect("frame")
                    .to_text()
                    .expect("text"),
            )
            .expect("json");
            ws.send(Message::Text(json!({"op":5,"d":{"eventType":"RecordFileChanged","eventData":{"newOutputPath":"test.mkv"}}}).to_string().into())).await.expect("event");
            ws.send(Message::Text(json!({"op":7,"d":{"requestId":req["d"]["requestId"],"requestStatus":{"result":true,"code":100},"responseData":{"outputActive":false}}}).to_string().into())).await.expect("reply");
        });
        let client = Client::connect(port, "password").await.expect("connect");
        let mut events = client.subscribe();
        assert_eq!(
            client
                .request("GetRecordStatus", json!({}))
                .await
                .expect("reply")["outputActive"],
            false
        );
        assert!(
            matches!(events.recv().await.expect("event"),Event::Obs{kind,..} if kind=="RecordFileChanged")
        );
        server.await.expect("server");
    }
}
