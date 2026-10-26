//! Bounded controller input. Producers wait rather than dropping capture events.
use tokio::sync::mpsc;
#[derive(Debug)]
pub enum CaptureEvent {
    Logs(Box<crate::log_watcher::LogBatch>),
    Obs {
        generation: u64,
        event: owc_obs_client::Event,
    },
}
#[derive(Clone)]
pub struct EventBus(mpsc::Sender<CaptureEvent>);
impl EventBus {
    pub fn new() -> (Self, mpsc::Receiver<CaptureEvent>) {
        let (tx, rx) = mpsc::channel(256);
        (Self(tx), rx)
    }
    pub fn watch_obs(
        &self,
        mut events: tokio::sync::broadcast::Receiver<owc_obs_client::Event>,
        generation: u64,
    ) -> tokio::task::JoinHandle<()> {
        let bus = self.clone();
        tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        owc_obs_client::Event::Disconnected("Missed OBS events".into())
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                let disconnected = matches!(event, owc_obs_client::Event::Disconnected(_));
                if !bus.publish(CaptureEvent::Obs { generation, event }).await || disconnected {
                    break;
                }
            }
        })
    }
    pub async fn publish(&self, event: CaptureEvent) -> bool {
        self.0.send(event).await.is_ok()
    }
}
