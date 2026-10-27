//! Installation-scoped parser workers publish checkpointed batches to the capture controller.
use crate::{
    config::Installation,
    events::{CaptureEvent, EventBus},
};
use notify::Watcher;
use owc_domain::{Activity, Marker, Outcome};
use owc_wow_log::{Detector, Flavour, Tailer};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogState {
    pub tailer: Tailer,
    pub detector: Detector,
    #[serde(default)]
    pub last_source: Option<PathBuf>,
    #[serde(default)]
    pub last_combat_sample_ms: Option<i64>,
    #[serde(default)]
    pub death_recaps: owc_wow_log::DeathRecaps,
}
impl LogState {
    pub fn new(installation: &Installation) -> Self {
        Self {
            tailer: Tailer::default(),
            detector: Detector::new(installation.name.clone(), installation.flavour),
            last_source: None,
            last_combat_sample_ms: None,
            death_recaps: owc_wow_log::DeathRecaps::default(),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct LineProvenance {
    pub file_identity: String,
    pub byte_offset: u64,
    pub raw_timestamp: String,
    pub event_utc_ms: i64,
    pub received_utc_ms: i64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub installation_id: Uuid,
    pub installation_name: String,
    pub flavour: Flavour,
    pub logs_directory: PathBuf,
    pub log_file: Option<PathBuf>,
    pub line: Option<LineProvenance>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AbandonReason {
    Replaced,
    Inactivity,
    DungeonExit,
    User,
}
#[derive(Debug, Clone, Serialize)]
pub enum ActivityEvent {
    Diagnostic(String),
    Marker(Marker),
    Started(Activity),
    Updated(Activity),
    Finished(Activity),
    Abandoned {
        activity: Activity,
        reason: AbandonReason,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct LogEvent {
    pub source: Source,
    pub event: ActivityEvent,
}
#[derive(Debug)]
pub struct LogBatch {
    pub generation: u64,
    pub installation_id: Uuid,
    pub state: LogState,
    pub events: Vec<LogEvent>,
    pub parse_errors: u64,
    pub error: Option<String>,
    pub committed: oneshot::Sender<()>,
}
fn source(installation: &Installation, log_file: Option<PathBuf>) -> Source {
    Source {
        installation_id: installation.id,
        installation_name: installation.name.clone(),
        flavour: installation.flavour,
        logs_directory: installation.logs.clone(),
        log_file,
        line: None,
    }
}
fn stamp(activity: &mut Activity, installation: &Installation) {
    activity.installation_id = Some(installation.id);
    activity.installation.clone_from(&installation.name);
}
fn completed(activity: Activity, reason: AbandonReason) -> ActivityEvent {
    if activity.outcome == Outcome::Abandoned {
        let reason = if activity
            .markers
            .iter()
            .any(|marker| marker.kind == "dungeon_exit")
        {
            AbandonReason::DungeonExit
        } else {
            reason
        };
        ActivityEvent::Abandoned { activity, reason }
    } else {
        ActivityEvent::Finished(activity)
    }
}
fn marker_from_log(event: &owc_wow_log::Event) -> Option<Marker> {
    let (kind, label) = match event.kind() {
        "ENCOUNTER_START" if event.number(1).is_some() && event.number(3).is_some() => {
            ("boss_start", event.field(2).to_owned())
        }
        "ENCOUNTER_END" if event.number(1).is_some() => (
            "boss_end",
            format!(
                "{}: {}",
                event.field(2),
                if event.field(5) == "1" {
                    "kill"
                } else {
                    "wipe"
                }
            ),
        ),
        "UNIT_DIED" if event.field(5).starts_with("Player-") && event.field(9) != "1" => {
            ("death", event.field(6).to_owned())
        }
        kind if (kind.ends_with("_DAMAGE") || kind.ends_with("_MISSED"))
            && kind != "ENVIRONMENTAL_DAMAGE"
            && [event.field(3), event.field(7)].iter().any(|flags| {
                u32::from_str_radix(flags.trim_start_matches("0x"), 16)
                    .is_ok_and(|flags| flags & 0x7 != 0)
            }) =>
        {
            ("combat_activity", "Combat activity (estimated)".into())
        }
        _ => return None,
    };
    Some(Marker {
        recent_hits: vec![],
        at_ms: event.at,
        kind: kind.into(),
        label,
    })
}

fn record_boundary(
    events: &mut Vec<LogEvent>,
    installation: &Installation,
    path: &std::path::Path,
    parsed: &owc_wow_log::Event,
) {
    if matches!(
        parsed.kind(),
        "ZONE_CHANGE" | "CHALLENGE_MODE_START" | "CHALLENGE_MODE_END" | "COMBAT_LOG_VERSION"
    ) {
        let detail = if parsed.kind() == "ZONE_CHANGE" {
            format!("ZONE_CHANGE: {} ({})", parsed.field(2), parsed.field(1))
        } else {
            parsed.kind().to_owned()
        };
        events.push(LogEvent {
            source: source(installation, Some(path.to_owned())),
            event: ActivityEvent::Diagnostic(detail),
        });
    }
}

fn ingest_line(
    installation: &Installation,
    state: &mut LogState,
    line: owc_wow_log::SourceLine,
    parsed: &owc_wow_log::Event,
    now: i64,
    events: &mut Vec<LogEvent>,
) {
    let emitted_from = events.len();
    let provenance = LineProvenance {
        file_identity: line.identity,
        byte_offset: line.offset,
        raw_timestamp: line.text.split_once("  ").map_or("", |(at, _)| at).into(),
        event_utc_ms: parsed.at,
        received_utc_ms: now,
    };
    if state
        .last_source
        .as_ref()
        .is_some_and(|path| path != &line.path)
    {
        state.death_recaps = owc_wow_log::DeathRecaps::default();
    }
    let recent_hits = state.death_recaps.ingest(parsed);
    state.last_source = Some(line.path.clone());
    record_boundary(events, installation, &line.path, parsed);
    let previous = state.detector.active.clone();
    for mut activity in state.detector.ingest(parsed) {
        stamp(&mut activity, installation);
        events.push(LogEvent {
            source: source(installation, Some(line.path.clone())),
            event: completed(activity, AbandonReason::Replaced),
        });
    }
    if parsed.kind() == "UNIT_DIED"
        && let Some(activity) = &mut state.detector.active
        && let Some(marker) = activity.markers.last_mut()
        && marker.kind == "death"
        && marker.at_ms == parsed.at
    {
        marker.recent_hits.clone_from(&recent_hits);
    }
    let marker = marker_from_log(parsed)
        .map(|mut marker| {
            if marker.kind == "death" {
                marker.recent_hits = recent_hits;
            }
            marker
        })
        .filter(|marker| {
            if marker.kind != "combat_activity" {
                return true;
            }
            if state
                .last_combat_sample_ms
                .is_some_and(|last| marker.at_ms >= last && marker.at_ms - last < 1000)
            {
                return false;
            }
            state.last_combat_sample_ms = Some(marker.at_ms);
            true
        });
    if let Some(marker) = &marker
        && marker.kind == "combat_activity"
        && let Some(activity) = &mut state.detector.active
    {
        activity.markers.push(marker.clone());
    }
    if let Some(activity) = &mut state.detector.active {
        stamp(activity, installation);
        let changed = previous.as_ref().is_none_or(|old| old.id != activity.id);
        if changed
            || previous.as_ref().is_some_and(|old| {
                old.markers.len() != activity.markers.len() || old.character != activity.character
            })
        {
            events.push(LogEvent {
                source: source(installation, Some(line.path.clone())),
                event: if changed {
                    ActivityEvent::Started(activity.clone())
                } else {
                    ActivityEvent::Updated(activity.clone())
                },
            });
        }
    }
    if let Some(marker) = marker {
        events.push(LogEvent {
            source: source(installation, Some(line.path)),
            event: ActivityEvent::Marker(marker),
        });
    }
    for event in &mut events[emitted_from..] {
        event.source.line = Some(provenance.clone());
    }
}

fn poll(
    installation: &Installation,
    state: &mut LogState,
    now: i64,
    timeout: i64,
) -> (Vec<LogEvent>, u64, Option<String>) {
    let mut events = vec![];
    let mut errors = 0;
    let (lines, error) = match state.tailer.poll_sources(&installation.logs) {
        Ok(lines) => (lines, None),
        Err(error) => (vec![], Some(error.to_string())),
    };
    for line in lines {
        let Ok(parsed) = owc_wow_log::parse(&line.text, now) else {
            errors += 1;
            continue;
        };
        ingest_line(installation, state, line, &parsed, now, &mut events);
    }
    // Wall-clock expiry is unsafe until the bounded reader reaches the file end.
    if state.tailer.has_backlog {
        return (events, errors, None);
    }
    for mut activity in state.detector.expire(now, timeout) {
        stamp(&mut activity, installation);
        events.push(LogEvent {
            source: source(installation, state.last_source.clone()),
            event: completed(activity, AbandonReason::Inactivity),
        });
    }
    (events, errors, error)
}
fn watch_changes(
    path: &std::path::Path,
) -> (
    Option<notify::RecommendedWatcher>,
    tokio::sync::mpsc::Receiver<()>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        // A wakeup requests reconciliation, not one read per notification. Coalescing
        // keeps bursts bounded while the controller acknowledges durable checkpoints.
        if !matches!(
            event,
            Ok(notify::Event {
                kind: notify::EventKind::Access(_),
                ..
            })
        ) {
            let _ = tx.try_send(());
        }
    })
    .and_then(|mut watcher| {
        watcher.watch(path, notify::RecursiveMode::NonRecursive)?;
        Ok(watcher)
    });
    match watcher {
        Ok(watcher) => (Some(watcher), rx),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "Log notifications unavailable; periodic reconciliation remains active");
            (None, rx)
        }
    }
}

pub fn spawn(
    installation: Installation,
    mut state: LogState,
    generation: u64,
    timeout: Duration,
    recover: bool,
    bus: EventBus,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let (_watcher, mut changes) = watch_changes(&installation.logs);
        // The durable detector and cursor describe one activity. Replay missed
        // lines before inactivity/exit expiry; restarting alone is not an ending.
        let mut first_poll = recover;
        state.detector.installation.clone_from(&installation.name);
        loop {
            let before = serde_json::to_vec(&state).ok();
            let config = installation.clone();
            let result = tokio::task::spawn_blocking(move || {
                let now = chrono::Utc::now().timestamp_millis();
                let result = poll(
                    &config,
                    &mut state,
                    now,
                    i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX),
                );
                (state, result)
            })
            .await;
            let Ok((next, (events, parse_errors, error))) = result else {
                break;
            };
            state = next;
            if first_poll
                || !events.is_empty()
                || parse_errors > 0
                || error.is_some()
                || before != serde_json::to_vec(&state).ok()
            {
                let (committed, ack) = oneshot::channel();
                let batch = LogBatch {
                    generation,
                    installation_id: installation.id,
                    state: state.clone(),
                    events,
                    parse_errors,
                    error,
                    committed,
                };
                if !bus.publish(CaptureEvent::Logs(Box::new(batch))).await || ack.await.is_err() {
                    break;
                }
                first_poll = false;
            }
            if !state.tailer.has_backlog {
                tokio::select! {
                    Some(()) = changes.recv() => {},
                    () = tokio::time::sleep(Duration::from_millis(250)) => {},
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Shared crash/replay fixture checks each terminal and continuing state.
    async fn restart_replays_missed_events_before_deciding_whether_key_has_ended() {
        use chrono::TimeZone;
        for scenario in [
            "running",
            "still",
            "completed",
            "exit",
            "quiet",
            "backlog",
            "missing",
        ] {
            let root = tempfile::tempdir().expect("root");
            let config = installation(root.path().join("Logs"));
            std::fs::create_dir(&config.logs).expect("logs");
            let file = config.logs.join("WoWCombatLog-restart.txt");
            let now = chrono::Utc::now().timestamp_millis();
            let start = now - if scenario == "still" { 10_000 } else { 600_000 };
            let line = |at, body: &str| {
                format!(
                    "{}  {body}",
                    chrono::Local
                        .timestamp_millis_opt(at)
                        .single()
                        .expect("time")
                        .format("%m/%d/%Y %H:%M:%S%.3f")
                )
            };
            let mut initial = LogState::new(&config);
            poll(&config, &mut initial, now, 300_000);
            append(
                &file,
                &line(start, "CHALLENGE_MODE_START,Restart dungeon,100,200,20,[]"),
            );
            poll(&config, &mut initial, start, 300_000);
            let id = initial.detector.active.as_ref().expect("started").id;
            let restored: LogState =
                serde_json::from_value(serde_json::to_value(&initial).expect("checkpoint"))
                    .expect("restart");
            match scenario {
                "running" => append(
                    &file,
                    &line(now - 1000, "UNIT_DIED,0,nil,0,0,Player-1,Player,0x511,0,0"),
                ),
                "completed" => append(
                    &file,
                    &line(now - 1000, "CHALLENGE_MODE_END,100,1,20,599000"),
                ),
                "exit" => append(&file, &line(now - 61_000, "ZONE_CHANGE,999,Outside,0")),
                "backlog" => {
                    let mut content = String::new();
                    for index in 0..15_000 {
                        content.push_str(&line(now - 590_000 + index * 39, "SPELL_DAMAGE,Player-1,Player,0x511,0,Creature-1,Enemy,0xa48,0,123,Spell,1,42"));
                        content.push('\n');
                    }
                    append(&file, &content);
                }
                "missing" => std::fs::remove_dir_all(&config.logs).expect("missing logs"),
                _ => {}
            }
            let (bus, mut receiver) = EventBus::new();
            let task = spawn(config, restored, 1, Duration::from_mins(5), true, bus);
            let mut finished = vec![];
            let mut starts = 0;
            let mut batches = 0;
            loop {
                let CaptureEvent::Logs(batch) =
                    tokio::time::timeout(Duration::from_secs(5), receiver.recv())
                        .await
                        .expect("batch timeout")
                        .expect("batch")
                else {
                    panic!("logs")
                };
                batches += 1;
                for event in &batch.events {
                    match &event.event {
                        ActivityEvent::Started(_) => starts += 1,
                        ActivityEvent::Finished(activity)
                        | ActivityEvent::Abandoned { activity, .. } => {
                            finished.push(activity.clone());
                        }
                        _ => {}
                    }
                }
                if !batch.state.tailer.has_backlog {
                    if matches!(scenario, "running" | "still" | "backlog") {
                        assert_eq!(batch.state.detector.active.as_ref().expect(scenario).id, id);
                        assert!(finished.is_empty(), "{scenario}: no invented abandonment");
                    } else {
                        assert!(batch.state.detector.active.is_none(), "{scenario}");
                        assert_eq!(finished.len(), 1, "{scenario}");
                        assert_eq!(finished[0].id, id);
                        assert_eq!(
                            finished[0].outcome,
                            if scenario == "completed" {
                                Outcome::Success
                            } else {
                                Outcome::Abandoned
                            }
                        );
                    }
                    assert_eq!(starts, 0, "restart must not create a second key");
                    if scenario == "backlog" {
                        assert!(batches > 1);
                    }
                    task.abort();
                    break;
                }
                batch
                    .committed
                    .send(())
                    .expect("checkpoint acknowledgement");
            }
        }
    }

    #[tokio::test]
    async fn reconciliation_recovers_when_notification_folder_is_missing() {
        let root = tempfile::tempdir().expect("root");
        let logs = root.path().join("Logs");
        std::fs::create_dir(&logs).expect("logs");
        let config = installation(logs.clone());
        let mut initial = LogState::new(&config);
        poll(&config, &mut initial, 0, i64::MAX);
        std::fs::remove_dir(&logs).expect("remove folder");
        let (bus, mut receiver) = EventBus::new();
        let task = spawn(config, initial, 1, Duration::from_hours(8760), false, bus);
        let CaptureEvent::Logs(missing) = receiver.recv().await.expect("missing folder") else {
            panic!("log batch")
        };
        assert!(missing.error.is_some());
        std::fs::create_dir(&logs).expect("restore folder");
        std::fs::write(
            logs.join("WoWCombatLog-new.txt"),
            "01/15/2026 12:00:00.000  ENCOUNTER_START,1001,Boss,16,20\n",
        )
        .expect("new log");
        missing.committed.send(()).expect("ack");
        let CaptureEvent::Logs(batch) =
            tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("reconcile timeout")
                .expect("batch")
        else {
            panic!("log batch")
        };
        assert!(batch.error.is_none());
        assert!(
            batch
                .events
                .iter()
                .any(|event| matches!(event.event, ActivityEvent::Started(_)))
        );
        task.abort();
    }

    #[tokio::test]
    async fn native_notifications_wake_on_create_append_and_rotation() {
        let root = tempfile::tempdir().expect("root");
        let (watcher, mut changes) = watch_changes(root.path());
        assert!(watcher.is_some(), "native watcher unavailable");
        let file = root.path().join("WoWCombatLog.txt");
        for action in 0..3 {
            while changes.try_recv().is_ok() {}
            match action {
                0 => std::fs::write(&file, "first\n").expect("create"),
                1 => writeln!(
                    std::fs::OpenOptions::new()
                        .append(true)
                        .open(&file)
                        .expect("open"),
                    "second"
                )
                .expect("append"),
                _ => std::fs::rename(&file, root.path().join("WoWCombatLog-old.txt"))
                    .expect("rotate"),
            }
            tokio::time::timeout(Duration::from_secs(3), changes.recv())
                .await
                .expect("native event")
                .expect("wakeup");
            assert!(changes.len() <= 1, "wakeups must remain bounded");
        }
    }

    fn installation(path: PathBuf) -> Installation {
        Installation {
            id: Uuid::new_v4(),
            name: "Retail".into(),
            logs: path,
            flavour: Flavour::Retail,
            categories: vec![],
            difficulties: vec![],
            minimum_key: 0,
            minimum_raid_difficulty: None,
        }
    }
    fn append(path: &std::path::Path, line: &str) {
        writeln!(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("log"),
            "{line}"
        )
        .expect("append");
    }
    #[test]
    fn emitted_events_keep_raw_time_receipt_and_file_position() {
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let file = config.logs.join("WoWCombatLog-provenance.txt");
        let raw_time = "01/15/2026 12:00:00.000";
        let line = format!("{raw_time}  ENCOUNTER_START,1001,Boss,16,20");
        let at = owc_wow_log::parse(&line, 0).expect("event").at;
        let mut state = LogState::new(&config);
        poll(&config, &mut state, at, 300_000);
        append(&file, "");
        append(&file, &line);
        let (events, errors, error) = poll(&config, &mut state, at + 30_000, 300_000);
        assert_eq!(errors, 0);
        assert!(error.is_none());
        assert!(
            events
                .iter()
                .any(|event| matches!(event.event, ActivityEvent::Started(_)))
        );
        for event in events {
            let provenance = event.source.line.expect("source line");
            assert_eq!(provenance.byte_offset, 1);
            assert_eq!(provenance.raw_timestamp, raw_time);
            assert_eq!(provenance.event_utc_ms, at);
            assert_eq!(provenance.received_utc_ms, at + 30_000);
            assert!(!provenance.file_identity.is_empty());
        }
        let (expired, _, _) = poll(&config, &mut state, at + 400_000, 300_000);
        assert!(!expired.is_empty());
        assert!(expired.iter().all(|event| event.source.line.is_none()));
    }

    #[test]
    fn forty_five_minute_key_keeps_nested_bosses_and_deaths_in_one_activity() {
        use chrono::TimeZone;
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let file = config.logs.join("WoWCombatLog-long-key.txt");
        let at = owc_wow_log::parse("01/15/2026 12:00:00.000  COMBAT_LOG_VERSION,1", 0)
            .expect("time")
            .at;
        let mut state = LogState::new(&config);
        poll(&config, &mut state, at, 300_000);
        let mut lifecycle = vec![];
        for second in (0..=2700).step_by(30) {
            let now = at + second * 1000;
            let stamp = chrono::Local
                .timestamp_millis_opt(now)
                .single()
                .expect("time")
                .format("%m/%d/%Y %H:%M:%S%.3f")
                .to_string();
            let mut bodies = vec![
                "SPELL_CAST_SUCCESS,Player-1,Player,0x511,0,Creature-1,Enemy,0xa48,0,1,Spell,1",
            ];
            if second == 0 {
                bodies.push("CHALLENGE_MODE_START,Dungeon,100,200,20,[]");
            }
            match second % 600 {
                60 => bodies.push("ENCOUNTER_START,1001,Boss,8,5"),
                90 => bodies.push("UNIT_DIED,0,nil,0,0,Player-1,Player,0x511,0,0"),
                180 => bodies.push("ENCOUNTER_END,1001,Boss,8,5,1"),
                _ => {}
            }
            if second == 2700 {
                bodies.push("CHALLENGE_MODE_END,100,1,20,2700000");
            }
            for body in bodies {
                append(&file, &format!("{stamp}  {body}"));
            }
            let (events, errors, error) = poll(&config, &mut state, now, 300_000);
            assert_eq!(errors, 0);
            assert!(error.is_none());
            lifecycle.extend(events.into_iter().filter(|event| {
                matches!(
                    event.event,
                    ActivityEvent::Started(_)
                        | ActivityEvent::Finished(_)
                        | ActivityEvent::Abandoned { .. }
                )
            }));
        }
        assert_eq!(lifecycle.len(), 2);
        let ActivityEvent::Finished(activity) = &lifecycle[1].event else {
            panic!("finished key");
        };
        assert_eq!(activity.kind, owc_domain::Kind::MythicPlus);
        assert_eq!(activity.outcome, Outcome::Success);
        assert_eq!(activity.end_ms.expect("end") - activity.start_ms, 2_700_000);
        for kind in ["boss_start", "boss_end", "death"] {
            assert_eq!(
                activity.markers.iter().filter(|m| m.kind == kind).count(),
                5
            );
        }
    }

    #[test]
    fn delve_boundaries_publish_sourced_lifecycle_events() {
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let file = config.logs.join("WoWCombatLog-delve.txt");
        let now = 1_768_478_400_000;
        let mut state = LogState::new(&config);
        poll(&config, &mut state, now, i64::MAX);
        append(
            &file,
            "01/15/2026 12:00:00.000  ZONE_CHANGE,100,Test delve,208",
        );
        append(&file, "01/15/2026 12:00:01.000  ZONE_CHANGE,1,World,0");
        let (events, errors, _) = poll(&config, &mut state, now, i64::MAX);
        assert_eq!(errors, 0);
        let activities = events
            .iter()
            .filter_map(|event| match &event.event {
                ActivityEvent::Started(a) | ActivityEvent::Finished(a) => Some(a),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(activities.len(), 2);
        for activity in &activities {
            assert_eq!(activity.kind, owc_domain::Kind::Delve);
            assert_eq!(activity.installation_id, Some(config.id));
            let restored: Activity =
                serde_json::from_value(serde_json::to_value(activity).expect("serialize"))
                    .expect("restore");
            assert_eq!(restored.kind, owc_domain::Kind::Delve);
        }
        assert_eq!(activities[0].id, activities[1].id);
        assert_eq!(activities[1].outcome, Outcome::Unknown);
        assert!(activities[1].end_ms.is_some());
    }

    #[test]
    fn unread_log_backlog_cannot_expire_an_active_key() {
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let file = config.logs.join("WoWCombatLog-backlog.txt");
        let start = "01/15/2026 12:00:00.000  CHALLENGE_MODE_START,Dungeon,100,200,12,[]";
        let at = owc_wow_log::parse(start, 1_768_478_400_000)
            .expect("start")
            .at;
        let mut state = LogState::new(&config);
        poll(&config, &mut state, at, 300_000);
        let mut contents = format!("{start}\n");
        let action = "01/15/2026 12:00:01.000  SPELL_CAST_SUCCESS,Player-1,Player,0x511,0,Creature-1,Enemy,0xa48,0,1,Spell,1\n";
        while contents.len() < 1_100_000 {
            contents.push_str(action);
        }
        contents.push_str("01/15/2026 12:10:00.000  CHALLENGE_MODE_END,100,1,12,600000\n");
        std::fs::write(&file, contents).expect("backlog");
        let (events, _, _) = poll(&config, &mut state, at + 600_000, 300_000);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e.event, ActivityEvent::Abandoned { .. }))
        );
        assert!(state.detector.active.is_some());
        let (events, _, _) = poll(&config, &mut state, at + 600_000, 300_000);
        assert!(events.iter().any(
            |e| matches!(&e.event, ActivityEvent::Finished(a) if a.outcome == Outcome::Success)
        ));
    }

    #[test]
    fn death_recaps_reach_activity_and_manual_marker_events() {
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let file = config.logs.join("WoWCombatLog-test.txt");
        let now = 1_768_478_400_000;
        let mut state = LogState::new(&config);
        poll(&config, &mut state, now, i64::MAX);
        for line in [
            "01/15/2026 12:00:00.000  ENCOUNTER_START,1001,Boss,16,20",
            "01/15/2026 12:00:01.000  SPELL_DAMAGE,Creature-1,Boss,0,0,Player-1,Player,0,0,123,Fireball,4,999",
            "01/15/2026 12:00:02.000  UNIT_DIED,0,nil,0,0,Player-1,Player,0,0,0",
            "01/15/2026 12:00:03.000  ENCOUNTER_END,1001,Boss,16,20,0",
        ] {
            append(&file, line);
        }
        let (events, errors, _) = poll(&config, &mut state, now, i64::MAX);
        assert_eq!(errors, 0);
        let marker = events
            .iter()
            .find_map(|event| match &event.event {
                ActivityEvent::Marker(marker) if marker.kind == "death" => Some(marker),
                _ => None,
            })
            .expect("manual marker");
        assert_eq!(marker.recent_hits.len(), 1);
        assert_eq!(marker.recent_hits[0].offset_ms, -1000);
        let activity = events
            .iter()
            .find_map(|event| match &event.event {
                ActivityEvent::Finished(activity) => Some(activity),
                _ => None,
            })
            .expect("finished activity");
        let restored: Activity =
            serde_json::from_value(serde_json::to_value(activity).expect("serialize"))
                .expect("restore");
        let death = restored
            .markers
            .iter()
            .find(|marker| marker.kind == "death")
            .expect("death");
        assert_eq!(death.recent_hits[0].ability, "Fireball");
    }

    #[test]
    fn dungeon_exit_publishes_abandonment_with_installation() {
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let file = config.logs.join("WoWCombatLog-test.txt");
        let now = 1_768_478_400_000;
        let mut state = LogState::new(&config);
        poll(&config, &mut state, now, i64::MAX);
        append(
            &file,
            "01/15/2026 12:00:00.000  CHALLENGE_MODE_START,Dungeon,100,200,12,[]",
        );
        append(&file, "01/15/2026 12:00:01.000  ZONE_CHANGE,1,World,0");
        poll(&config, &mut state, now, i64::MAX);
        let deadline = state.detector.dungeon_exit_ms.expect("exit") + 60_000;
        let (events, _, _) = poll(&config, &mut state, deadline, i64::MAX);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.installation_id, config.id);
        assert_eq!(events[0].source.log_file.as_ref(), Some(&file));
        assert!(
            matches!(&events[0].event, ActivityEvent::Abandoned { activity, reason: AbandonReason::DungeonExit } if activity.installation_id == Some(config.id))
        );
        assert!(state.detector.active.is_none());
    }
    #[test]
    fn lifecycle_preserves_installation_and_source_identity() {
        let root = tempfile::tempdir().expect("root");
        let now = 1_768_478_400_000;
        let mut ids = vec![];
        for name in ["one", "two"] {
            let dir = root.path().join(name);
            std::fs::create_dir(&dir).expect("directory");
            let config = installation(dir);
            let file = config.logs.join("WoWCombatLog-test.txt");
            let mut state = LogState::new(&config);
            poll(&config, &mut state, now, i64::MAX);
            append(
                &file,
                "01/15/2026 12:00:00.000  ENCOUNTER_START,1001,\"Boss\",16,20",
            );
            let (events, errors, error) = poll(&config, &mut state, now, i64::MAX);
            assert_eq!(errors, 0);
            assert!(error.is_none());
            let ActivityEvent::Started(activity) = &events[0].event else {
                panic!("start");
            };
            ids.push(activity.id);
            assert_eq!(activity.installation_id, Some(config.id));
            assert_eq!(events[0].source.installation_id, config.id);
            assert_eq!(events[0].source.log_file.as_ref(), Some(&file));
            append(
                &file,
                "01/15/2026 12:00:02.000  UNIT_DIED,0000000000000000,nil,0x80000000,0x80000000,Player-1-TEST,\"Player\",0x511,0x0,0",
            );
            let (events, _, _) = poll(&config, &mut state, now, i64::MAX);
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e.event, ActivityEvent::Updated(_)))
            );
            append(
                &file,
                "01/15/2026 12:00:03.000  ENCOUNTER_END,1001,\"Boss\",16,20,1",
            );
            let (events, _, _) = poll(&config, &mut state, now, i64::MAX);
            assert!(
                matches!(&events[0].event, ActivityEvent::Finished(a) if a.id == ids[ids.len()-1])
            );
            append(
                &file,
                "01/15/2026 12:00:04.000  ENCOUNTER_START,1001,\"Boss\",16,20",
            );
            poll(&config, &mut state, now, i64::MAX);
            let expire_at = state.detector.last_event_ms.expect("time") + 1001;
            let (events, _, _) = poll(&config, &mut state, expire_at, 1000);
            assert!(matches!(
                &events[0].event,
                ActivityEvent::Abandoned {
                    reason: AbandonReason::Inactivity,
                    ..
                }
            ));
            assert_eq!(events[0].source.log_file.as_ref(), Some(&file));
        }
        assert_ne!(ids[0], ids[1]);
    }
    #[test]
    fn markers_do_not_require_a_dungeon_start() {
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let mut state = LogState::new(&config);
        poll(&config, &mut state, 0, i64::MAX);
        let path = root.path().join("WoWCombatLog-test.txt");
        for line in [
            "01/15/2026 12:00:00.000  ENCOUNTER_START,1001,\"Dungeon boss\",8,5",
            "01/15/2026 12:00:02.000  UNIT_DIED,0000000000000000,nil,0x80000000,0x80000000,Player-1-TEST,\"Player\",0x511,0x0,0",
            "01/15/2026 12:00:03.000  ENCOUNTER_END,1001,\"Dungeon boss\",8,5,1",
        ] {
            append(&path, line);
        }
        let (events, errors, error) = poll(&config, &mut state, 1_768_478_400_000, i64::MAX);
        assert_eq!(errors, 0);
        assert!(error.is_none());
        assert!(state.detector.active.is_none());
        let kinds: Vec<_> = events
            .iter()
            .map(|event| {
                assert_eq!(event.source.installation_id, config.id);
                assert_eq!(event.source.log_file.as_ref(), Some(&path));
                let ActivityEvent::Marker(marker) = &event.event else {
                    panic!("marker only");
                };
                marker.kind.as_str()
            })
            .collect();
        assert_eq!(kinds, ["boss_start", "death", "boss_end"]);
        assert!(
            poll(&config, &mut state, 1_768_478_400_000, i64::MAX)
                .0
                .is_empty()
        );
    }

    #[test]
    fn combat_samples_are_group_scoped_bounded_and_checkpointed() {
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let mut state = LogState::new(&config);
        poll(&config, &mut state, 0, i64::MAX);
        let path = root.path().join("WoWCombatLog-test.txt");
        append(
            &path,
            "01/15/2026 12:00:00.000  CHALLENGE_MODE_START,Dungeon,100,200,12,[]",
        );
        for second in ["01.000", "01.100", "02.000", "12.000"] {
            append(
                &path,
                &format!(
                    "01/15/2026 12:00:{second}  SPELL_DAMAGE,Player-1,Player,0x511,0x0,Creature-1,Enemy,0xa48,0x0,123,Spell,1,42"
                ),
            );
        }
        append(
            &path,
            "01/15/2026 12:00:13.000  SPELL_DAMAGE,Creature-1,Enemy,0xa48,0x0,Creature-2,Enemy,0xa48,0x0,123,Spell,1,42",
        );
        append(
            &path,
            "01/15/2026 12:00:14.000  SPELL_HEAL,Player-1,Player,0x511,0x0,Player-2,Friend,0x512,0x0,123,Heal,1,42",
        );
        let (events, _, _) = poll(&config, &mut state, 1_768_478_400_000, i64::MAX);
        assert_eq!(events.iter().filter(|e| matches!(&e.event, ActivityEvent::Marker(marker) if marker.kind == "combat_activity")).count(),3);
        assert_eq!(
            state.detector.active.as_ref().expect("key").markers.len(),
            3
        );
        let mut restored: LogState =
            serde_json::from_value(serde_json::to_value(&state).expect("save")).expect("restore");
        append(
            &path,
            "01/15/2026 12:00:12.500  SWING_DAMAGE,Player-1,Player,0x511,0x0,Creature-1,Enemy,0xa48,0x0,42",
        );
        assert!(
            poll(&config, &mut restored, 1_768_478_400_000, i64::MAX)
                .0
                .is_empty()
        );
    }

    fn throughput_fixture() -> String {
        use chrono::TimeZone;
        use std::fmt::Write as _;
        let start = chrono::Utc::now().timestamp_millis() - 100_000;
        let line = |at, body: &str| {
            format!(
                "{}  {body}\n",
                chrono::Local
                    .timestamp_millis_opt(at)
                    .single()
                    .expect("time")
                    .format("%m/%d/%Y %H:%M:%S%.3f")
            )
        };
        let mut fixture = line(
            start,
            "CHALLENGE_MODE_START,Throughput dungeon,100,200,20,[]",
        );
        for index in 0..100_000 {
            let body = if index % 1000 == 999 {
                "UNIT_DIED,0,nil,0,0,Player-1,Player,0x511,0,0"
            } else {
                "SPELL_DAMAGE,Player-1,Player,0x511,0,Creature-1,Enemy,0xa48,0,123,Spell,1,42"
            };
            fixture.write_str(&line(start + index, body)).expect("line");
        }
        fixture.push_str(&line(start + 100_000, "CHALLENGE_MODE_END,100,1,20,100000"));
        fixture
    }

    #[tokio::test]
    #[ignore = "release throughput qualification; set OWC_INGESTION_REPORT and run in release mode"]
    async fn ingestion_pipeline_ten_times_reference_rate() {
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let mut initial = LogState::new(&config);
        poll(&config, &mut initial, 0, i64::MAX);
        let fixture = throughput_fixture();
        let bytes = fixture.len();
        let file = root.path().join("WoWCombatLog-throughput.txt");
        std::fs::write(&file, fixture).expect("fixture");
        let mut db =
            owc_library::Library::open(&root.path().join("library.sqlite")).expect("database");
        let (bus, mut receiver) = EventBus::new();
        let began = std::time::Instant::now();
        let task = spawn(config, initial, 1, Duration::from_mins(5), false, bus);
        let mut batches = 0;
        let mut starts = 0;
        let mut deaths = 0;
        let mut combat = 0;
        let mut max_queued = 0;
        loop {
            let CaptureEvent::Logs(batch) =
                tokio::time::timeout(Duration::from_secs(30), receiver.recv())
                    .await
                    .expect("bounded delivery")
                    .expect("batch")
            else {
                panic!("logs");
            };
            max_queued = max_queued.max(receiver.len());
            batches += 1;
            assert_eq!(batch.parse_errors, 0);
            assert!(batch.error.is_none());
            let mut completed = vec![];
            for event in batch.events {
                match event.event {
                    ActivityEvent::Started(_) => starts += 1,
                    ActivityEvent::Marker(marker) if marker.kind == "death" => deaths += 1,
                    ActivityEvent::Marker(marker) if marker.kind == "combat_activity" => {
                        combat += 1;
                    }
                    ActivityEvent::Finished(activity) => completed.push(activity),
                    ActivityEvent::Abandoned { .. } => panic!("backlog abandoned"),
                    _ => {}
                }
            }
            db.checkpoint(
                "logs",
                &serde_json::to_value(&batch.state).expect("state"),
                &completed,
            )
            .expect("durable checkpoint");
            let finished = !completed.is_empty();
            if finished {
                assert_eq!(completed.len(), 1);
                assert_eq!(completed[0].outcome, Outcome::Success);
                assert_eq!(batch.state.tailer.files[&file].offset, bytes as u64);
            }
            batch.committed.send(()).expect("ack");
            if finished {
                break;
            }
        }
        let elapsed = began.elapsed();
        task.abort();
        assert_eq!((starts, deaths, combat), (1, 100, 100));
        assert_eq!(
            max_queued, 0,
            "one outstanding acknowledged batch per watcher"
        );
        assert!(batches > 1);
        assert_eq!(db.list().expect("library").len(), 1);
        let report = serde_json::json!({"fixture_lines":100_002,"fixture_bytes":bytes,"represented_seconds":100,
            "reference_events_per_second":1000,"target_multiplier":10,"elapsed_seconds":elapsed.as_secs_f64(),
            "lines_per_second":100_002.0/elapsed.as_secs_f64(),"batches":batches,"max_waiting_batches":max_queued,
            "starts":starts,"deaths":deaths,"sampled_combat_markers":combat,"completed":1});
        let destination = std::env::var("OWC_INGESTION_REPORT").expect("set OWC_INGESTION_REPORT");
        std::fs::write(
            destination,
            serde_json::to_vec_pretty(&report).expect("report"),
        )
        .expect("write report");
        assert!(
            elapsed < Duration::from_secs(10),
            "failed 10x synthetic reference rate: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn watcher_waits_for_checkpoint_acknowledgement() {
        let root = tempfile::tempdir().expect("root");
        let config = installation(root.path().into());
        let file = root.path().join("WoWCombatLog-test.txt");
        let mut initial = LogState::new(&config);
        poll(&config, &mut initial, 0, i64::MAX);
        append(
            &file,
            "01/15/2026 12:00:00.000  ENCOUNTER_START,1001,\"Boss\",16,20",
        );
        let (bus, mut receiver) = EventBus::new();
        let task = spawn(
            config.clone(),
            initial,
            7,
            Duration::from_hours(8760),
            false,
            bus,
        );
        let CaptureEvent::Logs(batch) =
            tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("batch timeout")
                .expect("batch")
        else {
            panic!("log batch");
        };
        assert_eq!(batch.generation, 7);
        append(
            &file,
            "01/15/2026 12:00:03.000  ENCOUNTER_END,1001,\"Boss\",16,20,1",
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(350), receiver.recv())
                .await
                .is_err()
        );
        batch.committed.send(()).expect("ack");
        let CaptureEvent::Logs(next) =
            tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("next timeout")
                .expect("next")
        else {
            panic!("log batch");
        };
        assert!(
            next.events
                .iter()
                .any(|e| matches!(e.event, ActivityEvent::Finished(_)))
        );
        task.abort();
    }
}
