//! Incremental combat-log reader and deterministic activity detection.
mod death_recaps;
mod keystone_timers;
use chrono::{Datelike, Local, NaiveDateTime, TimeZone, Utc};
pub use death_recaps::DeathRecaps;
use owc_domain::{Activity, Kind, Outcome};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid combat log: {0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
#[derive(Debug, Clone)]
pub struct Event {
    pub at: i64,
    pub fields: Vec<String>,
}
impl Event {
    pub fn kind(&self) -> &str {
        self.field(0)
    }
    pub fn field(&self, i: usize) -> &str {
        self.fields.get(i).map_or("", String::as_str)
    }
    pub fn number(&self, i: usize) -> Option<u32> {
        self.field(i).parse().ok()
    }
}
/// Tokenizes quoted fields and nested arrays without treating their commas as top-level separators.
pub fn fields(input: &str) -> Result<Vec<String>, Error> {
    let mut result = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut depth = 0_u32;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            '[' | '(' if !quoted => {
                depth += 1;
                field.push(c);
            }
            ']' | ')' if !quoted => {
                depth = depth.saturating_sub(1);
                field.push(c);
            }
            ',' if !quoted && depth == 0 => {
                result.push(field.trim().to_owned());
                field.clear();
            }
            _ => field.push(c),
        }
    }
    if quoted || depth != 0 {
        return Err(Error::Invalid("unterminated field".into()));
    }
    result.push(field.trim().to_owned());
    Ok(result)
}
pub fn parse(line: &str, now_ms: i64) -> Result<Event, Error> {
    let (stamp, body) = line
        .trim_end()
        .split_once("  ")
        .ok_or_else(|| Error::Invalid("missing timestamp separator".into()))?;
    let year = Utc
        .timestamp_millis_opt(now_ms)
        .single()
        .ok_or_else(|| Error::Invalid("reference time".into()))?
        .year();
    let has_year = stamp
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .matches('/')
        .count()
        == 2;
    let full = if has_year {
        stamp.to_owned()
    } else {
        format!("{year}/{stamp}")
    };
    let fmt = if has_year {
        "%m/%d/%Y %H:%M:%S%.f"
    } else {
        "%Y/%m/%d %H:%M:%S%.f"
    };
    let mut naive =
        NaiveDateTime::parse_from_str(&full, fmt).map_err(|e| Error::Invalid(e.to_string()))?;
    if !has_year {
        // Logs without a year can straddle New Year; use the closest of adjacent years.
        naive = [year - 1, year, year + 1]
            .into_iter()
            .filter_map(|y| naive.with_year(y))
            .min_by_key(|n| (n.and_utc().timestamp_millis() - now_ms).abs())
            .unwrap_or(naive);
    }
    let at = Local
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| Error::Invalid("ambiguous or nonexistent local time".into()))?
        .timestamp_millis();
    let f = fields(body)?;
    if f.first().is_none_or(String::is_empty) {
        return Err(Error::Invalid("missing event".into()));
    }
    Ok(Event { at, fields: f })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Flavour {
    Retail,
    Classic,
    Era,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detector {
    pub installation: String,
    pub flavour: Flavour,
    pub active: Option<Activity>,
    pub last_event_ms: Option<i64>,
    #[serde(default)]
    pub last_activity_ms: Option<i64>,
    #[serde(default)]
    pub dungeon_exit_ms: Option<i64>,
    pub character: Option<String>,
    pub battleground_zones: Vec<u32>,
    pub arena_zones: Vec<u32>,
}
impl Detector {
    pub fn new(installation: String, flavour: Flavour) -> Self {
        Self {
            installation,
            flavour,
            active: None,
            last_event_ms: None,
            last_activity_ms: None,
            dungeon_exit_ms: None,
            character: None,
            battleground_zones: vec![
                30, 489, 529, 566, 607, 628, 726, 727, 761, 998, 1105, 1803, 2107, 2118,
            ],
            arena_zones: vec![
                559, 562, 572, 617, 618, 980, 1134, 1504, 1552, 1672, 1825, 1911, 2167, 2373, 2509,
                2511, 2547,
            ],
        }
    }
    fn begin(&mut self, kind: Kind, name: String, at: i64) {
        let mut a = Activity::new(kind, name, at, &self.installation);
        a.character.clone_from(&self.character);
        self.dungeon_exit_ms = None;
        self.last_activity_ms = Some(at);
        self.active = Some(a);
    }
    fn end(&mut self, at: i64, outcome: Outcome, out: &mut Vec<Activity>) {
        self.dungeon_exit_ms = None;
        if let Some(mut a) = self.active.take() {
            a.finish(at, outcome);
            out.push(a);
        }
    }
    /// Returns completed activities. The caller persists `active` and cursor in the same transaction.
    #[allow(clippy::too_many_lines)] // A single event dispatch makes mode transitions auditable.
    pub fn ingest(&mut self, e: &Event) -> Vec<Activity> {
        let mut out = vec![];
        self.expire_dungeon_exit(e.at, &mut out);
        if self.last_activity_ms.is_none() {
            self.last_activity_ms = self.last_event_ms;
        }
        self.last_event_ms = Some(e.at);
        let group_event = [e.field(3), e.field(7)].iter().any(|flags| {
            u32::from_str_radix(flags.trim_start_matches("0x"), 16)
                .is_ok_and(|flags| flags & 0x7 != 0)
        });
        let action = matches!(
            e.kind(),
            "ENCOUNTER_START" | "ENCOUNTER_END" | "CHALLENGE_MODE_START" | "CHALLENGE_MODE_END"
        ) || (group_event
            && (e.kind().ends_with("_DAMAGE")
                || e.kind().ends_with("_HEAL")
                || e.kind().ends_with("_MISSED")
                || matches!(
                    e.kind(),
                    "SPELL_CAST_SUCCESS"
                        | "SPELL_CAST_START"
                        | "SPELL_INTERRUPT"
                        | "SPELL_DISPEL"
                        | "UNIT_DIED"
                        | "SPELL_RESURRECT"
                )));
        if action && self.dungeon_exit_ms.is_none() {
            self.last_activity_ms = Some(e.at);
        }
        if (e.kind().starts_with("SPELL_") || e.kind().starts_with("SWING_"))
            && u32::from_str_radix(e.field(3).trim_start_matches("0x"), 16)
                .is_ok_and(|f| f & 1 != 0)
            && e.field(1).starts_with("Player-")
        {
            self.character = Some(e.field(2).into());
            if let Some(a) = &mut self.active {
                a.character.clone_from(&self.character);
            }
        }
        match e.kind() {
            "ENCOUNTER_START" if e.number(1).is_some() && e.number(3).is_some() => {
                if let Some(a) = &mut self.active {
                    if a.kind == Kind::MythicPlus {
                        a.marker(e.at, "boss_start", e.field(2).into());
                        return out;
                    }
                    if a.kind == Kind::Raid && a.encounter == e.number(1) {
                        return out;
                    }
                    if a.kind == Kind::Manual {
                        return out;
                    }
                }
                // Only raid difficulties; dungeon bosses without a challenge run are not raids.
                if ![
                    3, 4, 5, 6, 7, 9, 14, 15, 16, 17, 148, 175, 176, 186, 193, 194, 198, 201, 205,
                    215, 216,
                ]
                .contains(&e.number(3).unwrap_or(0))
                {
                    return out;
                }
                self.end(e.at, Outcome::Abandoned, &mut out);
                self.begin(Kind::Raid, e.field(2).into(), e.at);
                if let Some(a) = &mut self.active {
                    a.encounter = e.number(1);
                    a.difficulty = e.number(3);
                }
            }
            "ENCOUNTER_END" => {
                if let Some(a) = &mut self.active {
                    if a.kind == Kind::MythicPlus {
                        a.marker(
                            e.at,
                            "boss_end",
                            format!(
                                "{}: {}",
                                e.field(2),
                                if e.field(5) == "1" { "kill" } else { "wipe" }
                            ),
                        );
                    } else if a.kind == Kind::Raid && a.encounter == e.number(1) {
                        self.end(
                            e.at,
                            if e.field(5) == "1" {
                                Outcome::Success
                            } else {
                                Outcome::Failure
                            },
                            &mut out,
                        );
                    }
                }
            }
            "CHALLENGE_MODE_START" if e.number(4).is_some() => {
                if self
                    .active
                    .as_ref()
                    .is_some_and(|a| a.kind == Kind::MythicPlus || a.kind == Kind::Manual)
                {
                    return out;
                }
                self.end(e.at, Outcome::Abandoned, &mut out);
                self.begin(Kind::MythicPlus, e.field(1).into(), e.at);
                if let Some(a) = &mut self.active {
                    a.zone = e.number(2);
                    a.key_level = e.number(4);
                    a.challenge_mode_id = e.number(3);
                    a.keystone_time_limit_ms = a
                        .challenge_mode_id
                        .and_then(|id| keystone_timers::limit_ms(id, e.at));
                }
            }
            "CHALLENGE_MODE_END"
                if self
                    .active
                    .as_ref()
                    .is_some_and(|a| a.kind == Kind::MythicPlus) =>
            {
                if e.field(2) == "1"
                    && let Some(a) = &mut self.active
                {
                    a.keystone_elapsed_ms =
                        e.field(4).parse::<i64>().ok().filter(|value| *value > 0);
                    a.keystone_upgrade = a.keystone_elapsed_ms.zip(a.keystone_time_limit_ms).map(
                        |(elapsed, limit)| {
                            if elapsed < limit * 3 / 5 + 1000 {
                                3
                            } else if elapsed < limit * 4 / 5 + 1000 {
                                2
                            } else {
                                u8::from(elapsed < limit + 1000)
                            }
                        },
                    );
                }
                let depleted = self
                    .active
                    .as_ref()
                    .is_some_and(|a| a.keystone_upgrade == Some(0));
                self.end(
                    e.at,
                    if depleted {
                        Outcome::Failure
                    } else if e.field(2) == "1" {
                        Outcome::Success
                    } else {
                        Outcome::Abandoned
                    },
                    &mut out,
                );
            }
            "ARENA_MATCH_START" => {
                let shuffle = e.field(3) == "Rated Solo Shuffle";
                if shuffle
                    && self
                        .active
                        .as_ref()
                        .is_some_and(|a| a.kind == Kind::SoloShuffle)
                {
                    if let Some(a) = &mut self.active {
                        a.marker(e.at, "round_start", "Round start".into());
                    }
                } else {
                    self.end(e.at, Outcome::Abandoned, &mut out);
                    self.begin(
                        if shuffle {
                            Kind::SoloShuffle
                        } else {
                            Kind::Arena
                        },
                        e.field(3).into(),
                        e.at,
                    );
                    if let Some(a) = &mut self.active {
                        a.zone = e.number(1);
                        a.marker(e.at, "round_start", "Round start".into());
                    }
                }
            }
            "ARENA_MATCH_END"
                if self
                    .active
                    .as_ref()
                    .is_some_and(|a| matches!(a.kind, Kind::Arena | Kind::SoloShuffle)) =>
            {
                if let Some(a) = &mut self.active {
                    a.marker(e.at, "result", format!("Winning team {}", e.field(1)));
                }
                // A team ID does not identify whether the local player won.
                self.end(e.at, Outcome::Unknown, &mut out);
            }
            "ZONE_CHANGE" => {
                let zone = e.number(1);
                if let Some(activity) = &self.active
                    && activity.kind == Kind::MythicPlus
                    && let (Some(dungeon), Some(current)) = (activity.zone, zone)
                {
                    if dungeon == current {
                        self.dungeon_exit_ms = None;
                    } else {
                        self.dungeon_exit_ms.get_or_insert(e.at);
                    }
                }
                let bg = zone.is_some_and(|z| self.battleground_zones.contains(&z));
                let arena = self.flavour != Flavour::Retail
                    && zone.is_some_and(|z| self.arena_zones.contains(&z));
                if self.active.as_ref().is_some_and(|a| {
                    matches!(a.kind, Kind::Arena | Kind::SoloShuffle | Kind::Battleground)
                        && a.zone != zone
                }) {
                    self.end(e.at, Outcome::Unknown, &mut out);
                }
                if self.active.is_none() && (bg || arena) {
                    self.begin(
                        if bg { Kind::Battleground } else { Kind::Arena },
                        e.field(2).into(),
                        e.at,
                    );
                    if let Some(a) = &mut self.active {
                        a.zone = zone;
                        a.warnings
                            .push("Zone boundaries; outcome is not available".into());
                    }
                }
            }
            "UNIT_DIED" if e.field(5).starts_with("Player-") && e.field(9) != "1" => {
                if let Some(a) = &mut self.active {
                    a.marker(e.at, "death", e.field(6).into());
                }
            }
            _ => {}
        }
        out
    }
    fn expire_dungeon_exit(&mut self, now: i64, out: &mut Vec<Activity>) {
        if let Some(exit) = self.dungeon_exit_ms
            && now.saturating_sub(exit) >= 60_000
            && self
                .active
                .as_ref()
                .is_some_and(|a| a.kind == Kind::MythicPlus)
        {
            if let Some(activity) = &mut self.active {
                activity.marker(exit, "dungeon_exit", "Left dungeon".into());
            }
            self.end(exit + 60_000, Outcome::Abandoned, out);
        }
    }
    pub fn inactivity_since(&self) -> Option<i64> {
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.kind == Kind::MythicPlus)
        {
            self.last_activity_ms
                .or(self.last_event_ms)
                .or_else(|| self.active.as_ref().map(|a| a.start_ms))
        } else {
            self.last_event_ms
        }
    }
    pub fn expire(&mut self, now: i64, timeout_ms: i64) -> Vec<Activity> {
        let mut out = vec![];
        self.expire_dungeon_exit(now, &mut out);
        if self.active.is_some()
            && self
                .inactivity_since()
                .is_some_and(|last| now.saturating_sub(last) >= timeout_ms)
        {
            self.end(now, Outcome::Abandoned, &mut out);
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cursor {
    pub offset: u64,
    pub identity: String,
    pub pending: Vec<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tailer {
    pub files: BTreeMap<PathBuf, Cursor>,
    pub initialized: bool,
    #[serde(skip)]
    pub has_backlog: bool,
}
pub struct SourceLine {
    pub path: PathBuf,
    pub text: String,
    pub identity: String,
    pub offset: u64,
}
impl Tailer {
    /// Bounded to 1 MiB per file per poll. Cursor advances only as bytes are read.
    pub fn poll(&mut self, dir: &Path) -> Result<Vec<String>, Error> {
        Ok(self
            .poll_sources(dir)?
            .into_iter()
            .map(|line| line.text)
            .collect())
    }
    pub fn poll_sources(&mut self, dir: &Path) -> Result<Vec<SourceLine>, Error> {
        self.has_backlog = false;
        let mut paths = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("WoWCombatLog"))
                    && p.extension().is_some_and(|e| e == "txt")
            })
            .collect::<Vec<_>>();
        paths.sort_by_key(|p| (fs::metadata(p).and_then(|m| m.modified()).ok(), p.clone()));
        let mut lines = vec![];
        for path in paths {
            #[cfg(windows)]
            let before = file_id::get_file_id(&path)?;
            let mut f = File::open(&path)?;
            let meta = f.metadata()?;
            #[cfg(unix)]
            let identity = {
                use std::os::unix::fs::MetadataExt;
                format!("{}:{}", meta.dev(), meta.ino())
            };
            #[cfg(windows)]
            let identity = {
                let after = file_id::get_file_id(&path)?;
                if before != after {
                    continue;
                }
                match after {
                    file_id::FileId::HighRes {
                        volume_serial_number,
                        file_id,
                    } => format!("win-high:{volume_serial_number}:{file_id}"),
                    file_id::FileId::LowRes {
                        volume_serial_number,
                        file_index,
                    } => format!("win-low:{volume_serial_number}:{file_index}"),
                    file_id::FileId::Inode {
                        device_id,
                        inode_number,
                    } => format!("{device_id}:{inode_number}"),
                }
            };
            #[cfg(not(any(unix, windows)))]
            let identity = format!("{:?}", meta.created().ok());
            let state = self.files.entry(path.clone()).or_insert_with(|| Cursor {
                offset: if self.initialized { 0 } else { meta.len() },
                identity: identity.clone(),
                pending: vec![],
            });
            if state.identity != identity || meta.len() < state.offset {
                state.offset = 0;
                state.pending.clear();
                state.identity = identity;
            }
            f.seek(SeekFrom::Start(state.offset))?;
            let mut bytes = vec![];
            f.take(1_048_576).read_to_end(&mut bytes)?;
            state.offset += bytes.len() as u64;
            self.has_backlog |= state.offset < meta.len();
            state.pending.extend(bytes);
            if let Some(last) = state.pending.iter().rposition(|b| *b == b'\n') {
                let mut offset = state.offset - state.pending.len() as u64;
                let complete = state.pending.drain(..=last).collect::<Vec<_>>();
                for raw in complete.split_inclusive(|b| *b == b'\n') {
                    let line_offset = offset;
                    offset += raw.len() as u64;
                    let raw = raw.strip_suffix(b"\n").unwrap_or(raw);
                    if raw.is_empty() {
                        continue;
                    }
                    if let Ok(line) = std::str::from_utf8(raw) {
                        lines.push(SourceLine {
                            path: path.clone(),
                            text: line.trim_end().into(),
                            identity: state.identity.clone(),
                            offset: line_offset,
                        });
                    }
                }
            }
            if state.pending.len() > 4_194_304 {
                state.pending.clear();
                return Err(Error::Invalid("log line exceeds 4 MiB".into()));
            }
        }
        self.initialized = true;
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_offsets_survive_partial_utf8_and_checkpoint() {
        let dir = tempfile::tempdir().expect("directory");
        let path = dir.path().join("WoWCombatLog-offsets.txt");
        fs::write(&path, b"").expect("create");
        let mut tailer = Tailer::default();
        tailer.poll_sources(dir.path()).expect("initialize");
        let first = "évent\r\n";
        let bytes = [b"\n".as_slice(), first.as_bytes(), b"second\n"].concat();
        fs::write(&path, &bytes[..2]).expect("partial UTF-8");
        assert!(tailer.poll_sources(dir.path()).expect("partial").is_empty());
        let saved = serde_json::to_vec(&tailer).expect("checkpoint");
        let mut tailer: Tailer = serde_json::from_slice(&saved).expect("restore");
        fs::write(&path, &bytes).expect("complete");
        let lines = tailer.poll_sources(dir.path()).expect("lines");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "évent");
        assert_eq!(lines[0].offset, 1);
        assert_eq!(lines[1].offset, 1 + first.len() as u64);
        assert_eq!(lines[1].text, "second");
        assert!(!lines[0].identity.is_empty());
        assert_eq!(lines[0].identity, lines[1].identity);
        fs::write(&path, b"new\n").expect("truncate");
        let lines = tailer.poll_sources(dir.path()).expect("truncated");
        assert_eq!(lines[0].offset, 0);
        assert_eq!(lines[0].text, "new");
    }

    fn event(at: i64, text: &str) -> Event {
        Event {
            at,
            fields: fields(text).expect("fixture"),
        }
    }
    #[test]
    fn keystone_result_uses_logged_elapsed_time_and_known_timer() {
        let at = 1_792_946_556_804;
        for (elapsed, upgrade) in [
            (1_080_999, 3),
            (1_081_000, 2),
            (1_440_999, 2),
            (1_441_000, 1),
            (1_800_999, 1),
            (1_801_000, 0),
        ] {
            let mut detector = Detector::new("retail".into(), Flavour::Retail);
            detector.ingest(&event(at, "CHALLENGE_MODE_START,Vale,2859,584,10,[]"));
            let done = detector.ingest(&event(
                at + 5000,
                &format!("CHALLENGE_MODE_END,2859,1,10,{elapsed}"),
            ));
            assert_eq!(done[0].keystone_upgrade, Some(upgrade));
            assert_eq!(done[0].keystone_elapsed_ms, Some(elapsed));
            assert_eq!(
                done[0].outcome,
                if upgrade == 0 {
                    Outcome::Failure
                } else {
                    Outcome::Success
                }
            );
        }
        let mut detector = Detector::new("retail".into(), Flavour::Retail);
        detector.ingest(&event(at, "CHALLENGE_MODE_START,Unknown,999,999,10,[]"));
        let done = detector.ingest(&event(at + 5000, "CHALLENGE_MODE_END,999,1,10,1000"));
        assert_eq!(done[0].keystone_upgrade, None);
        detector.ingest(&event(at, "CHALLENGE_MODE_START,Vale,2859,584,10,[]"));
        let done = detector.ingest(&event(at + 5000, "CHALLENGE_MODE_END,2859,0,0,0"));
        assert_eq!(done[0].keystone_upgrade, None);
        assert_eq!(done[0].outcome, Outcome::Abandoned);
    }
    #[test]
    fn key_inactivity_counts_group_actions_not_background_auras() {
        let mut d = Detector::new("retail".into(), Flavour::Retail);
        d.ingest(&event(0, "CHALLENGE_MODE_START,Dungeon,100,200,21,[]"));
        d.ingest(&event(
            240_000,
            "SPELL_CAST_SUCCESS,Player-1,Player,0x511,0,Creature-1,Enemy,0xa48,0,1,Spell,1",
        ));
        d.ingest(&event(
            530_000,
            "SPELL_AURA_REFRESH,Player-1,Player,0x511,0,Player-1,Player,0x511,0,1,Aura,1",
        ));
        assert!(d.expire(539_999, 300_000).is_empty());
        let done = d.expire(540_000, 300_000);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].outcome, Outcome::Abandoned);
    }
    #[test]
    fn dungeon_exit_grace_survives_checkpoint_and_ignores_outside_activity() {
        let mut d = Detector::new("retail".into(), Flavour::Retail);
        d.ingest(&event(0, "CHALLENGE_MODE_START,Dungeon,100,200,12,[]"));
        d.ingest(&event(1000, "ZONE_CHANGE,100,Dungeon,8"));
        assert!(d.dungeon_exit_ms.is_none());
        d.ingest(&event(2000, "ZONE_CHANGE,1,World,0"));
        d.ingest(&event(5000, "ZONE_CHANGE,2,Other,0"));
        d.ingest(&event(30000, "SPELL_DAMAGE"));
        let mut d: Detector =
            serde_json::from_str(&serde_json::to_string(&d).expect("save")).expect("restore");
        assert!(d.expire(61999, 1_800_000).is_empty());
        let done = d.expire(62000, 1_800_000);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].outcome, Outcome::Abandoned);
        assert_eq!(done[0].end_ms, Some(62000));
        assert!(
            done[0]
                .markers
                .iter()
                .any(|m| m.kind == "dungeon_exit" && m.at_ms == 2000)
        );
        assert!(d.expire(40000, 1_800_000).is_empty());
    }
    #[test]
    fn dungeon_return_cancels_grace_but_late_return_does_not_resurrect() {
        let mut d = Detector::new("retail".into(), Flavour::Retail);
        d.ingest(&event(0, "CHALLENGE_MODE_START,Dungeon,100,200,12,[]"));
        d.ingest(&event(1000, "ZONE_CHANGE,1,World,0"));
        d.ingest(&event(60000, "ZONE_CHANGE,100,Dungeon,8"));
        assert!(d.expire(60000, 1_800_000).is_empty());
        assert!(d.active.is_some());
        d.ingest(&event(61000, "ZONE_CHANGE,1,World,0"));
        let done = d.ingest(&event(122_000, "ZONE_CHANGE,100,Dungeon,8"));
        assert_eq!(done[0].outcome, Outcome::Abandoned);
        assert!(d.active.is_none());
        d.ingest(&event(
            123_000,
            "CHALLENGE_MODE_START,Dungeon,100,200,12,[]",
        ));
        d.ingest(&event(124_000, "ZONE_CHANGE,1,World,0"));
        assert_eq!(
            d.ingest(&event(125_000, "CHALLENGE_MODE_END,100,1,12,1000"))[0].outcome,
            Outcome::Success
        );
        assert!(d.expire(190_000, 1_800_000).is_empty());
    }
    #[test]
    fn fields_handle_arrays_quotes_and_unicode() {
        assert_eq!(
            fields("X,\"A, B\",[1,2],\"Zoë\"").expect("parse"),
            vec!["X", "A, B", "[1,2]", "Zoë"]
        );
        assert!(fields("X,\"bad").is_err());
    }
    #[test]
    fn nested_bosses_and_duplicate_key_start_do_not_split_run() {
        let mut d = Detector::new("retail".into(), Flavour::Retail);
        d.ingest(&event(
            0,
            "CHALLENGE_MODE_START,\"Dungeon\",100,200,12,[1,2]",
        ));
        d.ingest(&event(
            100,
            "CHALLENGE_MODE_START,\"Dungeon\",100,200,12,[1,2]",
        ));
        assert!(
            d.ingest(&event(1000, "ENCOUNTER_START,10,\"Boss\",8,5"))
                .is_empty()
        );
        d.ingest(&event(2000, "ENCOUNTER_END,10,\"Boss\",8,5,1"));
        let done = d.ingest(&event(2_700_000, "CHALLENGE_MODE_END,100,1,12,2700000"));
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].kind, Kind::MythicPlus);
        assert_eq!(done[0].markers.len(), 2);
        assert_eq!(done[0].start_ms, 0);
    }
    #[test]
    fn raid_wipe_reset_and_wrong_end() {
        let mut d = Detector::new("classic".into(), Flavour::Classic);
        d.ingest(&event(0, "ENCOUNTER_START,10,Boss,3,10"));
        assert!(
            d.ingest(&event(10, "ENCOUNTER_END,11,Other,3,10,1"))
                .is_empty()
        );
        assert_eq!(
            d.ingest(&event(20, "ENCOUNTER_END,10,Boss,3,10,0"))[0].outcome,
            Outcome::Failure
        );
    }
    #[test]
    fn shuffle_rounds_and_zone_exit() {
        let mut d = Detector::new("r".into(), Flavour::Retail);
        for n in 0..6 {
            d.ingest(&event(
                n * 1000,
                "ARENA_MATCH_START,559,33,Rated Solo Shuffle,1",
            ));
        }
        let done = d.ingest(&event(6000, "ARENA_MATCH_END,0"));
        assert_eq!(done[0].markers.len(), 7);
        assert_eq!(done[0].kind, Kind::SoloShuffle);
        d.ingest(&event(7000, "ZONE_CHANGE,489,Warsong,0"));
        assert!(
            d.ingest(&event(8000, "ZONE_CHANGE,489,Warsong,0"))
                .is_empty()
        );
        assert_eq!(
            d.ingest(&event(9000, "ZONE_CHANGE,1,World,0"))[0].kind,
            Kind::Battleground
        );
    }
    #[test]
    fn tail_partial_utf8_truncation_and_restart() {
        use std::io::Write;
        let temp = tempfile::tempdir().expect("dir");
        let path = temp.path().join("WoWCombatLog-test.txt");
        fs::write(&path, b"old\n").expect("write");
        let mut t = Tailer::default();
        assert!(t.poll(temp.path()).expect("poll").is_empty());
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open");
        f.write_all(&[0xc3]).expect("write");
        assert!(t.poll(temp.path()).expect("poll").is_empty());
        f.write_all(&[0xa9, b'\n']).expect("write");
        assert_eq!(t.poll(temp.path()).expect("poll"), vec!["é"]);
        assert!(t.poll(temp.path()).expect("poll").is_empty());
        fs::write(path, b"new\n").expect("write");
        assert_eq!(t.poll(temp.path()).expect("poll"), vec!["new"]);
    }
}
