use crate::config::Config;
use crate::events::{CaptureEvent, EventBus};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use owc_capture::{Guard, Ownership};
use owc_domain::{Activity, Coverage, Kind, Outcome, Segment, plan_clip};
use owc_library::Library;
use owc_media::CutMode;
use owc_obs_client::{Client, Event};
use owc_wow_log::{Detector, Tailer};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, watch};
use uuid::Uuid;

pub struct Request {
    pub data: Value,
    pub reply: oneshot::Sender<Result<Value>>,
}

use crate::log_watcher::{AbandonReason, ActivityEvent, LogBatch, LogEvent, LogState};
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct OpenRecording {
    session: Uuid,
    path: Option<PathBuf>,
    start_ms: i64,
    managed: bool,
    invalid: bool,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct RecordingCheckpoint {
    recording: OpenRecording,
    port: u16,
    output_name: String,
    output_path: PathBuf,
    duration_ms: i64,
    bytes: u64,
    sampled_ms: i64,
    automatic: bool,
    scene_restore: Option<(String, String)>,
}
impl RecordingCheckpoint {
    fn continues(&self, state: &Value, now: i64) -> bool {
        let (Some(duration), Some(bytes)) = (
            state["outputDuration"].as_i64(),
            state["outputBytes"].as_u64(),
        ) else {
            return false;
        };
        let elapsed = now.saturating_sub(self.sampled_ms);
        let advanced = duration.saturating_sub(self.duration_ms);
        let recent_file = self
            .recording
            .path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .and_then(|m| m.modified().ok())
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age < Duration::from_secs(5));
        elapsed >= 0
            && advanced >= 0
            && bytes >= self.bytes
            && elapsed.abs_diff(advanced) <= 2000
            && recent_file
    }
}
struct ExportJob {
    id: String,
    started: Instant,
    progress: watch::Receiver<String>,
    cancel: watch::Sender<bool>,
}
#[allow(clippy::struct_excessive_bools)] // Independent job/capability flags; ownership is a separate state machine.
pub struct Engine {
    root: PathBuf,
    db: Library,
    config: Config,
    logs: BTreeMap<String, LogState>,
    obs: Option<Client>,
    obs_stats: Value,
    setup: Value,
    setup_generation: u64,
    last_setup_check: Instant,
    setup_completed: bool,
    credential_scope: String,
    credential_status: String,
    session_password: Option<String>,
    last_reconnect: Instant,
    reconnect_delay: Duration,
    scene_restore: Option<(String, String)>,
    guard: Guard,
    recording: Option<OpenRecording>,
    split_supported: bool,
    last_split: Instant,
    last_poll: Instant,
    sidecar_cursor: usize,
    last_clock: (Instant, i64),
    export_running: bool,
    export_job: Option<ExportJob>,
    preview_running: bool,
    share_running: bool,
    share_cancel: Option<watch::Sender<bool>>,
    share_status: Value,
    share_progress: Option<watch::Receiver<String>>,
    preview_progress: Option<watch::Receiver<String>>,
    last_media_log: Instant,
    preview_status: Value,
    message: String,
    event_log: crate::event_log::EventLog,
    logged_message: String,
    logged_connected: Option<bool>,
    log_errors: u64,
    tx: mpsc::Sender<Request>,
    status: watch::Sender<Value>,
    armed: bool,
    shutting_down: bool,
    bus: EventBus,
    bus_events: mpsc::Receiver<CaptureEvent>,
    obs_generation: u64,
    obs_watcher: Option<tokio::task::JoinHandle<()>>,
    automatic_paused: bool,
    automatic_session: bool,
    automatic_activities: std::collections::HashSet<Uuid>,
    interrupted_activities: std::collections::HashSet<Uuid>,
    automatic_stop: Option<Instant>,
    completed_events: Vec<Activity>,
    pending_log_acks: Vec<oneshot::Sender<()>>,
    deferred_starts: Vec<Activity>,
    stop_when_started: bool,
    log_generation: u64,
    log_watchers: Vec<tokio::task::JoinHandle<()>>,
    last_log_event: Option<LogEvent>,
}
impl Engine {
    pub fn new(
        root: PathBuf,
        tx: mpsc::Sender<Request>,
        status: watch::Sender<Value>,
    ) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        let mut db = Library::open(&root.join("library.sqlite"))?;
        let mut config = db
            .setting("config")?
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_else(|| Config::new(&root));
        Self::migrate_defaults(&db, &mut config)?;
        let mut logs: BTreeMap<String, LogState> = db
            .setting("logs")?
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default();
        Self::recover_manual(&mut db, &mut logs)?;
        db.set_setting("config", &serde_json::to_value(&config)?)?;
        for installation in &config.installations {
            let key = installation.id.to_string();
            if !logs.contains_key(&key)
                && let Some(state) = logs.remove(&installation.name)
            {
                logs.insert(key, state);
            }
        }
        std::fs::create_dir_all(&config.spool)?;
        std::fs::create_dir_all(&config.library)?;
        std::fs::create_dir_all(root.join("preview"))?;
        let setup_completed = db
            .setting("setup_completed")?
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let credential_scope = db
            .setting("credential_scope")?
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        db.set_setting("credential_scope", &json!(credential_scope))?;
        let (bus, bus_events) = EventBus::new();
        let automatic_paused = db
            .setting("automatic_paused")?
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Ok(Self {
            bus,
            bus_events,
            obs_generation: 0,
            obs_watcher: None,
            automatic_paused,
            automatic_session: false,
            automatic_activities: std::collections::HashSet::new(),
            interrupted_activities: std::collections::HashSet::new(),
            automatic_stop: None,
            completed_events: vec![],
            pending_log_acks: vec![],
            deferred_starts: vec![],
            stop_when_started: false,
            log_generation: 0,
            log_watchers: vec![],
            last_log_event: None,
            credential_scope,
            credential_status: String::new(),
            session_password: None,
            last_reconnect: Instant::now(),
            reconnect_delay: Duration::from_secs(2),
            scene_restore: None,
            event_log: crate::event_log::EventLog::new(&root),
            logged_message: String::new(),
            logged_connected: None,
            root,
            db,
            config,
            logs,
            obs: None,
            obs_stats: Value::Null,
            setup: json!({"connection":"checking","checks":[],"checking":true}),
            setup_generation: 0,
            last_setup_check: Instant::now(),
            setup_completed,
            guard: Guard::default(),
            recording: None,
            split_supported: false,
            last_split: Instant::now(),
            last_poll: Instant::now(),
            sidecar_cursor: 0,
            last_clock: (Instant::now(), Self::now()),
            export_running: false,
            export_job: None,
            preview_running: false,
            share_running: false,
            share_cancel: None,
            share_status: Value::Null,
            share_progress: None,
            preview_progress: None,
            last_media_log: Instant::now(),
            preview_status: Value::Null,
            message: "Waiting for OBS and your next encounter.".into(),
            log_errors: 0,
            tx,
            status,
            armed: false,
            shutting_down: false,
        })
    }
    fn now() -> i64 {
        Utc::now().timestamp_millis()
    }
    fn migrate_defaults(db: &Library, config: &mut Config) -> Result<()> {
        if db.setting("five_minute_inactivity_default")?.is_none() {
            if config.activity_timeout_minutes == 30 {
                config.activity_timeout_minutes = 5;
            }
            db.set_setting("five_minute_inactivity_default", &json!(true))?;
        }
        if db.setting("fast_export_default")?.is_none() {
            config.export_mode = CutMode::Fast;
            db.set_setting("fast_export_default", &json!(true))?;
        }
        Ok(())
    }
    pub(crate) fn snapshot(&self) -> Result<Value> {
        let segments = self.db.segments()?;
        let mut activities = self.db.list()?;
        for activity in &mut activities {
            activity["processing"] = self.processing_status(activity, &segments);
            activity["review_available"] = json!(
                matches!(activity["status"].as_str(), Some("pending" | "exporting"))
                    && activity["id"].as_str().is_some_and(|id| self
                        .retained_review_plan(id, &segments)
                        .ok()
                        .flatten()
                        .is_some())
            );
        }
        Ok(
            json!({"shutting_down":self.shutting_down,"shutdown_ready":self.shutting_down && !self.export_running && !self.share_running && !self.guard.stop_pending && !self.stop_when_started,"config":self.config,"connected":self.obs.is_some(),"ownership":self.guard.ownership,"armed":self.armed,"automatic_paused":self.automatic_paused,"recording_activity_ids":self.automatic_activities,"split_supported":self.split_supported,
   "setup":self.setup,"setup_completed":self.setup_completed,"credential_status":self.credential_status,
   "event_log":self.event_log.entries,"event_log_error":self.event_log.error,"last_log_event":self.last_log_event,"message":self.message,"log_errors":self.log_errors,"export_running":self.export_running,"recording_open":self.recording.is_some(),"preview":self.preview_status,"autostart":crate::autostart::enabled(),"obs_stats":self.obs_stats,
   "share_running":self.share_running,"share":self.share_status,"share_progress":self.share_progress.as_ref().map(|p|p.borrow().clone()),"share_directory":self.root.join("exports"),"activities":activities,"active":self.logs.values().filter_map(|s|s.detector.active.clone()).collect::<Vec<_>>(),
   "log_status":self.logs.iter().map(|(name,s)|json!({"installation_id":name,"name":s.detector.installation,"last_event_ms":s.detector.last_event_ms,"log_file":s.last_source,"active":s.detector.active.is_some(),"dungeon_exit_ms":s.detector.dungeon_exit_ms,"last_activity_ms":s.detector.inactivity_since(),"inactivity_deadline_ms":s.detector.inactivity_since().map(|last|last+i64::from(self.config.activity_timeout_minutes)*60_000)})).collect::<Vec<_>>(),
   "segments":segments.len()}),
        )
    }
    fn retained_review_plan(
        &self,
        id: &str,
        segments: &[Segment],
    ) -> Result<Option<owc_domain::ClipPlan>> {
        let activity = self.db.activity(id)?;
        let Some(end) = activity.end_ms else {
            return Ok(None);
        };
        let plan = self.db.export_plan(id)?.map_or_else(
            || {
                plan_clip(
                    &activity,
                    segments,
                    i64::from(self.config.pre_seconds) * 1000,
                    i64::from(self.config.post_seconds) * 1000,
                )
            },
            |(plan, _)| plan,
        );
        let closed_through = plan
            .sources
            .last()
            .map_or(0, |r| r.segment.start_ms + r.offset_ms + r.duration_ms);
        Ok(
            (!matches!(plan.coverage, Coverage::Gapped | Coverage::Unavailable)
                && !plan.sources.is_empty()
                && closed_through >= end
                && plan
                    .sources
                    .iter()
                    .all(|r| r.segment.closed && r.segment.path.is_file()))
            .then_some(plan),
        )
    }
    fn playback_offset(
        &self,
        id: &str,
        requested_origin: Option<i64>,
        final_origin: i64,
    ) -> Result<i64> {
        let Some(origin) = requested_origin else {
            return Ok(0);
        };
        anyhow::ensure!(
            origin == final_origin
                || (self.preview_status["key"] == id
                    && self.preview_status["status"] == "ready"
                    && self.preview_status["origin_ms"].as_i64() == Some(origin)),
            "Playback source changed; select the recording again"
        );
        origin
            .checked_sub(final_origin)
            .context("Invalid playback origin")
    }
    fn processing_status(&self, activity: &Value, segments: &[Segment]) -> Value {
        if !matches!(activity["status"].as_str(), Some("pending" | "exporting")) {
            return Value::Null;
        }
        if let Some(job) = &self.export_job
            && activity["id"] == job.id
        {
            return json!({"stage":if *job.cancel.borrow() {"cancelling"} else {"exporting"}, "title":if *job.cancel.borrow() {"Cancelling clip preparation"} else {"Saving clip"}, "detail":job.progress.borrow().clone(), "elapsed_seconds":job.started.elapsed().as_secs()});
        }
        let needed =
            activity["end_ms"].as_i64().unwrap_or(0) + i64::from(self.config.post_seconds) * 1000;
        let remaining = needed.saturating_sub(Self::now());
        if remaining > 0 {
            return json!({"stage":"post_roll", "title":"Post-roll", "detail":format!("Saving the last {} seconds after the encounter.", (remaining + 999) / 1000)});
        }
        if let Some(recording) = &self.recording
            && segments.iter().map(|s| s.end_ms).max().unwrap_or(0) < needed
        {
            if self.obs.is_none() {
                return json!({"stage":"reconnecting", "title":"Waiting for OBS", "detail":"Reconnecting to OBS before checking the remaining footage."});
            }
            if self.guard.ownership != Ownership::Managed {
                return json!({"stage":"observed_source", "title":"Waiting for OBS", "detail":"OBS is still recording. The companion does not own this recording and cannot stop it automatically. Stop recording in OBS so the remaining footage can be checked."});
            }
            if recording.invalid {
                return json!({"stage":"uncertain_source", "title":"Check OBS recording", "detail":"Recording timing was interrupted. Stop recording in OBS so the remaining footage can be checked."});
            }
            return json!({"stage":"source_open", "title":"Waiting for OBS to finish saving", "detail":"OBS is still writing the last part of this recording. Clip preparation starts when that file is finished."});
        }
        if self.export_running {
            json!({"stage":"queued", "title":"Queued", "detail":"Waiting for the current clip to finish saving."})
        } else {
            json!({"stage":"preparing", "title":"Preparing clip", "detail":"Checking the recorded footage before saving this clip."})
        }
    }
    fn start_setup_check(&mut self) {
        self.setup_generation += 1;
        let generation = self.setup_generation;
        self.last_setup_check = Instant::now();
        self.setup["checking"] = true.into();
        let config = self.config.clone();
        let client = self.obs.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let report = crate::readiness::inspect(&config, client.as_ref()).await;
            let (reply, _) = oneshot::channel();
            let _ = tx
                .send(Request {
                    data: json!({"action":"_setup_done","generation":generation,"report":report}),
                    reply,
                })
                .await;
        });
    }
    pub async fn run(mut self, mut rx: mpsc::Receiver<Request>) {
        self.event_log
            .push("Companion", "info", "Companion started");
        self.reconnect().await;
        self.start_setup_check();
        self.restart_log_watchers(true);
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            let reply = tokio::select! {
             req=rx.recv()=>{let Some(req)=req else{break};let result=self.command(req.data).await;if let Err(e)=&result{self.message=format!("{e:#}");}Some((req.reply,result))},
             event=self.bus_events.recv()=>{if let Some(event)=event && let Err(error)=self.capture_event(event).await { self.message=error.to_string(); } None},
             _=tick.tick()=>{if let Err(e)=self.tick().await{self.message=format!("{e:#}");tracing::warn!(error=%e,"engine tick");}None}
            };
            if self.message != self.logged_message {
                self.event_log.push("Companion", "info", &self.message);
                self.logged_message.clone_from(&self.message);
            }
            let connected = self.obs.is_some();
            if self.logged_connected != Some(connected) {
                self.event_log.push(
                    "OBS",
                    if connected { "info" } else { "warning" },
                    if connected {
                        "Connected"
                    } else {
                        "Disconnected; waiting to reconnect"
                    },
                );
                self.logged_connected = Some(connected);
            }
            if self.last_media_log.elapsed() >= Duration::from_secs(5) {
                if let Some(job) = &self.export_job {
                    self.event_log.push(
                        "FFmpeg",
                        "info",
                        &format!(
                            "Export {}: {} ({}s elapsed)",
                            job.id,
                            job.progress.borrow().as_str(),
                            job.started.elapsed().as_secs()
                        ),
                    );
                }
                if let Some(progress) = &self.share_progress {
                    self.event_log
                        .push("Export", "info", progress.borrow().as_str());
                }
                if let Some(progress) = &self.preview_progress {
                    self.event_log.push(
                        "FFmpeg",
                        "info",
                        &format!("Playback: {}", progress.borrow().as_str()),
                    );
                }
                self.last_media_log = Instant::now();
            }
            // Publish committed state before acknowledging a UI action.
            if let Ok(s) = self.snapshot() {
                self.status.send_if_modified(|current| {
                    if *current == s {
                        false
                    } else {
                        *current = s;
                        true
                    }
                });
            }
            if let Some((sender, result)) = reply {
                let _ = sender.send(result);
            }
        }
    }
    async fn start_capture(&mut self) -> Result<()> {
        if self.armed {
            bail!("already armed");
        }
        self.config.validate()?;
        let state = self.obs_request("GetRecordStatus", json!({})).await?;
        if state["outputActive"] == true {
            self.guard.reconcile(true);
            self.armed = true;
            self.message = "Observing existing recording; clips wait for source closure.".into();
        } else {
            if self.split_supported {
                let mode = self
                    .obs_request(
                        "GetProfileParameter",
                        json!({"parameterCategory":"Output", "parameterName":"Mode"}),
                    )
                    .await?;
                let split = self
                    .obs_request(
                        "GetProfileParameter",
                        json!({"parameterCategory":"AdvOut", "parameterName":"RecSplitFile"}),
                    )
                    .await?;
                self.split_supported = mode["parameterValue"].as_str() == Some("Advanced")
                    && split["parameterValue"]
                        .as_str()
                        .is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1");
            }
            let directory = self.obs_request("GetRecordDirectory", json!({})).await?;
            let dir = PathBuf::from(
                directory["recordDirectory"]
                    .as_str()
                    .context("OBS did not return recording directory")?,
            )
            .canonicalize()?;
            if dir != self.config.spool.canonicalize()? {
                bail!(
                    "OBS recording directory must match the dedicated spool; use Apply recording directory while idle"
                );
            }
            if fs2::available_space(&dir)? < u64::from(self.config.minimum_free_gib) * 1_073_741_824
            {
                bail!("insufficient free space to start recording");
            }
            self.select_recording_scene().await?;
            self.guard.reconcile(false);
            self.guard.arm(false);
            self.recording = Some(OpenRecording {
                session: Uuid::new_v4(),
                path: None,
                start_ms: Self::now(),
                managed: true,
                invalid: false,
            });
            if let Err(e) = self.obs_request("StartRecord", json!({})).await {
                let _ = self.restore_recording_scene().await;
                self.guard.lose_ownership();
                return Err(e);
            }
            self.armed = true;
            self.last_split = Instant::now();
            self.message = if self.split_supported {
                "Recording. Saving encounter clips automatically."
            } else {
                "Recording. Clips will be saved when recording stops."
            }
            .into();
        }
        Ok(())
    }
    async fn prepare_shutdown(&mut self) -> Result<()> {
        if self.shutting_down {
            return Ok(());
        }
        // Persist the stopped intervals before disabling input or closing the process.
        let mut logs = self.logs.clone();
        let mut completed = vec![];
        for log in logs.values_mut() {
            if log.detector.active.as_ref().is_some_and(|a| {
                a.kind == Kind::Manual
                    || self.automatic_activities.contains(&a.id)
                    || self.interrupted_activities.contains(&a.id)
            }) {
                let mut activity = log.detector.active.take().expect("active checked");
                activity.finish(
                    Self::now(),
                    if activity.kind == Kind::Manual {
                        Outcome::Unknown
                    } else {
                        Outcome::Abandoned
                    },
                );
                activity
                    .warnings
                    .push("Companion closed before this recording finished.".into());
                completed.push(activity);
            }
        }
        self.commit_logs(&serde_json::to_value(&logs)?, &completed)?;
        self.logs = logs;
        self.shutting_down = true;
        // Suppress fresh starts for this process without changing the user's preference.
        self.automatic_paused = true;
        self.log_generation = self.log_generation.wrapping_add(1);
        for worker in self.log_watchers.drain(..) {
            worker.abort();
        }
        self.deferred_starts.clear();
        self.automatic_activities.clear();
        self.automatic_stop = None;
        if let Some(job) = &self.export_job {
            job.cancel.send_replace(true);
        }
        if let Some(cancel) = &self.share_cancel {
            cancel.send_replace(true);
        }
        if let Err(error) = self.stop_capture().await {
            tracing::warn!(%error, "Could not confirm owned capture stop during shutdown");
        }
        self.message = "Closing. Unfinished clips will resume next time.".into();
        Ok(())
    }

    async fn stop_capture(&mut self) -> Result<()> {
        self.armed = false;
        self.stop_when_started = self.guard.ownership == Ownership::Starting;
        if self.guard.request_stop()
            && let Err(e) = self.obs_request("StopRecord", json!({})).await
        {
            self.guard.lose_ownership();
            return Err(e);
        }
        self.message = "Companion recording stopped. Any existing OBS recording continues.".into();
        self.automatic_session = false;
        Ok(())
    }
    async fn capture_event(&mut self, event: CaptureEvent) -> Result<()> {
        match event {
            CaptureEvent::Obs { generation, event } if generation == self.obs_generation => {
                match &event {
                    owc_obs_client::Event::Obs { kind, data, .. }
                        if matches!(
                            kind.as_str(),
                            "RecordStateChanged"
                                | "RecordFileChanged"
                                | "CurrentProgramSceneChanged"
                        ) =>
                    {
                        let detail = data["outputState"].as_str().unwrap_or("");
                        self.event_log
                            .push("OBS", "info", &format!("{kind} {detail}"));
                    }
                    owc_obs_client::Event::Disconnected(_) => {
                        self.event_log.push("OBS", "warning", "Connection lost");
                    }
                    owc_obs_client::Event::Obs { .. } => {}
                }
                self.event(event).await?;
            }
            CaptureEvent::Obs { .. } => {}
            CaptureEvent::Logs(batch) => self.log_batch(*batch).await?,
        }
        Ok(())
    }
    async fn activity_started(&mut self, activity: Activity) -> Result<()> {
        if self.automatic_paused
            || !self.setup_completed
            || self.obs.is_none()
            || self.setup["checks"]
                .as_array()
                .is_some_and(|checks| checks.iter().any(|check| check["status"] == "fail"))
        {
            return Ok(());
        }
        let now = Self::now();
        if activity.start_ms > now.saturating_add(10_000) {
            return Ok(());
        }
        if now.saturating_sub(activity.start_ms) > 10_000 {
            // Backlog may use footage already being captured, but must never start
            // a new OBS recording (or defer into one) for a historical encounter.
            // The planner labels missing pre-roll/start footage as truncated.
            let capturing = self.armed
                && self
                    .recording
                    .as_ref()
                    .is_some_and(|recording| !recording.invalid);
            if !capturing || self.guard.stop_pending || self.stop_when_started {
                return Ok(());
            }
        }
        if self.guard.stop_pending || self.stop_when_started {
            self.deferred_starts.push(activity);
            return Ok(());
        }
        if !self.armed {
            self.start_capture().await?;
            self.automatic_session = true;
        }
        self.automatic_activities.insert(activity.id);
        self.automatic_stop = None;
        self.message = format!("Recording {}", activity.name);
        Ok(())
    }
    fn activity_finished(&mut self, mut activity: Activity) {
        self.deferred_starts
            .retain(|pending| pending.id != activity.id);
        let tracked = self.automatic_activities.remove(&activity.id);
        let interrupted = self.interrupted_activities.remove(&activity.id);
        // A delayed completion can arrive after capture has stopped, or while
        // OBS is unavailable. Keep its metadata so the planner can recover closed
        // footage (or report unavailable), without acquiring capture ownership.
        let retain_metadata = self.setup_completed && !self.automatic_paused;
        if tracked || interrupted || retain_metadata || (self.armed && !self.automatic_session) {
            if interrupted || (tracked && !self.armed) {
                activity.warnings.push(
                    "Recording was interrupted before the encounter finished; footage may be incomplete or unavailable.".into(),
                );
            }
            self.completed_events.push(activity);
        }
        if tracked && self.automatic_session && self.automatic_activities.is_empty() {
            self.automatic_stop =
                Some(Instant::now() + Duration::from_secs(u64::from(self.config.post_seconds)));
        }
    }
    fn recover_manual(db: &mut Library, logs: &mut BTreeMap<String, LogState>) -> Result<()> {
        if let Some(activity) = logs
            .get_mut("manual")
            .and_then(|state| state.detector.active.take())
        {
            let mut activity = activity;
            let confirmed_end = db
                .segments()?
                .iter()
                .filter(|segment| segment.closed && segment.end_ms > activity.start_ms)
                .map(|segment| segment.end_ms)
                .max()
                .unwrap_or(activity.start_ms);
            let marker_end = activity
                .markers
                .iter()
                .map(|marker| marker.at_ms)
                .max()
                .unwrap_or(activity.start_ms);
            activity.finish(
                confirmed_end.max(marker_end).min(Self::now()),
                Outcome::Abandoned,
            );
            activity.warnings.push("Companion restarted during this manual clip; only confirmed closed footage can be recovered.".into());
            db.checkpoint("logs", &serde_json::to_value(&*logs)?, &[activity])?;
        }
        Ok(())
    }

    #[allow(clippy::needless_pass_by_ref_mut)] // Exclusive access keeps the actor future Send; SQLite is not Sync.
    async fn sync_sidecar(&mut self, id: &str) -> Result<()> {
        if let Some(record) = self
            .db
            .list()?
            .into_iter()
            .find(|record| record["id"] == id && record["status"] == "ready")
        {
            let video = PathBuf::from(record["path"].as_str().context("Recording path missing")?);
            let path = video.with_extension("json");
            let activity = self.db.activity(id)?;
            let data = match crate::sidecar::load(&path)? {
                Some(data) => data,
                None => {
                    json!({"schema_version":1,"activity":activity,"plan":self.db.export_plan(id)?.map(|(plan,_)|plan),"media":self.config.media.probe(&video).await?})
                }
            };
            crate::sidecar::update(&path, &activity, &record, data)?;
            self.db.sidecar_synced(id)?;
        }
        Ok(())
    }

    async fn add_marker(&mut self, v: &Value) -> Result<()> {
        let id: Uuid = v["id"].as_str().context("Select a recording")?.parse()?;
        let label = v["label"].as_str().unwrap_or("Marker").trim();
        if label.is_empty() || label.len() > 120 {
            bail!("Marker label must be 1 to 120 bytes");
        }
        if let Some(key) = self.logs.iter().find_map(|(key, state)| {
            state
                .detector
                .active
                .as_ref()
                .filter(|a| a.id == id)
                .map(|_| key.clone())
        }) {
            let mut logs = self.logs.clone();
            let activity = logs
                .get_mut(&key)
                .and_then(|s| s.detector.active.as_mut())
                .context("Activity ended")?;
            let now = Self::now();
            let at = v["at_ms"].as_i64().unwrap_or(now);
            if at < activity.start_ms || at > now {
                bail!("Marker is outside the active recording");
            }
            activity.marker(at, "manual", label.into());
            self.commit_logs(&serde_json::to_value(&logs)?, &[])?;
            self.logs = logs;
            if key != "manual" {
                self.restart_log_watchers(false);
            }
        } else {
            let path = self.db.media_path(&id.to_string())?;
            let position = v["position_ms"]
                .as_i64()
                .context("Playback position missing")?;
            let media = self.config.media.probe(&path).await?;
            let mut activity = self.db.activity(&id.to_string())?;
            let origin = self
                .db
                .export_plan(&id.to_string())?
                .and_then(|(p, _)| p.actual_start_ms)
                .unwrap_or(activity.start_ms);
            let offset = self.playback_offset(&id.to_string(), v["origin_ms"].as_i64(), origin)?;
            let position = position
                .checked_add(offset)
                .context("Invalid marker position")?;
            if position < 0 || position > media.duration_ms {
                bail!("Marker is outside the recording");
            }
            let at = origin
                .checked_add(position)
                .context("Invalid marker time")?;
            if !activity
                .markers
                .iter()
                .any(|m| m.kind == "manual" && m.at_ms == at && m.label == label)
            {
                activity.marker(at, "manual", label.into());
                self.db.save_activity(&activity)?;
            }
            self.sync_sidecar(&id.to_string()).await?;
        }
        Ok(())
    }

    fn stop_activity(&mut self, id: Uuid) -> Result<()> {
        let (key, state) = self
            .logs
            .iter()
            .find(|(_, state)| {
                state
                    .detector
                    .active
                    .as_ref()
                    .is_some_and(|a| a.id == id && a.kind != Kind::Manual)
            })
            .context("This encounter is no longer in progress")?;
        let key = key.clone();
        let installation = self
            .config
            .installations
            .iter()
            .find(|i| i.id.to_string() == key)
            .context("Installation no longer configured")?
            .clone();
        let mut next = state.clone();
        let mut activity = next.detector.active.take().context("No active encounter")?;
        activity.finish(Self::now(), Outcome::Abandoned);
        activity
            .warnings
            .push("Recording ended early by you.".into());
        let mut logs = self.logs.clone();
        logs.insert(key, next);
        self.commit_logs(
            &serde_json::to_value(&logs)?,
            std::slice::from_ref(&activity),
        )?;
        self.logs = logs;
        self.completed_events.clear();
        self.activity_finished(activity.clone());
        self.completed_events.retain(|a| a.id != id);
        self.last_log_event = Some(LogEvent {
            source: crate::log_watcher::Source {
                installation_id: installation.id,
                installation_name: installation.name,
                flavour: installation.flavour,
                logs_directory: installation.logs,
                log_file: self.logs[&installation.id.to_string()].last_source.clone(),
                line: None,
            },
            event: ActivityEvent::Abandoned {
                activity,
                reason: AbandonReason::User,
            },
        });
        // Invalidate queued worker snapshots so they cannot resurrect the abandoned run.
        self.restart_log_watchers(false);
        self.message = "Recording ended early. Waiting for your next encounter.".into();
        Ok(())
    }

    fn restart_log_watchers(&mut self, recover: bool) {
        for watcher in self.log_watchers.drain(..) {
            watcher.abort();
        }
        self.log_generation += 1;
        for installation in &self.config.installations {
            let state = self
                .logs
                .entry(installation.id.to_string())
                .or_insert_with(|| LogState::new(installation))
                .clone();
            self.log_watchers.push(crate::log_watcher::spawn(
                installation.clone(),
                state,
                self.log_generation,
                Duration::from_secs(u64::from(self.config.activity_timeout_minutes) * 60),
                recover,
                self.bus.clone(),
            ));
        }
    }
    fn record_log_event(&mut self, event: &LogEvent, installation: &str) {
        let detail = match &event.event {
            ActivityEvent::Started(a) => Some(format!("Started: {}", a.name)),
            ActivityEvent::Finished(a) => Some(format!("Finished: {} ({:?})", a.name, a.outcome)),
            ActivityEvent::Abandoned { activity, reason } => {
                Some(format!("Abandoned: {} ({reason:?})", activity.name))
            }
            ActivityEvent::Marker(m) if m.kind != "combat_activity" => {
                Some(format!("{}: {}", m.kind, m.label))
            }
            ActivityEvent::Diagnostic(message) => Some(message.clone()),
            _ => None,
        };
        if let Some(detail) = detail {
            self.event_log.push(installation, "info", &detail);
        }
    }
    fn record_watcher_state(
        &mut self,
        installation: &crate::config::Installation,
        batch: &LogBatch,
    ) {
        let previous_exit = self
            .logs
            .get(&installation.id.to_string())
            .and_then(|s| s.detector.dungeon_exit_ms);
        let next_exit = batch.state.detector.dungeon_exit_ms;
        if previous_exit != next_exit {
            self.event_log.push(
                &installation.name,
                "info",
                if next_exit.is_some() {
                    "Left dungeon; abandonment countdown started (60 seconds)"
                } else if batch.state.detector.active.is_some() {
                    "Dungeon exit countdown cancelled"
                } else {
                    "Dungeon exit countdown ended"
                },
            );
        }
        if batch.parse_errors > 0 {
            self.event_log.push(
                &installation.name,
                "warning",
                &format!("Skipped {} unparseable log lines", batch.parse_errors),
            );
        }
    }
    async fn log_batch(&mut self, batch: LogBatch) -> Result<()> {
        if batch.generation != self.log_generation {
            return Ok(());
        }
        let Some(installation) = self
            .config
            .installations
            .iter()
            .find(|i| i.id == batch.installation_id)
            .cloned()
        else {
            return Ok(());
        };
        self.record_watcher_state(&installation, &batch);
        self.logs.insert(installation.id.to_string(), batch.state);
        self.log_errors += batch.parse_errors;
        if let Some(error) = batch.error {
            self.message = format!("{}: {error}", installation.name);
        }
        for event in batch.events {
            if event.source.installation_id != installation.id {
                continue;
            }
            self.record_log_event(&event, &installation.name);
            self.last_log_event = Some(event.clone());
            let (activity, started, recovered) = match event.event {
                ActivityEvent::Started(activity) => (activity, true, false),
                ActivityEvent::Finished(activity) => (activity, false, false),
                ActivityEvent::Abandoned { activity, reason } => {
                    (activity, false, reason == AbandonReason::Restart)
                }
                ActivityEvent::Marker(marker) => {
                    if let Some(manual) = self
                        .logs
                        .get_mut("manual")
                        .and_then(|s| s.detector.active.as_mut())
                        && manual.installation_id == Some(installation.id)
                        && marker.at_ms >= manual.start_ms
                        && manual.end_ms.is_none_or(|end| marker.at_ms <= end)
                    {
                        manual.markers.push(marker);
                    }
                    continue;
                }
                ActivityEvent::Updated(_) | ActivityEvent::Diagnostic(_) => continue,
            };
            if recovered {
                self.completed_events.push(activity);
                continue;
            }
            if !installation.categories.contains(&activity.kind)
                || (!installation.difficulties.is_empty()
                    && activity
                        .difficulty
                        .is_some_and(|difficulty| !installation.difficulties.contains(&difficulty)))
                || activity
                    .key_level
                    .is_some_and(|key| key < installation.minimum_key)
            {
                continue;
            }
            if started {
                if let Err(error) = self.activity_started(activity).await {
                    self.message = error.to_string();
                }
            } else {
                self.activity_finished(activity);
            }
        }
        self.pending_log_acks.push(batch.committed);
        self.commit_logs(&serde_json::to_value(&self.logs)?, &[])?;
        Ok(())
    }
    fn commit_logs(&mut self, state: &Value, additional: &[Activity]) -> Result<()> {
        // Cursor writes from manual actions must also retain any failed watcher completions.
        let mut completed = self.completed_events.clone();
        completed.extend_from_slice(additional);
        self.db
            .checkpoint("logs", state, &completed)
            .context("Could not save combat-log progress; waiting to retry")?;
        self.completed_events.clear();
        for ack in self.pending_log_acks.drain(..) {
            let _ = ack.send(());
        }
        Ok(())
    }
    fn retry_log_checkpoint(&mut self) -> Result<()> {
        if !self.pending_log_acks.is_empty() {
            self.commit_logs(&serde_json::to_value(&self.logs)?, &[])?;
            self.message = "Combat-log saving recovered.".into();
        }
        Ok(())
    }
    async fn reconnect(&mut self) {
        self.last_reconnect = Instant::now();
        self.reconnect_delay = (self.reconnect_delay * 2).min(Duration::from_secs(30));
        let Ok(authentication_required) = owc_obs_client::probe(self.config.obs_port).await else {
            self.message = "Waiting for OBS. Reconnecting automatically.".into();
            return;
        };
        let password = if let Some(password) = &self.session_password {
            password.clone()
        } else if authentication_required {
            match crate::credentials::read(self.credential_scope.clone(), self.config.obs_port)
                .await
            {
                Ok(Some(password)) => password,
                Ok(None) => {
                    self.credential_status.clear();
                    return;
                }
                Err(error) => {
                    self.credential_status = error.to_string();
                    return;
                }
            }
        } else {
            String::new()
        };
        if self
            .command(json!({"action":"connect","password":password}))
            .await
            .is_ok()
        {
            self.credential_status = "Reconnected to OBS.".into();
        } else {
            self.credential_status =
                "Could not reconnect. Check the OBS password in connection settings.".into();
        }
    }
    async fn refresh_recording_checkpoint(&mut self) -> Result<()> {
        if self.guard.can_mutate() && !self.guard.split_pending {
            let state = self.obs_request("GetRecordStatus", json!({})).await?;
            self.checkpoint_recording(&state).await?;
        }
        Ok(())
    }
    async fn checkpoint_recording(&mut self, state: &Value) -> Result<()> {
        let Some(recording) = self
            .recording
            .clone()
            .filter(|r| r.managed && !r.invalid && r.path.is_some())
        else {
            return Ok(());
        };
        if self.guard.split_pending
            || state["outputPaused"] == true
            || state["outputActive"] != true
        {
            return Ok(());
        }
        let (Some(duration_ms), Some(bytes)) = (
            state["outputDuration"].as_i64(),
            state["outputBytes"].as_u64(),
        ) else {
            return Ok(());
        };
        let first = self
            .db
            .segments()?
            .into_iter()
            .filter(|s| s.session == recording.session)
            .min_by_key(|s| s.start_ms)
            .map(|s| s.path)
            .or_else(|| recording.path.clone());
        let outputs = self.obs_request("GetOutputList", json!({})).await?;
        for output in outputs["outputs"].as_array().into_iter().flatten() {
            if output["outputActive"] != true {
                continue;
            }
            let Some(name) = output["outputName"].as_str() else {
                continue;
            };
            let settings = self
                .obs_request("GetOutputSettings", json!({"outputName":name}))
                .await?;
            let Some(path) = settings["outputSettings"]["path"]
                .as_str()
                .map(PathBuf::from)
            else {
                continue;
            };
            if Some(&path) != first.as_ref() && Some(&path) != recording.path.as_ref() {
                continue;
            }
            let checkpoint = RecordingCheckpoint {
                recording,
                port: self.config.obs_port,
                output_name: name.into(),
                output_path: path,
                duration_ms,
                bytes,
                sampled_ms: Self::now(),
                automatic: self.automatic_session,
                scene_restore: self.scene_restore.clone(),
            };
            self.db
                .set_setting("recording_checkpoint", &serde_json::to_value(checkpoint)?)?;
            break;
        }
        Ok(())
    }
    async fn recover_recording(&mut self, client: &Client, state: &Value) -> Result<bool> {
        let Some(value) = self
            .db
            .setting("recording_checkpoint")?
            .filter(|v| !v.is_null())
        else {
            return Ok(false);
        };
        let checkpoint: RecordingCheckpoint = serde_json::from_value(value)?;
        let valid = checkpoint.port == self.config.obs_port
            && checkpoint.recording.managed
            && !checkpoint.recording.invalid
            && state["outputPaused"] != true;
        if !valid {
            self.db.set_setting("recording_checkpoint", &Value::Null)?;
            return Ok(false);
        }
        let settings = client
            .request(
                "GetOutputSettings",
                json!({"outputName":checkpoint.output_name}),
            )
            .await;
        if !settings.as_ref().is_ok_and(|s| {
            s["outputSettings"]["path"].as_str().map(PathBuf::from)
                == Some(checkpoint.output_path.clone())
        }) {
            self.db.set_setting("recording_checkpoint", &Value::Null)?;
            return Ok(false);
        }
        if state["outputActive"] != true {
            self.recording = Some(checkpoint.recording.clone());
            if let Some(path) = checkpoint.recording.path {
                self.close_file(path).await?;
            }
            self.recording = None;
            self.db.set_setting("recording_checkpoint", &Value::Null)?;
            return Ok(false);
        }
        if !checkpoint.continues(state, Self::now()) {
            self.db.set_setting("recording_checkpoint", &Value::Null)?;
            return Ok(false);
        }
        self.recording = Some(checkpoint.recording);
        self.guard = Guard {
            ownership: Ownership::Managed,
            ..Guard::default()
        };
        self.armed = true;
        self.automatic_session = checkpoint.automatic;
        self.scene_restore = checkpoint.scene_restore;
        self.automatic_activities = self
            .logs
            .values()
            .filter_map(|log| log.detector.active.as_ref())
            .filter(|a| a.kind != Kind::Manual)
            .map(|a| a.id)
            .collect();
        if self.automatic_session && self.automatic_activities.is_empty() {
            let deadline = self
                .db
                .list()?
                .iter()
                .filter_map(|a| a["end_ms"].as_i64())
                .max()
                .unwrap_or_else(Self::now)
                + i64::from(self.config.post_seconds) * 1000;
            self.automatic_stop = Some(
                Instant::now()
                    + Duration::from_millis(
                        u64::try_from(deadline.saturating_sub(Self::now()).max(0)).unwrap_or(0),
                    ),
            );
        }
        self.last_split = Instant::now();
        Ok(true)
    }
    async fn select_recording_scene(&mut self) -> Result<()> {
        self.scene_restore = None;
        let target = self.config.recording_scene.trim().to_owned();
        if target.is_empty() {
            return Ok(());
        }
        if self.obs_request("GetStreamStatus", json!({})).await?["outputActive"] == true {
            bail!("Stop streaming or clear the recording scene setting before starting recording");
        }
        let current = self
            .obs_request("GetCurrentProgramScene", json!({}))
            .await?;
        let previous = current["currentProgramSceneName"]
            .as_str()
            .context("OBS did not return its current scene")?
            .to_owned();
        if previous != target {
            self.obs_request("SetCurrentProgramScene", json!({"sceneName":target}))
                .await?;
            self.scene_restore = Some((previous, target));
        }
        Ok(())
    }
    async fn restore_recording_scene(&mut self) -> Result<()> {
        let Some((previous, selected)) = self.scene_restore.take() else {
            return Ok(());
        };
        if self.obs_request("GetStreamStatus", json!({})).await?["outputActive"] == true {
            return Ok(());
        }
        let current = self
            .obs_request("GetCurrentProgramScene", json!({}))
            .await?;
        if current["currentProgramSceneName"] == selected {
            self.obs_request("SetCurrentProgramScene", json!({"sceneName":previous}))
                .await?;
        }
        Ok(())
    }
    #[allow(clippy::needless_pass_by_ref_mut)] // Mutable borrow keeps the actor future Send with SQLite.
    async fn obs_request(&mut self, kind: &str, data: Value) -> Result<Value> {
        Ok(self
            .obs
            .as_ref()
            .context("not connected to OBS")?
            .request(kind, data)
            .await?)
    }
    #[allow(clippy::too_many_lines)] // Serialized command boundary; mutation guards are kept with actions.
    async fn command(&mut self, v: Value) -> Result<Value> {
        let action = v["action"].as_str().unwrap_or_default();
        if self.shutting_down && !action.starts_with('_') && action != "shutdown" {
            bail!("Companion is closing");
        }

        let action = v["action"].as_str().context("missing action")?;
        match action {
            "discover_installations" => {
                let folder = v["folder"]
                    .as_str()
                    .filter(|p| !p.trim().is_empty())
                    .map(PathBuf::from);
                if folder
                    .as_ref()
                    .is_some_and(|path| !path.is_absolute() || !path.is_dir())
                {
                    bail!("Choose an existing game folder using its full path");
                }
                let result =
                    tokio::task::spawn_blocking(move || crate::installations::discover(folder))
                        .await?;
                return Ok(serde_json::to_value(result)?);
            }
            "check_setup" => {
                if self.setup["checking"] != true {
                    self.start_setup_check();
                }
            }
            "_setup_done" => {
                if v["generation"].as_u64() == Some(self.setup_generation) {
                    self.setup = v["report"].clone();
                    self.setup["checking"] = false.into();
                }
            }
            "finish_setup" => {
                if self.obs.is_none()
                    || self.setup["connection"] != "connected"
                    || self.setup["checking"] == true
                {
                    bail!("Connect to OBS and wait for the readiness check before finishing setup");
                }
                if self.setup["checks"]
                    .as_array()
                    .is_none_or(|checks| checks.iter().any(|c| c["status"] == "fail"))
                {
                    bail!("Resolve the failed readiness checks before finishing setup");
                }
                if self.config.installations.is_empty() && v["manual_only"] != true {
                    bail!("Add a WoW Logs directory, or choose manual clips only");
                }
                self.automatic_paused = v["manual_only"] == true;
                self.db
                    .set_setting("automatic_paused", &json!(self.automatic_paused))?;
                self.db.set_setting("setup_completed", &json!(true))?;
                self.setup_completed = true;
                self.message = "Setup complete. Waiting for your next encounter.".into();
            }
            "open_obs_download" => crate::open_browser("https://obsproject.com/download")?,
            "autostart" => {
                crate::autostart::set(
                    v["enabled"]
                        .as_bool()
                        .context("enabled must be true or false")?,
                    &self.root,
                )?;
                self.message = "Startup preference saved.".into();
            }
            "list_scenes" => {
                return self.obs_request("GetSceneList", json!({})).await;
            }
            "connect" => {
                if self.guard.can_mutate() {
                    bail!("disarm owned capture before reconnecting");
                }
                let supplied = v["password"].as_str().unwrap_or_default();
                let password = if supplied.is_empty() && v["use_saved"] == true {
                    crate::credentials::read(self.credential_scope.clone(), self.config.obs_port)
                        .await?
                        .unwrap_or_default()
                } else {
                    supplied.to_owned()
                };
                let client = Client::connect(self.config.obs_port, &password).await?;
                let events = client.subscribe();
                let version = client.request("GetVersion", json!({})).await?;
                self.split_supported = version["availableRequests"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|r| r == "SplitRecordFile"));
                let state = client.request("GetRecordStatus", json!({})).await?;
                self.guard.reconcile(state["outputActive"] == true);
                let recovered = self.recover_recording(&client, &state).await?;
                if state["outputActive"] == true && !recovered {
                    self.recording = Some(OpenRecording {
                        session: Uuid::new_v4(),
                        path: None,
                        start_ms: Self::now() - state["outputDuration"].as_i64().unwrap_or(0),
                        managed: false,
                        invalid: state["outputPaused"] == true,
                    });
                }
                if state["outputActive"] != true {
                    self.recording = None;
                }
                if !recovered {
                    self.scene_restore = None;
                }
                self.session_password = Some(password.clone());
                self.reconnect_delay = Duration::from_secs(2);
                self.last_reconnect = Instant::now();
                if let Some(watcher) = self.obs_watcher.take() {
                    watcher.abort();
                }
                self.obs_generation += 1;
                let generation = self.obs_generation;
                let bus = self.bus.clone();
                self.obs_watcher = Some(bus.watch_obs(events, generation));
                self.obs = Some(client);
                self.start_setup_check();
                self.message = if recovered {
                    "Reconnected to the companion recording."
                } else {
                    "Connected. Existing recordings are observed without control."
                }
                .into();
                self.credential_status = if v["remember"] == true && !password.is_empty() {
                    match crate::credentials::save(
                        self.credential_scope.clone(),
                        self.config.obs_port,
                        password,
                    )
                    .await
                    {
                        Ok(()) => "Password saved in your system password store.".into(),
                        Err(error) => error.to_string(),
                    }
                } else {
                    "Connected for this session.".into()
                };
            }
            "forget_password" => {
                self.session_password = None;
                crate::credentials::forget(self.credential_scope.clone(), self.config.obs_port)
                    .await?;
                self.credential_status =
                    "Saved password removed. Your current connection stays open.".into();
            }
            "configure_output" => {
                let client = self.obs.as_ref().context("connect to OBS first")?;
                if client.output_active("GetRecordStatus").await?
                    || client.output_active("GetStreamStatus").await?
                    || client.output_active("GetReplayBufferStatus").await?
                {
                    bail!("stop OBS outputs before changing the recording directory");
                }
                self.obs_request(
                    "SetRecordDirectory",
                    json!({"recordDirectory":self.config.spool}),
                )
                .await?;
                self.start_setup_check();
                self.message="OBS recording directory set to the dedicated spool. Other OBS settings unchanged.".into();
            }
            "set_library_layout" => {
                let width = v["width"]
                    .as_u64()
                    .filter(|w| (240..=720).contains(w))
                    .context("Recording pane width must be between 240 and 720")?;
                let collapsed = v["collapsed"]
                    .as_bool()
                    .context("collapsed must be a boolean")?;
                let mut config = self.config.clone();
                config.recordings_width = u32::try_from(width)?;
                config.recordings_collapsed = collapsed;
                self.db
                    .set_setting("config", &serde_json::to_value(&config)?)?;
                self.config = config;
            }
            "set_marker_visibility" => {
                let visible = v["visible"]
                    .as_bool()
                    .context("visible must be a boolean")?;
                let mut config = self.config.clone();
                config.show_markers = visible;
                if let Some(types) = v.get("hidden_types") {
                    let types: Vec<String> = serde_json::from_value(types.clone())?;
                    anyhow::ensure!(
                        types.len() <= 100 && types.iter().all(|kind| kind.len() <= 100),
                        "Too many marker filters"
                    );
                    config.hidden_marker_types = types;
                }
                self.db
                    .set_setting("config", &serde_json::to_value(&config)?)?;
                self.config = config;
            }
            "save_config" => {
                if self.recording.is_some() || self.armed {
                    bail!("disarm and finish recording before changing settings");
                }
                let cfg: Config = serde_json::from_value(v["config"].clone())?;
                cfg.validate()?;
                std::fs::create_dir_all(&cfg.spool)?;
                std::fs::create_dir_all(&cfg.library)?;
                let a = cfg.spool.canonicalize()?;
                let b = cfg.library.canonicalize()?;
                if a.starts_with(&b) || b.starts_with(&a) {
                    bail!("spool and library directories must not contain each other");
                }
                self.db
                    .set_setting("config", &serde_json::to_value(&cfg)?)?;
                if self.config.obs_port != cfg.obs_port {
                    self.obs = None;
                    self.obs_generation += 1;
                    self.guard = Guard::default();
                    self.credential_status.clear();
                    self.session_password = None;
                    self.scene_restore = None;
                    self.reconnect_delay = Duration::from_secs(2);
                }
                self.config = cfg;
                self.start_setup_check();
                self.logs.retain(|id, _| {
                    id == "manual"
                        || self
                            .config
                            .installations
                            .iter()
                            .any(|i| i.id.to_string() == *id)
                });
                self.restart_log_watchers(false);
                self.message = "Settings saved.".into();
            }
            "shutdown" => self.prepare_shutdown().await?,
            "arm" => self.start_capture().await?,
            "disarm" => self.stop_capture().await?,
            "pause_automatic" => {
                self.automatic_paused = true;
                self.db.set_setting("automatic_paused", &json!(true))?;
                self.automatic_activities.clear();
                self.deferred_starts.clear();
                self.automatic_stop = None;
                self.stop_capture().await?;
            }
            "resume_automatic" => {
                self.automatic_paused = false;
                self.db.set_setting("automatic_paused", &json!(false))?;
                self.message = "Waiting for your next encounter.".into();
            }
            "add_marker" => self.add_marker(&v).await?,
            "abandon_key" | "stop_activity" => {
                let id: Uuid = v["id"]
                    .as_str()
                    .context("Select an active encounter")?
                    .parse()?;
                self.stop_activity(id)?;
            }
            "manual_start" => {
                let installation = if let Some(id) = v["installation_id"].as_str() {
                    let id: Uuid = id.parse().context("Invalid installation")?;
                    Some(
                        self.config
                            .installations
                            .iter()
                            .find(|i| i.id == id)
                            .context("Installation no longer configured")?
                            .clone(),
                    )
                } else {
                    match self.config.installations.as_slice() {
                        [] => None,
                        [only] => Some(only.clone()),
                        _ => bail!("Choose the WoW installation for this clip"),
                    }
                };
                if !self.armed {
                    self.start_capture().await?;
                    self.automatic_session = true;
                    self.automatic_stop = None;
                }
                let mut detector = Detector::new("manual".into(), owc_wow_log::Flavour::Retail);
                if self
                    .logs
                    .get("manual")
                    .is_some_and(|s| s.detector.active.is_some())
                {
                    bail!("manual interval already active");
                }
                detector.active = Some(Activity::new(
                    Kind::Manual,
                    v["name"].as_str().unwrap_or("Manual recording").into(),
                    Self::now(),
                    "manual",
                ));
                if let Some(installation) = installation
                    && let Some(activity) = &mut detector.active
                {
                    activity.installation_id = Some(installation.id);
                    activity.installation = installation.name;
                }
                let mut logs = self.logs.clone();
                logs.insert(
                    "manual".into(),
                    LogState {
                        tailer: Tailer::default(),
                        last_source: None,
                        last_combat_sample_ms: None,
                        death_recaps: owc_wow_log::DeathRecaps::default(),
                        detector,
                    },
                );
                self.commit_logs(&serde_json::to_value(&logs)?, &[])?;
                self.logs = logs;
            }
            "manual_stop" => {
                let mut logs = self.logs.clone();
                let mut a = logs
                    .get_mut("manual")
                    .and_then(|s| s.detector.active.take())
                    .context("no manual interval")?;
                a.finish(Self::now(), Outcome::Unknown);
                self.commit_logs(&serde_json::to_value(&logs)?, &[a])?;
                self.logs = logs;
                if self.automatic_session && self.automatic_activities.is_empty() {
                    self.automatic_stop = Some(
                        Instant::now() + Duration::from_secs(u64::from(self.config.post_seconds)),
                    );
                }
            }
            "annotate" => {
                let id = v["id"].as_str().context("missing id")?;
                let tags: Vec<String> = serde_json::from_value(v["tags"].clone())?;
                self.db.annotate(id, v["protected"] == true, &tags)?;
                self.sync_sidecar(id).await?;
            }
            "cancel_export" => {
                let id = v["id"].as_str().context("Select a recording")?;
                let job = self
                    .export_job
                    .as_ref()
                    .filter(|job| job.id == id)
                    .context("This recording is not being prepared")?;
                job.cancel.send_replace(true);
            }
            "retry" => self.db.retry(v["id"].as_str().context("missing id")?)?,
            "delete" => {
                let id = v["id"].as_str().context("missing id")?;
                if self.preview_running || self.export_running || self.share_running {
                    bail!("wait for media processing before deleting");
                }
                if let Some(path) = self.db.deletable_path(id)? {
                    self.remove_library_file(&path)?;
                }
                self.db.delete(id)?;
            }
            "preview" => {
                if self.preview_running {
                    return Ok(json!({"busy":true}));
                }
                let track = usize::try_from(v["track"].as_u64().unwrap_or(0))?;
                if action == "preview" {
                    let id = v["id"].as_str().context("missing id")?;
                    Uuid::parse_str(id)?;
                    if self.db.media_path(id).is_err() {
                        let plan = self
                            .retained_review_plan(id, &self.db.segments()?)?
                            .context("Waiting for continuous footage to close")?;
                        let key = id.to_owned();
                        let filename = format!("{}-track-{track}.mp4", Uuid::new_v4());
                        let dest = self.root.join("preview").join(&filename);
                        let mut media = self.config.media.clone();
                        let (progress, receiver) =
                            watch::channel("Preparing retained footage".to_owned());
                        media.progress = Some(progress);
                        self.preview_progress = Some(receiver);
                        self.preview_running = true;
                        self.preview_status = json!({"status":"generating","key":key,"track":track,"early":true,"origin_ms":plan.actual_start_ms});
                        self.event_log.push(
                            "FFmpeg",
                            "info",
                            "Preparing retained footage for review",
                        );
                        let tx = self.tx.clone();
                        tokio::spawn(async move {
                            let data = match media.preview_plan(&plan, &dest, track).await {
                                Ok(review) => {
                                    json!({"action":"_preview_done","track":track,"file":filename,"key":key,"early":true,"origin_ms":review.origin_ms,"duration_ms":review.probe.duration_ms,"source":review.probe,"mode":if review.converted {"transcode"} else {"remux"},"reason":"Playback from retained footage"})
                                }
                                Err(error) => {
                                    json!({"action":"_preview_done","track":track,"key":key,"error":format!("{error:#}")})
                                }
                            };
                            let (reply, _) = oneshot::channel();
                            let _ = tx.send(Request { data, reply }).await;
                        });
                        return Ok(json!({}));
                    }
                }
                let id = v["id"].as_str().context("missing id")?;
                Uuid::parse_str(id)?;
                let source = self.db.media_path(id)?;
                let key = id.to_owned();
                let origin = self
                    .db
                    .list()?
                    .into_iter()
                    .find(|a| a["id"] == id)
                    .and_then(|a| a["origin_ms"].as_i64())
                    .unwrap_or(0);
                let mut source_info = self.config.media.probe(&source).await?;
                if let Some(sidecar) = crate::sidecar::load(&source.with_extension("json"))? {
                    source_info.export =
                        serde_json::from_value(sidecar["media"]["export"].clone()).ok();
                }
                let source_duration_ms = source_info.duration_ms;
                let reason = v["reason"]
                    .as_str()
                    .unwrap_or("Selected audio track requires a playback copy")
                    .chars()
                    .take(500)
                    .collect::<String>();
                let conversion =
                    if source_info.video_codec == "h264" && source_info.pixel_format == "yuv420p" {
                        "remux"
                    } else {
                        "transcode"
                    };
                if v["direct"] == true && track == 0 {
                    self.preview_status = json!({"status":"ready","url":format!("/media/{key}"),"origin_ms":origin,"key":key,"duration_ms":source_duration_ms,"track":track,"direct":true,"source":source_info,"mode":"original"});
                    return Ok(json!({}));
                }
                let filename = format!("{key}-track-{track}.mp4");
                let dest = self.root.join("preview").join(&filename);
                let mut media = self.config.media.clone();
                let (progress, receiver) = watch::channel("Preparing playback".to_owned());
                media.progress = Some(progress);
                self.preview_progress = Some(receiver);
                self.event_log
                    .push("FFmpeg", "info", "Preparing playback copy");
                let tx = self.tx.clone();
                self.preview_running = true;
                self.preview_status = json!({"status":"generating","origin_ms":origin,"key":key,"track":track,"source":source_info,"mode":conversion,"reason":reason});
                tokio::spawn(async move {
                    let result = media.preview(&source, &dest, track).await;
                    let data = match result {
                        Ok(()) => {
                            json!({"action":"_preview_done","track":track,"file":filename,"origin_ms":origin,"key":key,"duration_ms":source_duration_ms,"source":source_info,"mode":conversion,"reason":reason})
                        }
                        Err(e) => {
                            json!({"action":"_preview_done","track":track,"error":format!("{e:#}"),"key":key,"source":source_info,"mode":conversion,"reason":reason})
                        }
                    };
                    let (reply, _) = oneshot::channel();
                    let _ = tx.send(Request { data, reply }).await;
                });
            }
            "_preview_done" => {
                self.preview_running = false;
                self.preview_progress = None;
                self.event_log.push(
                    "FFmpeg",
                    if v["error"].is_string() {
                        "warning"
                    } else {
                        "info"
                    },
                    if v["error"].is_string() {
                        "Playback preparation failed"
                    } else {
                        "Playback ready"
                    },
                );
                self.preview_status = if v["error"].is_string() {
                    json!({"status":"failed","track":v["track"],"error":v["error"],"key":v["key"],"source":v["source"],"mode":v["mode"],"reason":v["reason"]})
                } else {
                    json!({"status":"ready","track":v["track"],"early":v["early"],"url":format!("/preview/{}",v["file"].as_str().unwrap_or_default()),"origin_ms":v["origin_ms"],"key":v["key"],"duration_ms":v["duration_ms"],"source":v["source"],"mode":v["mode"],"reason":v["reason"]})
                };
            }
            "share_export" => self.start_share(&v)?,
            "cancel_share" => {
                if let Some(cancel) = &self.share_cancel {
                    cancel.send_replace(true);
                    self.share_status["status"] = "cancelling".into();
                }
            }
            "_share_done" => {
                self.share_running = false;
                self.share_cancel = None;
                self.share_progress = None;
                if !v["error"].is_string() {
                    self.db.save_latest_bundle(
                        v["id"].as_str().context("Missing share recording")?,
                        &v["details"],
                    )?;
                }
                self.share_status = if v["error"].is_string() {
                    json!({"status":if v["cancelled"] == true { "cancelled" } else { "failed" },"error":v["error"]})
                } else {
                    json!({"status":"ready","id":v["id"],"path":v["details"]["path"]})
                };
                self.event_log.push(
                    "Export",
                    if v["error"].is_string() {
                        "warning"
                    } else {
                        "info"
                    },
                    &self.share_status.to_string(),
                );
            }
            "trim" => {
                let id = v["id"].as_str().context("missing id")?;
                let path = self.db.media_path(id)?;
                let info = self.config.media.probe(&path).await?;
                let start = v["start_ms"].as_i64().context("start_ms required")?;
                let end = v["end_ms"].as_i64().context("end_ms required")?;
                let parent_origin = self
                    .db
                    .export_plan(id)?
                    .and_then(|(p, _)| p.actual_start_ms)
                    .unwrap_or(self.db.activity(id)?.start_ms);
                let offset = self.playback_offset(id, v["origin_ms"].as_i64(), parent_origin)?;
                let start = start
                    .checked_add(offset)
                    .context("Invalid trim start")?
                    .max(0);
                let end = end
                    .checked_add(offset)
                    .context("Invalid trim end")?
                    .min(info.duration_ms);
                if start < 0 || end <= start || end > info.duration_ms {
                    bail!("trim range outside recording");
                }
                if self.export_running {
                    bail!("wait for current export");
                }
                let parent = self.db.activity(id)?;
                let mut a = Activity::new(
                    Kind::Manual,
                    format!("{}: clip", parent.name),
                    start,
                    "clip",
                );
                a.finish(end, Outcome::Unknown);
                a.installation_id = parent.installation_id;
                let origin = self
                    .db
                    .export_plan(id)?
                    .and_then(|(plan, _)| plan.actual_start_ms)
                    .unwrap_or(parent.start_ms);
                a.markers = parent
                    .markers
                    .into_iter()
                    .filter_map(|mut marker| {
                        marker.at_ms = marker.at_ms.checked_sub(origin)?;
                        (marker.at_ms >= start && marker.at_ms <= end).then_some(marker)
                    })
                    .collect();
                let seg = Segment {
                    id: Uuid::new_v4(),
                    session: Uuid::new_v4(),
                    path,
                    start_ms: 0,
                    end_ms: info.duration_ms,
                    closed: true,
                    managed: false,
                };
                let mut plan = plan_clip(&a, &[seg], 0, 0);
                self.db.save_activity(&a)?;
                let mode = if v["precise"] == true {
                    CutMode::Precise
                } else {
                    CutMode::Fast
                };
                if matches!(mode, CutMode::Fast) {
                    self.config.media.align_fast_cut(&mut plan).await?;
                }
                self.start_export(a, plan, mode)?;
            }
            "_export_done" => {
                self.export_running = false;
                self.export_job = None;
                let id = v["id"].as_str().context("id")?;
                if let Some(error) = v["error"].as_str() {
                    self.db.fail(id, "failed", error)?;
                    if self.shutting_down {
                        self.db.retry(id)?;
                    }
                    self.message = format!("Export failed: {error}");
                } else {
                    self.db
                        .finish_export(id, usize::try_from(v["audio"].as_u64().unwrap_or(0))?)?;
                    self.sync_sidecar(id).await?;
                    self.message = "Recording ready.".into();
                }
            }
            "media_path" => {
                return Ok(json!({"path":self.db.media_path(v["id"].as_str().context("id")?)?}));
            }
            _ => bail!("unknown action"),
        }
        Ok(json!({"ok":true}))
    }
    fn start_share(&mut self, v: &Value) -> Result<()> {
        anyhow::ensure!(!self.share_running, "Wait for the current sharing export");
        let id = v["id"].as_str().context("Select a recording")?;
        let source = self.db.media_path(id)?;
        let activity = self.db.activity(id)?;
        let record = self
            .db
            .list()?
            .into_iter()
            .find(|a| a["id"] == id)
            .context("Recording not found")?;
        let origin = record["origin_ms"].as_i64().unwrap_or(activity.start_ms);
        let offset = self.playback_offset(id, v["origin_ms"].as_i64(), origin)?;
        let range = if v["start_ms"].is_number() || v["end_ms"].is_number() {
            Some((
                v["start_ms"]
                    .as_i64()
                    .context("Start missing")?
                    .checked_add(offset)
                    .context("Invalid start")?
                    .max(0),
                v["end_ms"]
                    .as_i64()
                    .context("End missing")?
                    .checked_add(offset)
                    .context("Invalid end")?,
            ))
        } else {
            None
        };
        let (cancel, cancelled) = watch::channel(false);
        let request = crate::sharing::ShareRequest {
            cancel: cancelled,
            activity,
            source,
            origin_ms: origin,
            range,
            tracks: serde_json::from_value(v["tracks"].clone())
                .context("Choose export audio tracks")?,
            tags: serde_json::from_value(record["tags"].clone()).unwrap_or_default(),
            destination: PathBuf::from(v["path"].as_str().context("Choose a ZIP destination")?),
        };
        let (progress, receiver) = watch::channel("Preparing sharing bundle".to_owned());
        self.share_running = true;
        self.share_cancel = Some(cancel.clone());
        self.share_progress = Some(receiver);
        self.share_status = json!({"status":"generating","id":id});
        let media = self.config.media.clone();
        let tx = self.tx.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            let data = match crate::sharing::export(request, media, progress).await {
                Ok(details) => json!({"action":"_share_done","id":id,"details":details}),
                Err(error) => {
                    json!({"action":"_share_done","cancelled":*cancel.borrow(),"error":format!("{error:#}")})
                }
            };
            let (reply, _) = oneshot::channel();
            let _ = tx.send(Request { data, reply }).await;
        });
        Ok(())
    }
    fn remove_library_file(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let canonical = path.canonicalize()?;
        if canonical.parent() != Some(self.config.library.canonicalize()?.as_path()) {
            bail!("refusing to delete a file outside the library");
        }
        std::fs::remove_file(&canonical)?;
        let _ = std::fs::remove_file(canonical.with_extension("json"));
        Ok(())
    }
    fn start_export(
        &mut self,
        mut a: Activity,
        plan: owc_domain::ClipPlan,
        mode: CutMode,
    ) -> Result<()> {
        if plan.coverage == Coverage::Truncated {
            let warning =
                "Requested history is unavailable; this clip contains only retained footage."
                    .to_owned();
            if !a.warnings.contains(&warning) {
                a.warnings.push(warning);
            }
        }
        if a.installation != "clip" && plan.sources.iter().any(|source| !source.segment.managed) {
            let warning =
                "Observed recording timing is estimated; verify marker alignment.".to_owned();
            if !a.warnings.contains(&warning) {
                a.warnings.push(warning);
            }
        }
        self.db.save_activity(&a)?;
        let dest = self.config.library.join(format!("{}.mkv", a.id));
        self.db
            .begin_export(&plan, &dest, matches!(mode, CutMode::Precise))?;
        self.export_running = true;
        let mut media = self.config.media.clone();
        let (progress, updates) = watch::channel("Preparing footage".to_owned());
        media.progress = Some(progress);
        let (cancel, cancelled) = watch::channel(false);
        media.cancel = Some(cancelled);
        self.export_job = Some(ExportJob {
            cancel,
            id: a.id.to_string(),
            started: Instant::now(),
            progress: updates,
        });
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = async {
                let p = media.export(&plan, &dest, mode).await?;
                crate::sidecar::write(
                    &dest.with_extension("json"),
                    &json!({"schema_version":1,"activity":a,"plan":plan,"media":p}),
                )?;
                Ok::<_, anyhow::Error>(p)
            }
            .await;
            let data = match result {
                Ok(p) => json!({"action":"_export_done","id":a.id,"audio":p.audio_tracks}),
                Err(e) => json!({"action":"_export_done","id":a.id,"error":format!("{e:#}")}),
            };
            let (reply, _) = oneshot::channel();
            let _ = tx.send(Request { data, reply }).await;
        });
        Ok(())
    }
    async fn close_file(&mut self, path: PathBuf) -> Result<()> {
        let Some(r) = self.recording.as_mut() else {
            return Ok(());
        };
        if r.invalid {
            self.message =
                "Recording timing was interrupted; automatic extraction withheld.".into();
            return Ok(());
        }
        if self
            .db
            .segments()?
            .iter()
            .any(|segment| segment.path == path)
        {
            self.message =
                "OBS reported an already indexed file; waiting for verified source recovery."
                    .into();
            return Ok(());
        }
        let info = self.config.media.probe(&path).await?;
        let managed = r.managed
            && path.canonicalize()?.parent() == Some(self.config.spool.canonicalize()?.as_path());
        let seg = Segment {
            id: Uuid::new_v4(),
            session: r.session,
            path,
            start_ms: r.start_ms,
            end_ms: r.start_ms + info.duration_ms,
            closed: true,
            managed,
        };
        // File durations, rather than delivery times of split events, advance the media timeline.
        r.start_ms = seg.end_ms;
        self.db.add_segment(&seg)?;
        Ok(())
    }
    fn disconnected(&mut self) {
        self.stop_when_started = false;
        self.interrupted_activities
            .extend(self.automatic_activities.drain());
        self.deferred_starts.clear();
        self.automatic_stop = None;
        self.automatic_session = false;
        self.scene_restore = None;
        self.last_reconnect = Instant::now();
        self.reconnect_delay = Duration::from_secs(2);
        self.obs = None;
        self.obs_generation += 1;
        self.setup_generation += 1;
        self.setup = json!({"connection":"unreachable","checks":[],"checking":false});
        self.guard.lose_ownership();
        self.armed = false;
        if let Some(r) = &mut self.recording {
            r.invalid = true;
        }
        self.message = "Waiting for OBS. Reconnecting automatically.".into();
    }
    #[allow(clippy::too_many_lines)] // OBS event transitions stay together to make ownership invalidation auditable.
    async fn event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Disconnected(_reason) => {
                self.disconnected();
            }
            Event::Obs {
                kind,
                data,
                received_ms,
            } => match kind.as_str() {
                "RecordFileChanged" => {
                    let next = PathBuf::from(
                        data["newOutputPath"]
                            .as_str()
                            .context("missing recording path")?,
                    );
                    let old = self.recording.as_ref().and_then(|r| r.path.clone());
                    if let Some(old) = old {
                        self.close_file(old).await?;
                        self.guard.file_changed();
                    }
                    if let Some(r) = &mut self.recording {
                        r.path = Some(next);
                        r.managed &= self.guard.can_mutate();
                    }
                    self.last_split = Instant::now();
                    self.refresh_recording_checkpoint().await?;
                }
                "RecordStateChanged" => match data["outputState"].as_str().unwrap_or_default() {
                    "OBS_WEBSOCKET_OUTPUT_STARTED" => {
                        if self.recording.is_none() {
                            self.recording = Some(OpenRecording {
                                session: Uuid::new_v4(),
                                path: None,
                                start_ms: Self::now(),
                                managed: false,
                                invalid: false,
                            });
                        }
                        self.guard.started();
                        if self.stop_when_started {
                            self.stop_capture().await?;
                        }
                        if let Some(r) = &mut self.recording {
                            // Output duration counts encoded frames and lags wall time; anchor to event receipt.
                            r.start_ms = received_ms;
                            r.invalid |= received_ms <= 0;
                        }
                        if let Some(r) = &mut self.recording
                            && let Some(path) = data["outputPath"].as_str()
                        {
                            r.path = Some(path.into());
                        }
                        self.refresh_recording_checkpoint().await?;
                    }
                    "OBS_WEBSOCKET_OUTPUT_STOPPED" => {
                        // OBS 32 reports the original file here even after splitting.
                        let path = self
                            .recording
                            .as_ref()
                            .and_then(|r| r.path.clone())
                            .or_else(|| data["outputPath"].as_str().map(PathBuf::from));
                        if let Some(p) = path {
                            self.close_file(p).await?;
                        }
                        self.recording = None;
                        self.db.set_setting("recording_checkpoint", &Value::Null)?;
                        self.restore_recording_scene().await?;
                        self.guard.stopped();
                        self.armed = false;
                    }
                    "OBS_WEBSOCKET_OUTPUT_PAUSED" | "OBS_WEBSOCKET_OUTPUT_RESUMED" => {
                        self.guard.lose_ownership();
                        self.db.set_setting("recording_checkpoint", &Value::Null)?;
                        self.armed = false;
                        if let Some(r) = &mut self.recording {
                            r.invalid = true;
                        }
                        self.message =
                            "OBS pause/resume changed timing; capture is now observe-only.".into();
                    }
                    _ => {}
                },
                "CurrentProgramSceneChanged"
                    if self
                        .scene_restore
                        .as_ref()
                        .is_some_and(|(_, target)| data["sceneName"] != *target) =>
                {
                    self.scene_restore = None;
                }
                "CurrentProfileChanging" | "CurrentSceneCollectionChanging" => {
                    self.db.set_setting("recording_checkpoint", &Value::Null)?;
                    self.scene_restore = None;
                    self.setup_generation += 1;
                    self.setup = json!({"connection":"changed","checks":[],"checking":false});
                    self.guard.lose_ownership();
                    self.armed = false;
                    if let Some(r) = &mut self.recording {
                        r.invalid = true;
                    }
                }
                _ => {}
            },
        }
        Ok(())
    }
    #[allow(clippy::too_many_lines)]
    async fn tick(&mut self) -> Result<()> {
        if self.shutting_down {
            return self.retry_log_checkpoint();
        }
        if let Err(error) = self.retry_log_checkpoint() {
            self.message = error.to_string();
            tracing::warn!(%error, "Combat-log checkpoint retry failed");
        }
        if self.obs.is_none() && self.last_reconnect.elapsed() >= self.reconnect_delay {
            self.reconnect().await;
        }
        let wall = Self::now();
        let monotonic_ms = i64::try_from(self.last_clock.0.elapsed().as_millis())?;
        if (wall - self.last_clock.1 - monotonic_ms).abs() > 1000 {
            if let Some(recording) = &mut self.recording {
                recording.invalid = true;
            }
            self.guard.lose_ownership();
            self.db.set_setting("recording_checkpoint", &Value::Null)?;
            self.armed = false;
            self.message = "System clock changed; recording timing is uncertain. OBS is left running; reconnect after stopping it.".into();
        }
        self.last_clock = (Instant::now(), wall);
        if self.last_setup_check.elapsed() > Duration::from_secs(15)
            && self.setup["checking"] != true
        {
            self.start_setup_check();
        }
        while let Ok(event) = self.bus_events.try_recv() {
            self.capture_event(event).await?;
        }
        if !self.guard.stop_pending && !self.stop_when_started && self.recording.is_none() {
            for activity in std::mem::take(&mut self.deferred_starts) {
                self.activity_started(activity).await?;
            }
        }
        let now = Self::now();
        if self.guard.ownership == Ownership::Starting
            && self.last_split.elapsed() > Duration::from_secs(10)
        {
            self.guard.lose_ownership();
            self.armed = false;
            self.message =
                "OBS start confirmation timed out; checking state before further control.".into();
        }
        if self.automatic_session
            && self.automatic_activities.is_empty()
            && self
                .automatic_stop
                .is_some_and(|deadline| Instant::now() >= deadline)
            && self
                .logs
                .get("manual")
                .is_none_or(|state| state.detector.active.is_none())
        {
            self.automatic_stop = None;
            self.stop_capture().await?;
            self.message = "Waiting for your next encounter.".into();
        }
        if self.guard.can_mutate()
            && self.split_supported
            && self.last_split.elapsed()
                > Duration::from_secs(u64::from(self.config.segment_seconds))
            && self.guard.request_split()
        {
            self.db.set_setting("recording_checkpoint", &Value::Null)?;
            if let Err(e) = self.obs_request("SplitRecordFile", json!({})).await {
                self.guard.lose_ownership();
                self.message = format!("Split failed: {e}; recording left running.");
            }
            self.last_split = Instant::now();
        }
        if self.last_poll.elapsed() > Duration::from_secs(3) {
            self.last_poll = Instant::now();
            if self.obs.is_some() {
                if let Ok(stats) = self.obs_request("GetStats", json!({})).await {
                    self.obs_stats = stats;
                }
                match self.obs_request("GetRecordStatus", json!({})).await {
                    Ok(v) => {
                        if v["outputActive"] == true && self.guard.can_mutate() {
                            self.checkpoint_recording(&v).await?;
                        }
                        if v["outputActive"] != true
                            && self.recording.is_some()
                            && self.last_split.elapsed() > Duration::from_secs(10)
                        {
                            self.guard.reconcile(false);
                            self.recording = None;
                            self.armed = false;
                            self.message = "OBS did not confirm recording. Check its output error dialog and encoder settings.".into();
                        }
                        if v["outputPaused"] == true {
                            self.guard.lose_ownership();
                            if let Some(r) = &mut self.recording {
                                r.invalid = true;
                            }
                        }
                    }
                    Err(e) => {
                        self.guard.lose_ownership();
                        self.message = format!("OBS status unavailable: {e}");
                    }
                }
            }
            let mut pending_sidecars = self.db.pending_sidecars()?;
            if !pending_sidecars.is_empty() {
                pending_sidecars.sort();
                let offset = self.sidecar_cursor % pending_sidecars.len();
                pending_sidecars.rotate_left(offset);
                self.sidecar_cursor = self.sidecar_cursor.wrapping_add(8);
            }
            for id in pending_sidecars.into_iter().take(8) {
                if let Err(error) = self.sync_sidecar(&id).await {
                    self.message = format!("Metadata update pending; will retry: {error:#}");
                    tracing::warn!(%id, %error, "sidecar reconciliation pending");
                }
            }
            self.cleanup().await?;
        }
        if !self.export_running {
            let segments = self.db.segments()?;
            for mut a in self.db.pending()? {
                if let Some((plan, precise)) = self.db.export_plan(&a.id.to_string())? {
                    self.start_export(
                        a,
                        plan,
                        if precise {
                            CutMode::Precise
                        } else {
                            CutMode::Fast
                        },
                    )?;
                    break;
                }
                let Some(end) = a.end_ms else { continue };
                let needed = end + i64::from(self.config.post_seconds) * 1000;
                if needed > now {
                    continue;
                }
                if self.recording.is_some()
                    && segments.iter().map(|s| s.end_ms).max().unwrap_or(0) < needed
                {
                    continue;
                }
                let mut plan = plan_clip(
                    &a,
                    &segments,
                    i64::from(self.config.pre_seconds) * 1000,
                    i64::from(self.config.post_seconds) * 1000,
                );
                if matches!(plan.coverage, Coverage::Gapped | Coverage::Unavailable) {
                    self.db.fail(
                        &a.id.to_string(),
                        "unavailable",
                        "Source timeline has gaps or no retained footage",
                    )?;
                    continue;
                }
                let mut mode = self.config.export_mode;
                if matches!(mode, CutMode::Fast)
                    && let Err(error) = self.config.media.align_fast_cut(&mut plan).await
                {
                    mode = CutMode::Precise;
                    a.warnings.push(format!(
                        "Fast start alignment failed; using precise cuts: {error:#}"
                    ));
                }
                self.start_export(a, plan, mode)?;
                break;
            }
        }
        Ok(())
    }
    async fn cleanup(&mut self) -> Result<()> {
        let records = self.db.list()?;
        let pending_start = records
            .iter()
            .filter(|a| a["status"] != "ready")
            .filter_map(|a| a["start_ms"].as_i64())
            .chain(
                self.logs
                    .values()
                    .filter_map(|s| s.detector.active.as_ref().map(|a| a.start_ms)),
            )
            .min();
        let history = Self::now() - i64::from(self.config.history_seconds) * 1000;
        let retain_from = pending_start.map_or(history, |s| {
            history.min(s - i64::from(self.config.pre_seconds) * 1000)
        });
        let spool = self.config.spool.canonicalize()?;
        let mut bytes = 0_u64;
        for s in self.db.segments()? {
            let Ok(meta) = std::fs::metadata(&s.path) else {
                continue;
            };
            if s.managed {
                let canonical = s.path.canonicalize()?;
                if canonical.parent() == Some(spool.as_path())
                    && s.end_ms < retain_from
                    && !self.preview_running
                    && !self.share_running
                {
                    std::fs::remove_file(&canonical)?;
                    self.db.remove_segment(&s.id.to_string())?;
                } else {
                    bytes = bytes.saturating_add(meta.len());
                }
            }
        }
        if let Some(path) = self.recording.as_ref().and_then(|r| r.path.as_ref()) {
            bytes = bytes.saturating_add(std::fs::metadata(path).map_or(0, |m| m.len()));
        }
        if (bytes > u64::from(self.config.spool_gib) * 1_073_741_824
            || fs2::available_space(&spool)?
                < u64::from(self.config.minimum_free_gib) * 1_073_741_824)
            && self.guard.request_stop()
        {
            self.obs_request("StopRecord", json!({})).await?;
            self.armed = false;
            self.message =
                "Capture stopped: storage budget reached; pinned footage preserved.".into();
        }
        let mut total = records
            .iter()
            .filter(|a| a["status"] == "ready")
            .filter_map(|a| a["path"].as_str())
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum::<u64>();
        for a in records.iter().rev() {
            if total <= u64::from(self.config.library_gib) * 1_073_741_824 {
                break;
            }
            if a["status"] != "ready"
                || a["protected"] == true
                || self.preview_running
                || self.share_running
                || self.export_running
            {
                continue;
            }
            let Some(id) = a["id"].as_str() else { continue };
            let Ok(Some(p)) = self.db.deletable_path(id) else {
                continue;
            };
            let size = std::fs::metadata(&p).map_or(0, |m| m.len());
            self.remove_library_file(&p)?;
            self.db.delete(id)?;
            total = total.saturating_sub(size);
        }
        self.cleanup_previews()?;
        Ok(())
    }
    fn cleanup_previews(&self) -> Result<()> {
        if self.preview_running {
            return Ok(());
        }
        let directory = self.root.join("preview").canonicalize()?;
        let current = self.preview_status["url"]
            .as_str()
            .and_then(|url| url.strip_prefix("/preview/"));
        let mut files = vec![];
        for item in std::fs::read_dir(&directory)? {
            let item = item?;
            let name = item.file_name();
            let Some(name) = name.to_str() else { continue };
            if name
                .strip_prefix(".review-")
                .is_some_and(|id| Uuid::parse_str(id).is_ok())
                && item.file_type()?.is_dir()
            {
                std::fs::remove_dir_all(item.path())?;
                continue;
            }
            let Some((id, track)) = name
                .strip_suffix(".mp4")
                .and_then(|name| name.split_once("-track-"))
            else {
                continue;
            };
            if Uuid::parse_str(id).is_err()
                || track.trim_end_matches(".partial").parse::<usize>().is_err()
                || item.file_type()?.is_symlink()
            {
                continue;
            }
            let metadata = item.metadata()?;
            if Some(name) != current && track.ends_with(".partial") {
                std::fs::remove_file(item.path())?;
                continue;
            }
            files.push((
                metadata.modified()?,
                metadata.len(),
                item.path(),
                Some(name) == current,
            ));
        }
        files.sort_by_key(|item| item.0);
        let mut bytes: u64 = files.iter().map(|item| item.1).sum();
        let limit = (u64::from(self.config.library_gib) * 1_073_741_824 / 4).min(512 * 1_048_576);
        for (_, size, path, active) in files {
            if bytes <= limit {
                break;
            }
            if !active {
                std::fs::remove_file(path)?;
                bytes = bytes.saturating_sub(size);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn test_video(media: &owc_media::Media, path: &Path) -> Result<owc_media::Probe> {
        let status = tokio::process::Command::new(&media.ffmpeg)
            .args([
                "-nostdin",
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=30",
                "-t",
                "3",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(path)
            .status()
            .await?;
        anyhow::ensure!(status.success(), "test fixture encoding failed");
        media.probe(path).await
    }

    #[test]
    fn retained_review_requires_closed_continuous_footage_and_preserves_preferences() {
        let root = tempfile::tempdir().expect("root");
        let make_engine = || {
            let (tx, _) = mpsc::channel(32);
            let (status, _) = watch::channel(Value::Null);
            Engine::new(root.path().into(), tx, status).expect("engine")
        };
        let mut engine = make_engine();
        assert!(matches!(engine.config.export_mode, CutMode::Fast));
        engine.config.export_mode = CutMode::Precise;
        engine
            .db
            .set_setting(
                "config",
                &serde_json::to_value(&engine.config).expect("config"),
            )
            .expect("save");
        drop(engine);
        let mut engine = make_engine();
        assert!(matches!(engine.config.export_mode, CutMode::Precise));
        engine.config.pre_seconds = 0;
        engine.config.post_seconds = 0;
        let mut activity = Activity::new(Kind::Raid, "Review".into(), 1000, "retail");
        activity.finish(5000, Outcome::Success);
        let id = activity.id.to_string();
        engine.db.save_activity(&activity).expect("save");
        let path = root.path().join("retained.mkv");
        std::fs::write(&path, b"fixture").expect("file");
        let mut segments = vec![Segment {
            id: Uuid::new_v4(),
            session: Uuid::new_v4(),
            path,
            start_ms: 0,
            end_ms: 5000,
            closed: false,
            managed: true,
        }];
        assert!(
            engine
                .retained_review_plan(&id, &segments)
                .expect("plan")
                .is_none()
        );
        segments[0].closed = true;
        assert!(
            engine
                .retained_review_plan(&id, &segments)
                .expect("plan")
                .is_some()
        );
        segments[0].end_ms = 4000;
        assert!(
            engine
                .retained_review_plan(&id, &segments)
                .expect("plan")
                .is_none()
        );
        let mut second = segments[0].clone();
        second.start_ms = 4200;
        second.end_ms = 5000;
        segments.push(second);
        assert!(
            engine
                .retained_review_plan(&id, &segments)
                .expect("plan")
                .is_none()
        );
        engine.preview_status = json!({"key":id,"status":"ready","origin_ms":500});
        assert_eq!(
            engine
                .playback_offset(&id, Some(500), 1000)
                .expect("offset"),
            -500
        );
        assert!(engine.playback_offset(&id, Some(123), 1000).is_err());
        assert!(engine.playback_offset("another", Some(500), 1000).is_err());
    }

    #[tokio::test]
    async fn pending_activity_can_prepare_retained_playback_before_final_export() {
        let root = tempfile::tempdir().expect("root");
        let (tx, mut rx) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().into(), tx, status).expect("engine");
        engine.config.pre_seconds = 0;
        engine.config.post_seconds = 0;
        let video = root.path().join("retained.mkv");
        let probe = test_video(&engine.config.media, &video)
            .await
            .expect("video");
        let mut activity = Activity::new(Kind::Raid, "Early review".into(), 0, "retail");
        activity.finish(probe.duration_ms, Outcome::Success);
        engine.db.save_activity(&activity).expect("activity");
        engine
            .db
            .add_segment(&Segment {
                id: Uuid::new_v4(),
                session: Uuid::new_v4(),
                path: video,
                start_ms: 0,
                end_ms: probe.duration_ms,
                closed: true,
                managed: true,
            })
            .expect("segment");
        assert_eq!(
            engine.snapshot().expect("snapshot")["activities"][0]["review_available"],
            true
        );
        engine
            .command(json!({"action":"preview","id":activity.id,"track":0}))
            .await
            .expect("preview");
        assert!(engine.preview_running);
        assert_eq!(engine.preview_status["early"], true);
        let request = tokio::time::timeout(Duration::from_secs(30), rx.recv())
            .await
            .expect("worker timeout")
            .expect("worker result");
        engine.command(request.data).await.expect("completion");
        assert!(!engine.preview_running);
        assert_eq!(
            engine.preview_status["status"], "ready",
            "{}",
            engine.preview_status
        );
        assert_eq!(engine.preview_status["early"], true);
        assert!(
            engine.db.media_path(&activity.id.to_string()).is_err(),
            "final export should still be pending"
        );
        let file = engine.preview_status["url"]
            .as_str()
            .expect("url")
            .trim_start_matches("/preview/");
        assert!(root.path().join("preview").join(file).is_file());
    }

    #[tokio::test]
    async fn library_layout_persists_without_interrupting_capture() {
        let root = tempfile::tempdir().expect("root");
        let create = || {
            let (tx, _) = mpsc::channel(32);
            let (status, _) = watch::channel(Value::Null);
            Engine::new(root.path().into(), tx, status).expect("engine")
        };
        let mut engine = create();
        engine.armed = true;
        engine
            .command(json!({"action":"set_library_layout", "width":410, "collapsed":true}))
            .await
            .expect("save layout");
        assert!(engine.armed);
        assert!(
            engine
                .command(json!({"action":"set_library_layout", "width":40, "collapsed":true}))
                .await
                .is_err()
        );
        drop(engine);
        let restored = create();
        assert_eq!(restored.config.recordings_width, 410);
        assert!(restored.config.recordings_collapsed);
    }
    #[tokio::test]
    async fn marker_visibility_defaults_on_and_survives_restart() {
        let root = tempfile::tempdir().expect("root");
        let create = || {
            let (tx, _) = mpsc::channel(32);
            let (status, _) = watch::channel(Value::Null);
            Engine::new(root.path().into(), tx, status).expect("engine")
        };
        let mut engine = create();
        let mut old = serde_json::to_value(&engine.config).expect("config");
        old.as_object_mut().expect("object").remove("show_markers");
        assert!(
            serde_json::from_value::<Config>(old)
                .expect("old config")
                .show_markers
        );
        engine.armed = true;
        engine
            .command(json!({"action":"set_marker_visibility","visible":false,"hidden_types":["death","future_event"]}))
            .await
            .expect("save");
        assert!(engine.armed);
        drop(engine);
        let restored = create();
        assert!(!restored.config.show_markers);
        assert_eq!(
            restored.config.hidden_marker_types,
            vec!["death", "future_event"]
        );
    }
    #[tokio::test]
    async fn repeated_preview_requests_are_busy_without_error_or_state_reset() {
        let root = tempfile::tempdir().expect("root");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().into(), tx, status).expect("engine");
        engine.preview_running = true;
        engine.preview_status = json!({"status":"generating","key":"existing","track":0});
        let before = engine.preview_status.clone();
        let response = engine
            .command(json!({"action":"preview","id":"other"}))
            .await
            .expect("busy response");
        assert_eq!(response["busy"], true);
        assert_eq!(engine.preview_status, before);
    }
    #[tokio::test]
    async fn manual_start_and_finish_survive_restart_without_resurrection() {
        let root = tempfile::tempdir().expect("root");
        let make_engine = || {
            let (tx, _) = mpsc::channel(32);
            let (status, _) = watch::channel(Value::Null);
            Engine::new(root.path().into(), tx, status).expect("engine")
        };
        let mut engine = make_engine();
        engine.armed = true;
        engine
            .command(json!({"action":"manual_start"}))
            .await
            .expect("start");
        let id = engine.logs["manual"]
            .detector
            .active
            .as_ref()
            .expect("active")
            .id;
        assert!(engine.db.setting("logs").expect("checkpoint").is_some());
        drop(engine);
        let mut engine = make_engine();
        assert!(engine.logs["manual"].detector.active.is_none());
        assert_eq!(
            engine
                .db
                .activity(&id.to_string())
                .expect("recovered")
                .outcome,
            Outcome::Abandoned
        );
        engine.armed = true;
        engine
            .command(json!({"action":"manual_start"}))
            .await
            .expect("second start");
        let finished = engine.logs["manual"]
            .detector
            .active
            .as_ref()
            .expect("active")
            .id;
        engine
            .command(json!({"action":"manual_stop"}))
            .await
            .expect("stop");
        drop(engine);
        let engine = make_engine();
        assert!(engine.logs["manual"].detector.active.is_none());
        assert_eq!(
            engine
                .db
                .activity(&finished.to_string())
                .expect("finished")
                .outcome,
            Outcome::Unknown
        );
        assert_eq!(engine.db.list().expect("list").len(), 2);
    }

    #[tokio::test]
    async fn sidecar_queue_repairs_stale_and_missing_metadata_after_restart() {
        let root = tempfile::tempdir().expect("root");
        let make_engine = || {
            let (tx, _) = mpsc::channel(32);
            let (status, _) = watch::channel(Value::Null);
            Engine::new(root.path().into(), tx, status).expect("engine")
        };
        let mut engine = make_engine();
        let mut activity = Activity::new(Kind::Manual, "Recovery".into(), 0, "manual");
        activity.finish(2000, Outcome::Unknown);
        let id = activity.id.to_string();
        let video = root.path().join("videos").join(format!("{id}.mkv"));
        let probe = test_video(&engine.config.media, &video)
            .await
            .expect("fixture");
        engine.db.save_activity(&activity).expect("activity");
        let plan = owc_domain::ClipPlan {
            activity_id: activity.id,
            requested_start_ms: 0,
            requested_end_ms: 2000,
            actual_start_ms: Some(0),
            coverage: Coverage::Complete,
            sources: vec![],
        };
        engine.db.begin_export(&plan, &video, true).expect("plan");
        engine
            .db
            .finish_export(&id, probe.audio_tracks)
            .expect("ready");
        engine
            .sync_sidecar(&id)
            .await
            .expect("create missing sidecar");
        assert!(engine.db.pending_sidecars().expect("queue").is_empty());
        activity.marker(500, "manual", "Saved before crash".into());
        engine.db.save_activity(&activity).expect("edit");
        activity.marker(750, "manual", "Second edit while queued".into());
        engine
            .db
            .save_activity(&activity)
            .expect("coalesce queued edit");
        engine
            .db
            .annotate(&id, true, &["review".into()])
            .expect("annotations");
        drop(engine);
        let mut engine = make_engine();
        assert_eq!(
            engine.db.pending_sidecars().expect("queue"),
            vec![id.clone()]
        );
        engine.sync_sidecar(&id).await.expect("reconcile");
        let path = video.with_extension("json");
        let saved = crate::sidecar::load(&path).expect("load").expect("exists");
        assert_eq!(
            saved["activity"]["markers"][0]["label"],
            "Saved before crash"
        );
        assert_eq!(saved["annotations"]["tags"], json!(["review"]));
        assert!(engine.db.pending_sidecars().expect("queue").is_empty());
        std::fs::remove_file(&path).expect("remove");
        drop(engine);
        let mut engine = make_engine();
        engine.sync_sidecar(&id).await.expect("rebuild");
        assert_eq!(
            crate::sidecar::load(&path).expect("load").expect("rebuilt")["activity"],
            saved["activity"]
        );
        assert!(video.exists());
        std::fs::remove_file(&path).expect("remove sidecar");
        std::fs::create_dir(&path).expect("block sidecar path");
        engine
            .db
            .annotate(&id, true, &["retry".into()])
            .expect("queue edit");
        assert!(engine.sync_sidecar(&id).await.is_err());
        assert_eq!(
            engine.db.pending_sidecars().expect("still queued"),
            vec![id.clone()]
        );
        std::fs::remove_dir(&path).expect("unblock");
        engine.sync_sidecar(&id).await.expect("retry repair");
        assert!(
            engine
                .db
                .pending_sidecars()
                .expect("queue cleared")
                .is_empty()
        );
        assert_eq!(
            crate::sidecar::load(&path).expect("load").expect("exists")["annotations"]["tags"],
            json!(["retry"])
        );
    }

    #[tokio::test]
    async fn cancelled_preparation_keeps_a_durable_retryable_plan() {
        check_cancelled_preparation(false).await;
    }

    #[tokio::test]
    async fn shutdown_requeues_cancelled_preparation_for_restart() {
        check_cancelled_preparation(true).await;
    }

    async fn check_cancelled_preparation(shutdown: bool) {
        let root = tempfile::tempdir().expect("root");
        let (tx, mut requests) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        let source = engine.config.spool.join("source.mkv");
        let info = test_video(&engine.config.media, &source)
            .await
            .expect("fixture");
        let mut activity = Activity::new(Kind::Manual, "Cancel".into(), 0, "manual");
        activity.finish(2000, Outcome::Unknown);
        let segment = Segment {
            id: Uuid::new_v4(),
            session: Uuid::new_v4(),
            path: source.clone(),
            start_ms: 0,
            end_ms: info.duration_ms,
            closed: true,
            managed: true,
        };
        engine.db.add_segment(&segment).expect("source index");
        let plan = plan_clip(&activity, &[segment], 0, 0);
        engine
            .start_export(activity.clone(), plan, CutMode::Fast)
            .expect("start");
        if shutdown {
            engine.prepare_shutdown().await.expect("shutdown");
        } else {
            engine
                .command(json!({"action":"cancel_export","id":activity.id}))
                .await
                .expect("cancel");
        }
        assert!(
            engine.export_running,
            "retain pins until completion is acknowledged"
        );
        let request = tokio::time::timeout(Duration::from_secs(5), requests.recv())
            .await
            .expect("completion deadline")
            .expect("completion");
        engine
            .command(request.data)
            .await
            .expect("record cancellation");
        assert!(!engine.export_running);
        if shutdown {
            assert_eq!(engine.db.list().expect("library")[0]["status"], "pending");
            engine.tick().await.expect("shutdown tick");
            assert!(
                !engine.export_running,
                "shutdown must not restart queued jobs"
            );
            assert_eq!(engine.snapshot().expect("snapshot")["shutdown_ready"], true);
        } else {
            assert_eq!(engine.db.list().expect("library")[0]["status"], "failed");
            assert!(
                engine.db.list().expect("library")[0]["error"]
                    .as_str()
                    .expect("error")
                    .contains("cancelled")
            );
        }
        engine.cleanup().await.expect("cleanup");
        assert!(source.exists(), "failed plan keeps source pinned");
        drop(engine);
        let db = Library::open(&root.path().join("library.sqlite")).expect("restart");
        assert!(
            db.export_plan(&activity.id.to_string())
                .expect("plan")
                .is_some()
        );
        db.retry(&activity.id.to_string()).expect("retry");
        assert_eq!(db.pending().expect("pending").len(), 1);
    }

    #[tokio::test]
    #[ignore = "requires private limited tmpfs; run scripts/disk-full-smoke.py"]
    async fn disk_full_log_checkpoint_retains_ack_and_retries() {
        let mount = std::env::var_os("OWC_LIMITED_FILESYSTEM").expect("private tmpfs");
        let root = tempfile::tempdir_in(mount).expect("root");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        let installation = crate::config::Installation {
            id: Uuid::new_v4(),
            name: "Retail".into(),
            logs: root.path().into(),
            flavour: owc_wow_log::Flavour::Retail,
            categories: vec![],
            difficulties: vec![],
            minimum_key: 0,
        };
        engine.config.installations = vec![installation.clone()];
        engine
            .commit_logs(&json!({}), &[])
            .expect("initial checkpoint");
        let mut activity = Activity::new(
            Kind::MythicPlus,
            "x".repeat(8 * 1024 * 1024),
            1000,
            "Retail",
        );
        activity.finish(2000, Outcome::Success);
        engine.completed_events.push(activity.clone());
        let (committed, mut ack) = oneshot::channel();
        let result = engine
            .log_batch(LogBatch {
                generation: engine.log_generation,
                installation_id: installation.id,
                state: LogState::new(&installation),
                events: vec![],
                parse_errors: 0,
                error: None,
                committed,
            })
            .await;
        assert!(result.is_err(), "oversized checkpoint must exhaust tmpfs");
        assert_eq!(ack.try_recv(), Err(oneshot::error::TryRecvError::Empty));
        assert_eq!(engine.db.setting("logs").expect("cursor"), Some(json!({})));
        assert!(engine.db.list().expect("library").is_empty());
        assert_eq!(engine.completed_events.len(), 1);
        // Make the retained transaction fit, without replaying lifecycle events.
        engine.completed_events[0].name = "Temple".into();
        engine.retry_log_checkpoint().expect("retry");
        ack.await.expect("watcher can continue");
        assert!(engine.pending_log_acks.is_empty());
        assert!(engine.completed_events.is_empty());
        let saved = engine.db.list().expect("library");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0]["id"], activity.id.to_string());
        assert_eq!(saved[0]["outcome"], "success");
        assert!(
            engine
                .db
                .setting("logs")
                .expect("cursor")
                .expect("logs")
                .get(installation.id.to_string())
                .is_some()
        );
        engine.retry_log_checkpoint().expect("idempotent retry");
        assert_eq!(engine.db.list().expect("library").len(), 1);
    }

    #[tokio::test]
    async fn delayed_completion_recovers_closed_history_without_controlling_obs() {
        let root = tempfile::tempdir().expect("root");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        let now = Engine::now();
        let mut activity = Activity::new(Kind::Raid, "Delayed".into(), now - 60_000, "Retail");
        activity.finish(now - 20_000, Outcome::Success);
        engine.activity_finished(activity.clone());
        assert!(engine.completed_events.is_empty(), "setup is required");
        engine.setup_completed = true;
        engine.automatic_paused = true;
        engine.activity_finished(activity.clone());
        assert!(
            engine.completed_events.is_empty(),
            "respect automatic pause"
        );
        engine.automatic_paused = false;
        engine
            .activity_started(activity.clone())
            .await
            .expect("offline start");
        assert!(!engine.armed);
        assert!(engine.automatic_activities.is_empty());
        // An unrelated delayed completion must not postpone another capture's stop.
        engine.automatic_session = true;
        let deadline = Instant::now() + Duration::from_secs(30);
        engine.automatic_stop = Some(deadline);
        engine.activity_finished(activity.clone());
        assert_eq!(engine.automatic_stop, Some(deadline));
        engine.automatic_session = false;
        engine.automatic_stop = None;
        engine.commit_logs(&Value::Null, &[]).expect("checkpoint");
        let saved = engine
            .db
            .activity(&activity.id.to_string())
            .expect("saved activity");
        assert!(
            saved.warnings.is_empty(),
            "normal closure is not interruption"
        );
        let segment = Segment {
            id: Uuid::new_v4(),
            session: Uuid::new_v4(),
            path: root.path().join("retained.mkv"),
            start_ms: now - 70_000,
            end_ms: now - 10_000,
            closed: true,
            managed: true,
        };
        assert_eq!(
            plan_clip(&saved, &[segment], 5_000, 5_000).coverage,
            Coverage::Complete
        );
        // With no retained files the normal scheduler publishes an honest state.
        engine.tick().await.expect("schedule unavailable encounter");
        let records = engine.db.list().expect("library");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["status"], "unavailable");
        assert!(!engine.armed);
        assert!(engine.recording.is_none());
        assert!(engine.deferred_starts.is_empty());
    }

    #[tokio::test]
    async fn completed_encounter_survives_loss_of_capture() {
        let root = tempfile::tempdir().expect("root");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        let mut activity = Activity::new(
            Kind::MythicPlus,
            "Temple".into(),
            Engine::now() - 1000,
            "Retail",
        );
        activity.finish(Engine::now(), Outcome::Success);
        engine.automatic_session = true;
        engine.automatic_activities.insert(activity.id);
        engine.armed = true;
        engine.disconnected();
        engine.activity_finished(activity.clone());
        engine
            .db
            .checkpoint("logs", &Value::Null, &engine.completed_events)
            .expect("checkpoint");
        let saved = engine.db.list().expect("library");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0]["id"], activity.id.to_string());
        assert_eq!(saved[0]["outcome"], "success");
        assert!(
            saved[0]["warnings"][0]
                .as_str()
                .expect("warning")
                .contains("interrupted")
        );
        assert!(engine.automatic_activities.is_empty());
    }

    #[tokio::test]
    async fn stopped_automatic_recordings_are_durable_and_reject_stale_workers() {
        for kind in [
            Kind::MythicPlus,
            Kind::Raid,
            Kind::Arena,
            Kind::SoloShuffle,
            Kind::Battleground,
        ] {
            verify_stopped_activity(kind).await;
        }
    }

    async fn verify_stopped_activity(kind: Kind) {
        let root = tempfile::tempdir().expect("root");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        let installation = crate::config::Installation {
            id: Uuid::new_v4(),
            name: "Retail".into(),
            logs: root.path().into(),
            flavour: owc_wow_log::Flavour::Retail,
            categories: vec![kind],
            difficulties: vec![],
            minimum_key: 0,
        };
        let mut state = LogState::new(&installation);
        let mut activity = Activity::new(
            kind,
            "Current encounter".into(),
            Engine::now() - 1000,
            "Retail",
        );
        activity.installation_id = Some(installation.id);
        state.detector.active = Some(activity.clone());
        engine.config.installations.push(installation.clone());
        engine
            .logs
            .insert(installation.id.to_string(), state.clone());
        engine.automatic_session = true;
        engine.automatic_activities.insert(activity.id);
        let other = Uuid::new_v4();
        if kind != Kind::Raid {
            engine.automatic_activities.insert(other);
        }
        engine
            .add_marker(&json!({"id":activity.id,"label":"Review pull"}))
            .await
            .expect("live marker");
        assert_eq!(
            engine.logs[&installation.id.to_string()]
                .detector
                .active
                .as_ref()
                .expect("active")
                .markers[0]
                .label,
            "Review pull"
        );
        let generation = engine.log_generation;
        engine
            .command(json!({"action":if kind == Kind::MythicPlus {"abandon_key"} else {"stop_activity"},"id":activity.id}))
            .await
            .expect("abandon");
        assert!(!engine.automatic_activities.contains(&activity.id));
        assert_eq!(
            engine.automatic_activities.contains(&other),
            kind != Kind::Raid
        );
        assert_eq!(engine.automatic_stop.is_some(), kind == Kind::Raid);
        assert!(!engine.automatic_paused);
        assert!(
            engine.logs[&installation.id.to_string()]
                .detector
                .active
                .is_none()
        );
        let saved = engine.db.list().expect("saved");
        assert_eq!(saved[0]["outcome"], "abandoned");
        assert!(engine.stop_activity(activity.id).is_err());
        let (committed, _) = oneshot::channel();
        engine
            .log_batch(LogBatch {
                generation,
                installation_id: installation.id,
                state,
                events: vec![],
                parse_errors: 0,
                error: None,
                committed,
            })
            .await
            .expect("stale batch");
        assert!(
            engine.logs[&installation.id.to_string()]
                .detector
                .active
                .is_none()
        );
    }

    #[tokio::test]
    async fn manual_markers_are_scoped_to_installation_and_recording_window() {
        let root = tempfile::tempdir().expect("root");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        let config = crate::config::Installation {
            id: Uuid::new_v4(),
            name: "Retail".into(),
            logs: root.path().into(),
            flavour: owc_wow_log::Flavour::Retail,
            categories: vec![],
            difficulties: vec![],
            minimum_key: 0,
        };
        let mut other = config.clone();
        other.id = Uuid::new_v4();
        engine.config.installations = vec![config.clone(), other.clone()];
        let mut manual = LogState::new(&config);
        let mut activity = Activity::new(Kind::Manual, "Mid dungeon".into(), 1000, &config.name);
        activity.installation_id = Some(config.id);
        manual.detector.active = Some(activity);
        engine.logs.insert("manual".into(), manual);
        for (installation, at) in [(&config, 999), (&other, 1500), (&config, 2000)] {
            let (committed, ack) = oneshot::channel();
            engine
                .log_batch(LogBatch {
                    generation: engine.log_generation,
                    installation_id: installation.id,
                    state: LogState::new(installation),
                    parse_errors: 0,
                    error: None,
                    committed,
                    events: vec![LogEvent {
                        source: crate::log_watcher::Source {
                            installation_id: installation.id,
                            installation_name: installation.name.clone(),
                            flavour: installation.flavour,
                            logs_directory: installation.logs.clone(),
                            log_file: None,
                            line: None,
                        },
                        event: ActivityEvent::Marker(owc_domain::Marker {
                            recent_hits: vec![],
                            at_ms: at,
                            kind: "death".into(),
                            label: "Player".into(),
                        }),
                    }],
                })
                .await
                .expect("batch");
            ack.await.expect("checkpoint ack");
        }
        let manual = engine.logs["manual"]
            .detector
            .active
            .as_ref()
            .expect("manual");
        assert_eq!(manual.markers.len(), 1);
        assert_eq!(manual.markers[0].at_ms, 2000);
        engine
            .command(json!({"action":"manual_stop"}))
            .await
            .expect("finish");
        let saved = engine.db.list().expect("recordings");
        assert_eq!(saved[0]["markers"][0]["at_ms"], 2000);
        assert_eq!(saved[0]["installation_id"], config.id.to_string());
    }

    #[tokio::test]
    async fn stale_readiness_cannot_override_a_new_check_or_finish_offline_setup() {
        let root = tempfile::tempdir().expect("directory");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        engine.setup_generation = 2;
        engine.command(json!({"action":"_setup_done","generation":1,"report":{"connection":"connected","checks":[]}})).await.expect("stale result");
        assert_ne!(engine.setup["connection"], "connected");
        assert!(
            engine
                .command(json!({"action":"finish_setup","picture_sound":true,"manual_only":true}))
                .await
                .is_err()
        );
        assert!(!engine.setup_completed);
    }

    #[tokio::test]
    async fn missed_events_preserve_source_and_withhold_uncertain_footage() {
        let root = tempfile::tempdir().expect("directory");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        let source = engine.config.spool.join("uncertain.mkv");
        std::fs::write(&source, b"unconfirmed source must remain untouched").expect("file");
        engine.armed = true;
        engine.guard.arm(false);
        engine.guard.started();
        engine.recording = Some(OpenRecording {
            session: Uuid::new_v4(),
            path: Some(source.clone()),
            start_ms: 1000,
            managed: true,
            invalid: false,
        });
        let (events, receiver) = tokio::sync::broadcast::channel(2);
        let watcher = engine.bus.watch_obs(receiver, engine.obs_generation);
        for _ in 0..4 {
            events
                .send(Event::Obs {
                    kind: "RecordFileChanged".into(),
                    data: json!({"newOutputPath":"unknown"}),
                    received_ms: 2000,
                })
                .expect("event");
        }
        tokio::task::yield_now().await;
        engine.tick().await.expect("lost events handled");
        watcher.abort();
        assert!(!engine.armed);
        assert!(!engine.guard.can_mutate());
        assert!(engine.recording.as_ref().expect("recording").invalid);
        engine
            .event(Event::Obs {
                kind: "RecordStateChanged".into(),
                data: json!({"outputState":"OBS_WEBSOCKET_OUTPUT_STOPPED"}),
                received_ms: 3000,
            })
            .await
            .expect("stop uncertain source");
        assert!(source.exists());
        assert!(engine.db.segments().expect("segments").is_empty());
    }

    #[tokio::test]
    async fn cleanup_removes_only_owned_expired_segments_and_unpinned_cache() {
        let root = tempfile::tempdir().expect("directory");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        // A zero test budget exercises eviction without allocating large media files.
        engine.config.library_gib = 0;
        let managed = engine.config.spool.join("managed.mkv");
        let external = root.path().join("external.mkv");
        for (path, owned) in [(&managed, true), (&external, false)] {
            std::fs::write(path, b"source").expect("file");
            engine
                .db
                .add_segment(&Segment {
                    id: Uuid::new_v4(),
                    session: Uuid::new_v4(),
                    path: path.clone(),
                    start_ms: 0,
                    end_ms: 1000,
                    closed: true,
                    managed: owned,
                })
                .expect("segment");
        }
        let cache = root.path().join("preview");
        let old = cache.join(format!("{}-track-0.mp4", Uuid::new_v4()));
        let current_name = format!("{}-track-0.mp4", Uuid::new_v4());
        let current = cache.join(&current_name);
        let unknown = cache.join("personal-video.mp4");
        for path in [&old, &current, &unknown] {
            std::fs::write(path, b"cache").expect("cache");
        }
        engine.preview_status = json!({"url":format!("/preview/{current_name}")});
        let staging = cache.join(format!(".review-{}", Uuid::new_v4()));
        std::fs::create_dir(&staging).expect("staging");
        std::fs::write(staging.join("playback.mp4"), b"partial").expect("partial");
        engine.preview_running = true;
        engine.cleanup().await.expect("pinned cleanup");
        assert!(managed.exists());
        assert!(staging.exists());
        engine.preview_running = false;
        engine.cleanup().await.expect("cleanup");
        assert!(!staging.exists());
        assert!(!managed.exists());
        assert!(external.exists());
        assert!(!old.exists());
        assert!(current.exists());
        assert!(unknown.exists());
    }

    #[tokio::test]
    async fn stop_closes_current_split_instead_of_original_path() {
        let root = tempfile::tempdir().expect("directory");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        let current = engine.config.spool.join("current.mkv");
        test_video(&engine.config.media, &current)
            .await
            .expect("media");
        engine.recording = Some(OpenRecording {
            session: Uuid::new_v4(),
            path: Some(current.clone()),
            start_ms: 1000,
            managed: true,
            invalid: false,
        });
        engine.event(Event::Obs {
            kind: "RecordStateChanged".into(),
            received_ms: 4000,
            data: json!({"outputState":"OBS_WEBSOCKET_OUTPUT_STOPPED", "outputPath":root.path().join("original-already-closed.mkv")}),
        }).await.expect("stop");
        let segments = engine.db.segments().expect("segments");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].path, current);
        assert!(engine.recording.is_none());
    }

    #[tokio::test]
    async fn clock_jump_disarms_without_mutating_obs() {
        let root = tempfile::tempdir().expect("directory");
        let (tx, _) = mpsc::channel(32);
        let (status, _) = watch::channel(Value::Null);
        let mut engine = Engine::new(root.path().to_owned(), tx, status).expect("engine");
        engine.armed = true;
        engine.last_clock.1 -= 10_000;
        engine.tick().await.expect("tick");
        assert!(!engine.armed);
        assert!(!engine.guard.can_mutate());
        assert!(engine.message.contains("clock changed"));
    }
}

#[cfg(test)]
#[path = "engine_connection_tests.rs"]
mod connection_tests;

impl Drop for Engine {
    fn drop(&mut self) {
        for watcher in self.log_watchers.drain(..) {
            watcher.abort();
        }
        if let Some(watcher) = self.obs_watcher.take() {
            watcher.abort();
        }
    }
}
