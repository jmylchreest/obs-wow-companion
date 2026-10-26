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
            });
        }
        config.validate().expect("different folders");
        config.installations[1].logs = config.installations[0].logs.clone();
        assert!(config.validate().is_err());
    }
}
