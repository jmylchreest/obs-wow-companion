use anyhow::{Result, bail};
use owc_domain::Kind;
use owc_media::Media;
use owc_wow_log::Flavour;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Installation {
    #[serde(default = "uuid::Uuid::new_v4")]
    pub id: uuid::Uuid,
    pub name: String,
    pub logs: PathBuf,
    pub flavour: Flavour,
    pub categories: Vec<Kind>,
    pub difficulties: Vec<u32>,
    pub minimum_key: u32,
    #[serde(default)]
    pub minimum_raid_difficulty: Option<RaidMinimum>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaidMinimum {
    Normal,
    Heroic,
    Mythic,
}
impl Installation {
    pub fn allows(&self, activity: &owc_domain::Activity) -> bool {
        if !self.categories.contains(&activity.kind)
            || (!self.difficulties.is_empty()
                && activity
                    .difficulty
                    .is_some_and(|id| !self.difficulties.contains(&id)))
        {
            return false;
        }
        if activity.kind == Kind::MythicPlus
            && activity.key_level.is_some_and(|key| key < self.minimum_key)
        {
            return false;
        }
        if activity.kind == Kind::Raid
            && let Some(minimum) = self.minimum_raid_difficulty
        {
            let rank = match activity.difficulty {
                Some(3 | 4 | 9 | 14) => 1,
                Some(5 | 6 | 15) => 2,
                Some(16) => 3,
                _ => 0,
            };
            let required = match minimum {
                RaidMinimum::Normal => 1,
                RaidMinimum::Heroic => 2,
                RaidMinimum::Mythic => 3,
            };
            return rank >= required;
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "markers_visible_by_default")]
    pub show_markers: bool,
    #[serde(default)]
    pub hidden_marker_types: Vec<String>,
    #[serde(default = "default_recordings_width")]
    pub recordings_width: u32,
    #[serde(default)]
    pub recordings_collapsed: bool,
    pub obs_port: u16,
    #[serde(default)]
    pub recording_scene: String,
    pub installations: Vec<Installation>,
    pub spool: PathBuf,
    pub library: PathBuf,
    pub pre_seconds: u32,
    pub post_seconds: u32,
    pub segment_seconds: u32,
    pub history_seconds: u32,
    pub spool_gib: u32,
    pub library_gib: u32,
    pub minimum_free_gib: u32,
    pub activity_timeout_minutes: u32,
    #[serde(default = "fast_exports")]
    pub export_mode: owc_media::CutMode,
    pub media: Media,
}
const fn default_recordings_width() -> u32 {
    320
}
const fn fast_exports() -> owc_media::CutMode {
    owc_media::CutMode::Fast
}
const fn markers_visible_by_default() -> bool {
    true
}
impl Config {
    pub fn new(root: &Path) -> Self {
        Self {
            show_markers: true,
            hidden_marker_types: vec![],
            recordings_width: default_recordings_width(),
            recordings_collapsed: false,
            obs_port: 4455,
            recording_scene: String::new(),
            installations: vec![],
            spool: root.join("spool"),
            library: root.join("videos"),
            pre_seconds: 5,
            post_seconds: 10,
            segment_seconds: 60,
            history_seconds: 120,
            spool_gib: 30,
            library_gib: 100,
            minimum_free_gib: 2,
            activity_timeout_minutes: 5,
            export_mode: fast_exports(),
            media: Media::default(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !(240..=720).contains(&self.recordings_width)
            || self.obs_port == 0
            || !(2..=600).contains(&self.segment_seconds)
            || self.history_seconds < self.pre_seconds.saturating_add(30)
            || self.history_seconds > 3600
            || self.pre_seconds > 300
            || self.post_seconds > 300
            || self.spool_gib == 0
            || self.library_gib == 0
            || !(1..=1440).contains(&self.activity_timeout_minutes)
        {
            bail!("invalid capture duration, port or storage limits");
        }
        if !self.spool.is_absolute() || !self.library.is_absolute() || self.spool == self.library {
            bail!("spool and library must be different absolute paths");
        }
        if self.installations.len() > 10 {
            bail!("at most ten installations");
        }
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        let mut paths = std::collections::HashSet::new();
        for i in &self.installations {
            if i.id.is_nil() || !ids.insert(i.id) {
                bail!("each installation needs a distinct identity");
            }
            if i.name.trim().is_empty() || !names.insert(&i.name) || !i.logs.is_dir() {
                bail!("each installation needs a unique name and existing log directory");
            }
            if !paths.insert(i.logs.canonicalize()?) {
                bail!(
                    "that Logs folder is already configured; each installation needs a different folder"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_filters_are_independent_and_old_preferences_survive() {
        let mut config: Installation = serde_json::from_value(serde_json::json!({
            "name":"Retail", "logs":".", "flavour":"retail",
            "categories":["raid","mythic_plus"], "difficulties":[], "minimum_key":10
        }))
        .expect("old installation");
        assert!(config.minimum_raid_difficulty.is_none());
        let mut raid = owc_domain::Activity::new(Kind::Raid, "Raid".into(), 0, "Retail");
        raid.difficulty = Some(17);
        assert!(config.allows(&raid));
        for (minimum, expected) in [
            (RaidMinimum::Normal, [false, true, true, true]),
            (RaidMinimum::Heroic, [false, false, true, true]),
            (RaidMinimum::Mythic, [false, false, false, true]),
        ] {
            config.minimum_raid_difficulty = Some(minimum);
            for (difficulty, allowed) in [17, 14, 15, 16].into_iter().zip(expected) {
                raid.difficulty = Some(difficulty);
                assert_eq!(config.allows(&raid), allowed);
            }
            raid.difficulty = None;
            assert!(!config.allows(&raid));
        }
        let mut key = owc_domain::Activity::new(Kind::MythicPlus, "Key".into(), 0, "Retail");
        key.key_level = Some(9);
        assert!(!config.allows(&key));
        key.key_level = Some(10);
        assert!(config.allows(&key));
        let mut delve = owc_domain::Activity::new(Kind::Delve, "Delve".into(), 0, "Retail");
        delve.difficulty = Some(208);
        assert!(!config.allows(&delve));
        config.categories.push(Kind::Delve);
        assert!(config.allows(&delve));
        config.categories.retain(|kind| *kind != Kind::MythicPlus);
        assert!(!config.allows(&key));
        config.categories.retain(|kind| *kind != Kind::Raid);
        raid.difficulty = Some(16);
        assert!(!config.allows(&raid));
        assert!(config.allows(&delve));
        config.difficulties = vec![15];
        assert!(!config.allows(&delve));
    }

    #[test]
    fn older_config_defaults_to_fast_cpu_exports() {
        let root = tempfile::tempdir().expect("root");
        let mut value = serde_json::to_value(Config::new(root.path())).expect("serialize");
        value.as_object_mut().expect("object").remove("export_mode");
        value["media"]
            .as_object_mut()
            .expect("media")
            .remove("encoder");
        value["media"]
            .as_object_mut()
            .expect("media")
            .remove("vaapi_device");
        let config: Config = serde_json::from_value(value).expect("old config");
        assert!(matches!(config.export_mode, owc_media::CutMode::Fast));
        assert_eq!(config.media.encoder, owc_media::Encoder::Cpu);
    }
    #[test]
    fn multiple_installations_require_distinct_log_folders() {
        let root = tempfile::tempdir().expect("root");
        let mut config = Config::new(root.path());
        for name in ["Retail", "Classic"] {
            let logs = root.path().join(name);
            std::fs::create_dir_all(&logs).expect("logs");
            config.installations.push(Installation {
                id: uuid::Uuid::new_v4(),
                name: name.into(),
                logs,
                flavour: Flavour::Retail,
                categories: vec![],
                difficulties: vec![],
                minimum_key: 0,
                minimum_raid_difficulty: None,
            });
        }
        config.validate().expect("different folders");
        config.installations[1].logs = config.installations[0].logs.clone();
        assert!(config.validate().is_err());
    }
}
