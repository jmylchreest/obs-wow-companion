use super::*;
use futures_util::{SinkExt, StreamExt};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

fn server(
    listener: Arc<TcpListener>,
    scene: Arc<Mutex<String>>,
    streaming: bool,
) -> tokio::task::JoinHandle<()> {
    server_logged(listener, scene, streaming, Arc::new(Mutex::new(vec![])))
}
fn server_logged(
    listener: Arc<TcpListener>,
    scene: Arc<Mutex<String>>,
    streaming: bool,
    requests: Arc<Mutex<Vec<String>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.expect("accept");
            let Ok(mut socket) = accept_async(socket).await else {
                continue;
            };
            socket
                .send(Message::Text(
                    json!({"op":0,"d":{"rpcVersion":1}}).to_string().into(),
                ))
                .await
                .expect("hello");
            while let Some(Ok(frame)) = socket.next().await {
                let Ok(text) = frame.to_text() else {
                    break;
                };
                let Ok(value) = serde_json::from_str::<Value>(text) else {
                    break;
                };
                let reply = if value["op"] == 1 {
                    json!({"op":2,"d":{"negotiatedRpcVersion":1}})
                } else {
                    let request = &value["d"];
                    requests
                        .lock()
                        .expect("requests")
                        .push(request["requestType"].as_str().expect("type").into());
                    let data = match request["requestType"].as_str().expect("request") {
                        "GetVersion" => json!({"availableRequests":[]}),
                        "GetRecordStatus" => json!({"outputActive":false}),
                        "GetStreamStatus" => json!({"outputActive":streaming}),
                        "GetCurrentProgramScene" => {
                            json!({"currentProgramSceneName":scene.lock().expect("scene").clone()})
                        }
                        "SetCurrentProgramScene" => {
                            *scene.lock().expect("scene") = request["requestData"]["sceneName"]
                                .as_str()
                                .expect("name")
                                .into();
                            json!({})
                        }
                        _ => json!({}),
                    };
                    json!({"op":7,"d":{"requestType":request["requestType"],"requestId":request["requestId"],"requestStatus":{"result":true,"code":100},"responseData":data}})
                };
                if socket
                    .send(Message::Text(reply.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    })
}
fn engine(root: &std::path::Path, port: u16) -> Engine {
    let (tx, _) = mpsc::channel(32);
    let (status, _) = watch::channel(json!({}));
    let mut engine = Engine::new(root.to_owned(), tx, status).expect("engine");
    engine.config.obs_port = port;
    engine
}
#[tokio::test]
async fn low_space_stops_only_owned_output_once_and_preserves_sources() {
    let root = tempfile::tempdir().expect("root");
    let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.expect("listen"));
    let port = listener.local_addr().expect("address").port();
    let requests = Arc::new(Mutex::new(vec![]));
    let task = server_logged(
        listener,
        Arc::new(Mutex::new("Original".into())),
        false,
        requests.clone(),
    );
    let mut engine = engine(root.path(), port);
    engine
        .command(json!({"action":"connect","password":""}))
        .await
        .expect("connect");
    // Exercise the actual free-space branch without filling the host filesystem.
    engine.config.minimum_free_gib = u32::MAX;
    let now = Engine::now();
    let open = engine.config.spool.join("open.mkv");
    let pinned = engine.config.spool.join("pinned.mkv");
    let external = root.path().join("external.mkv");
    for path in [&open, &pinned, &external] {
        std::fs::write(path, b"preserve source").expect("source");
    }
    let activity = Activity::new(Kind::MythicPlus, "Key".into(), now - 1000, "Retail");
    engine
        .db
        .save_activity(&activity)
        .expect("pending activity");
    for (path, managed) in [(&pinned, true), (&external, false)] {
        engine
            .db
            .add_segment(&Segment {
                id: Uuid::new_v4(),
                session: Uuid::new_v4(),
                path: path.clone(),
                start_ms: now - 1000,
                end_ms: now,
                closed: true,
                managed,
            })
            .expect("segment");
    }
    engine.recording = Some(OpenRecording {
        session: Uuid::new_v4(),
        path: Some(open.clone()),
        start_ms: now,
        managed: false,
        invalid: false,
    });
    engine.guard.reconcile(true);
    engine.armed = true;
    requests.lock().expect("requests").clear();
    engine.cleanup().await.expect("observed cleanup");
    assert!(
        requests
            .lock()
            .expect("requests")
            .iter()
            .all(|request| request.starts_with("Get"))
    );
    assert!(engine.armed);
    engine.guard.reconcile(false);
    engine.guard.arm(false);
    engine.guard.started();
    engine.cleanup().await.expect("managed cleanup");
    engine.cleanup().await.expect("repeat cleanup");
    assert_eq!(
        requests
            .lock()
            .expect("requests")
            .iter()
            .filter(|request| !request.starts_with("Get"))
            .cloned()
            .collect::<Vec<_>>(),
        vec!["StopRecord"]
    );
    assert!(!engine.armed);
    assert!(engine.message.contains("storage budget"));
    for path in [&open, &pinned, &external] {
        assert_eq!(std::fs::read(path).expect("preserved"), b"preserve source");
    }
    task.abort();
}

#[tokio::test]
async fn reconnects_after_socket_loss_without_resuming_ownership() {
    let root = tempfile::tempdir().expect("root");
    let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.expect("listen"));
    let port = listener.local_addr().expect("address").port();
    let scene = Arc::new(Mutex::new("Original".into()));
    let task = server(listener.clone(), scene.clone(), false);
    let mut engine = engine(root.path(), port);
    engine
        .command(json!({"action":"connect","password":"session-fixture"}))
        .await
        .expect("connect");
    engine.armed = true;
    task.abort();
    let _ = task.await;
    let event = tokio::time::timeout(Duration::from_secs(2), engine.bus_events.recv())
        .await
        .expect("disconnect timeout")
        .expect("event");
    engine.capture_event(event).await.expect("disconnect");
    assert!(engine.obs.is_none());
    assert!(!engine.armed);
    let task = server(listener, scene, false);
    engine.last_reconnect = Instant::now()
        .checked_sub(Duration::from_secs(3))
        .expect("clock");
    engine.tick().await.expect("retry tick");
    assert!(engine.obs.is_some());
    assert!(!engine.armed);
    assert!(!engine.guard.can_mutate());
    assert!(
        !engine
            .snapshot()
            .expect("status")
            .to_string()
            .contains("session-fixture")
    );
    task.abort();
}
#[tokio::test]
async fn scene_restores_only_without_user_override_and_never_while_streaming() {
    let root = tempfile::tempdir().expect("root");
    for streaming in [false, true] {
        let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.expect("listen"));
        let port = listener.local_addr().expect("address").port();
        let scene = Arc::new(Mutex::new("Original".into()));
        let task = server(listener, scene.clone(), streaming);
        let mut engine = engine(root.path(), port);
        engine
            .command(json!({"action":"connect"}))
            .await
            .expect("connect");
        engine.config.recording_scene = "WoW".into();
        let result = engine.select_recording_scene().await;
        if streaming {
            assert!(result.is_err());
            assert_eq!(*scene.lock().expect("scene"), "Original");
        } else {
            result.expect("select");
            assert_eq!(*scene.lock().expect("scene"), "WoW");
            engine.restore_recording_scene().await.expect("restore");
            assert_eq!(*scene.lock().expect("scene"), "Original");
            engine.select_recording_scene().await.expect("select again");
            *scene.lock().expect("scene") = "User choice".into();
            engine
                .restore_recording_scene()
                .await
                .expect("respect override");
            assert_eq!(*scene.lock().expect("scene"), "User choice");
        }
        task.abort();
        let _ = task.await;
    }
}

#[tokio::test]
async fn automatic_lifecycle_tracks_overlap_rejects_backlog_and_respects_pause() {
    let root = tempfile::tempdir().expect("root");
    let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.expect("listen"));
    let port = listener.local_addr().expect("port").port();
    let task = server(listener, Arc::new(Mutex::new("Original".into())), false);
    let mut engine = engine(root.path(), port);
    engine
        .command(json!({"action":"connect"}))
        .await
        .expect("connect");
    engine.setup_completed = true;
    engine.armed = true;
    engine.automatic_session = true;
    let first = Activity::new(Kind::Raid, "First".into(), Engine::now(), "Retail");
    let second = Activity::new(Kind::Raid, "Second".into(), Engine::now(), "Classic");
    engine.activity_started(first.clone()).await.expect("first");
    engine
        .activity_started(second.clone())
        .await
        .expect("second");
    engine.activity_finished(first);
    assert!(engine.automatic_stop.is_none());
    engine.activity_finished(second);
    assert!(engine.automatic_stop.is_some());
    assert_eq!(engine.completed_events.len(), 2);
    engine
        .activity_started(Activity::new(
            Kind::Raid,
            "Old".into(),
            Engine::now() - 60_000,
            "Retail",
        ))
        .await
        .expect("backlog");
    assert!(engine.automatic_activities.is_empty());
    engine
        .command(json!({"action":"pause_automatic"}))
        .await
        .expect("pause");
    engine
        .activity_started(Activity::new(
            Kind::Raid,
            "Paused".into(),
            Engine::now(),
            "Retail",
        ))
        .await
        .expect("paused");
    assert!(!engine.armed);
    assert!(engine.automatic_activities.is_empty());
    assert_eq!(
        engine.db.setting("automatic_paused").expect("setting"),
        Some(json!(true))
    );
    engine.guard.arm(false);
    engine.stop_capture().await.expect("stop during start");
    assert!(engine.stop_when_started);
    engine
        .event(Event::Obs {
            kind: "RecordStateChanged".into(),
            data: json!({"outputState":"OBS_WEBSOCKET_OUTPUT_STARTED"}),
            received_ms: Engine::now(),
        })
        .await
        .expect("late confirmation");
    assert!(engine.guard.stop_pending);
    assert!(!engine.stop_when_started);

    task.abort();
}

#[tokio::test]
async fn delayed_start_uses_existing_capture_without_starting_or_deferring_obs() {
    let root = tempfile::tempdir().expect("root");
    let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.expect("listen"));
    let port = listener.local_addr().expect("port").port();
    let task = server(listener, Arc::new(Mutex::new("Original".into())), false);
    let mut engine = engine(root.path(), port);
    engine
        .command(json!({"action":"connect"}))
        .await
        .expect("connect");
    engine.setup_completed = true;
    engine.armed = true;
    engine.automatic_session = true;
    let now = Engine::now();
    let session = Uuid::new_v4();
    engine.recording = Some(OpenRecording {
        session,
        path: None,
        start_ms: now - 5_000,
        managed: true,
        invalid: false,
    });
    let delayed = Activity::new(Kind::Raid, "Delayed".into(), now - 30_000, "Retail");
    engine
        .activity_started(delayed.clone())
        .await
        .expect("partial coverage");
    assert!(engine.automatic_activities.contains(&delayed.id));
    let mut partial = delayed.clone();
    partial.finish(now, owc_domain::Outcome::Success);
    engine.activity_finished(partial);
    let plan = owc_domain::plan_clip(
        &engine.completed_events[0],
        &[Segment {
            id: Uuid::new_v4(),
            session,
            path: root.path().join("partial.mkv"),
            start_ms: now - 5_000,
            end_ms: now,
            closed: true,
            managed: true,
        }],
        1_000,
        0,
    );
    assert_eq!(plan.coverage, owc_domain::Coverage::Truncated);
    assert_eq!(plan.actual_start_ms, Some(now - 5_000));
    engine.completed_events.clear();
    assert!(engine.automatic_activities.is_empty());
    engine
        .db
        .add_segment(&Segment {
            id: Uuid::new_v4(),
            session,
            path: root.path().join("retained.mkv"),
            start_ms: now - 40_000,
            end_ms: now - 5_000,
            closed: true,
            managed: true,
        })
        .expect("retained segment");
    engine.guard.stop_pending = true;
    engine
        .activity_started(delayed.clone())
        .await
        .expect("stopping");
    assert!(engine.deferred_starts.is_empty());
    assert!(engine.automatic_activities.is_empty());
    engine.guard.stop_pending = false;
    engine
        .activity_started(delayed.clone())
        .await
        .expect("recover delayed start");
    assert!(engine.automatic_activities.contains(&delayed.id));
    engine.activity_finished(delayed);
    assert_eq!(engine.completed_events.len(), 1);
    assert!(engine.automatic_stop.is_some());
    task.abort();
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Fake peer and recovery assertions form a single protocol scenario.
async fn persisted_owned_recording_resumes_and_stops_after_reconnect() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("current.mkv");
    std::fs::write(&path, b"active fixture").expect("file");
    let now = Engine::now();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let current = Arc::new(Mutex::new(
        json!({"outputActive":true,"outputPaused":false,"outputDuration":5100,"outputBytes":2000}),
    ));
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let task = {
        let current = current.clone();
        let requests = requests.clone();
        let path = path.clone();
        tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.expect("accept");
                let Ok(mut socket) = accept_async(socket).await else {
                    continue;
                };
                socket
                    .send(Message::Text(
                        json!({"op":0,"d":{"rpcVersion":1}}).to_string().into(),
                    ))
                    .await
                    .expect("hello");
                while let Some(Ok(frame)) = socket.next().await {
                    let Ok(value) =
                        serde_json::from_str::<Value>(frame.to_text().unwrap_or_default())
                    else {
                        break;
                    };
                    let reply = if value["op"] == 1 {
                        json!({"op":2,"d":{"negotiatedRpcVersion":1}})
                    } else {
                        let request = &value["d"];
                        let kind = request["requestType"].as_str().expect("kind");
                        requests.lock().expect("requests").push(kind.into());
                        let data = match kind {
                            "GetVersion" => json!({"availableRequests":[]}),
                            "GetRecordStatus" => current.lock().expect("state").clone(),
                            "GetOutputSettings" => json!({"outputSettings":{"path":path}}),
                            _ => json!({}),
                        };
                        json!({"op":7,"d":{"requestType":kind,"requestId":request["requestId"],"requestStatus":{"result":true,"code":100},"responseData":data}})
                    };
                    if socket
                        .send(Message::Text(reply.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        })
    };
    let checkpoint = RecordingCheckpoint {
        recording: OpenRecording {
            session: Uuid::new_v4(),
            path: Some(path.clone()),
            start_ms: now - 5000,
            managed: true,
            invalid: false,
        },
        port,
        output_name: "recording".into(),
        output_path: path,
        duration_ms: 5000,
        bytes: 1000,
        sampled_ms: now,
        automatic: true,
        scene_restore: None,
    };
    for (duration, paused, expected) in [
        (5100, false, true),
        (100, false, false),
        (5100, true, false),
    ] {
        let first = engine(root.path(), port);
        first
            .db
            .set_setting(
                "recording_checkpoint",
                &serde_json::to_value(&checkpoint).expect("checkpoint"),
            )
            .expect("persist");
        drop(first);
        current.lock().expect("state")["outputDuration"] = json!(duration);
        current.lock().expect("state")["outputPaused"] = json!(paused);
        let mut resumed = engine(root.path(), port);
        resumed.config.post_seconds = 0;
        resumed
            .command(json!({"action":"connect"}))
            .await
            .expect("reconnect");
        assert_eq!(resumed.guard.can_mutate(), expected);
        let pending = json!({"id":"waiting", "status":"pending", "end_ms":now-1000});
        assert_eq!(
            resumed.processing_status(&pending, &[])["stage"],
            if expected {
                "source_open"
            } else {
                "observed_source"
            }
        );
        requests.lock().expect("requests").clear();
        if expected {
            resumed.automatic_stop = Some(Instant::now());
        }
        resumed.tick().await.expect("tick");
        if expected {
            assert_eq!(
                resumed.processing_status(&pending, &[])["stage"],
                "source_open",
                "a pending StopRecord is still companion-owned"
            );
        }
        assert_eq!(
            requests
                .lock()
                .expect("requests")
                .iter()
                .any(|r| r == "StopRecord"),
            expected
        );
        if let Some(watcher) = resumed.obs_watcher.take() {
            watcher.abort();
        }
        resumed.obs = None;
    }
    task.abort();
}

#[tokio::test]
#[ignore = "requires isolated OBS; run with OWC_RECOVERY_OBS_PORT"]
async fn native_obs_reconnect_keeps_owned_fragment_and_closes_it() {
    let port = std::env::var("OWC_RECOVERY_OBS_PORT")
        .expect("isolated OBS port")
        .parse()
        .expect("port");
    let root = tempfile::tempdir().expect("root");
    let mut engine = engine(root.path(), port);
    engine
        .command(json!({"action":"connect","password":"owc-isolated-test"}))
        .await
        .expect("connect");
    engine
        .command(json!({"action":"configure_output"}))
        .await
        .expect("output");
    engine.config.post_seconds = 0;
    engine.start_capture().await.expect("start");
    engine.automatic_session = true;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        while let Ok(event) = engine.bus_events.try_recv() {
            engine.capture_event(event).await.expect("event");
        }
    }
    engine
        .refresh_recording_checkpoint()
        .await
        .expect("checkpoint");
    assert!(
        engine
            .db
            .setting("recording_checkpoint")
            .expect("checkpoint")
            .is_some_and(|v| !v.is_null())
    );
    assert!(engine.guard.request_split());
    engine
        .db
        .set_setting("recording_checkpoint", &Value::Null)
        .expect("invalidate in-flight split");
    engine
        .obs_request("SplitRecordFile", json!({}))
        .await
        .expect("split");
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        while let Ok(event) = engine.bus_events.try_recv() {
            engine.capture_event(event).await.expect("split event");
        }
        if !engine.guard.split_pending {
            break;
        }
    }
    assert_eq!(
        engine.db.segments().expect("closed first fragment").len(),
        1
    );
    engine
        .refresh_recording_checkpoint()
        .await
        .expect("split checkpoint");
    drop(engine);
    let mut engine = self::engine(root.path(), port);
    engine.config.post_seconds = 0;
    engine
        .command(json!({"action":"connect","password":"owc-isolated-test"}))
        .await
        .expect("reconnect");
    assert!(
        engine.guard.can_mutate(),
        "verified original recording should resume control"
    );
    engine.automatic_stop = Some(Instant::now());
    engine.tick().await.expect("stop tick");
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        while let Ok(event) = engine.bus_events.try_recv() {
            engine.capture_event(event).await.expect("event");
        }
        if engine.recording.is_none() {
            break;
        }
    }
    assert!(engine.recording.is_none());
    assert_eq!(engine.db.segments().expect("segments").len(), 2);
    assert_eq!(
        engine
            .db
            .setting("recording_checkpoint")
            .expect("checkpoint"),
        Some(Value::Null)
    );
}

#[tokio::test]
async fn shutdown_stops_owned_capture_once_and_leaves_observed_output_alone() {
    for owned in [false, true] {
        let root = tempfile::tempdir().expect("root");
        let listener = Arc::new(TcpListener::bind("127.0.0.1:0").await.expect("listen"));
        let port = listener.local_addr().expect("address").port();
        let requests = Arc::new(Mutex::new(vec![]));
        let server = server_logged(
            listener,
            Arc::new(Mutex::new("Original".into())),
            false,
            requests.clone(),
        );
        let mut engine = engine(root.path(), port);
        engine
            .command(json!({"action":"connect","password":""}))
            .await
            .expect("connect");
        if owned {
            engine.guard.arm(false);
            engine.guard.started();
        } else {
            engine.guard.reconcile(true);
        }
        engine.armed = true;
        requests.lock().expect("requests").clear();
        engine.prepare_shutdown().await.expect("shutdown");
        engine
            .prepare_shutdown()
            .await
            .expect("idempotent shutdown");
        let mutations = requests
            .lock()
            .expect("requests")
            .iter()
            .filter(|request| !request.starts_with("Get"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(mutations, if owned { vec!["StopRecord"] } else { vec![] });
        assert!(engine.shutting_down);
        assert!(!engine.armed);
        assert!(
            engine
                .command(json!({"action":"manual_start"}))
                .await
                .is_err()
        );
        assert_ne!(
            engine.db.setting("automatic_paused").expect("preference"),
            Some(json!(true))
        );
        assert_eq!(
            engine.snapshot().expect("snapshot")["shutdown_ready"],
            !owned
        );
        server.abort();
    }
}
