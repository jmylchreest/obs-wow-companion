//! Conservative source reconciliation. Unknown files are never made cleanup-owned.
use anyhow::{Context, Result, bail};
use owc_domain::{
    Segment,
    status::{RecoveryReason, RecoveryStage, TimingConfidence},
};
use owc_media::Media;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub path: PathBuf,
    pub session: Uuid,
    pub start_ms: i64,
    pub managed: bool,
    pub invalid: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub path: PathBuf,
    pub reason: RecoveryReason,
    #[serde(default)]
    pub fingerprint: Option<(u64, i64)>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recovery {
    #[serde(default)]
    pub records: std::collections::BTreeMap<String, Record>,
    pub stage: RecoveryStage,
    #[serde(default)]
    pub waiting: bool,
    pub completed: usize,
    pub total: usize,
    pub recovered: Vec<String>,
    pub affected: Vec<String>,
    pub issues: Vec<Issue>,
    pub pending: Vec<PathBuf>,
    pub started_ms: i64,
}
impl Recovery {
    pub fn needs_attention(&self) -> bool {
        !self.issues.is_empty()
            || self
                .records
                .values()
                .any(|record| record.stage == RecoveryStage::NeedsAttention)
    }

    pub const fn is_running(&self) -> bool {
        matches!(
            self.stage,
            RecoveryStage::Scanning | RecoveryStage::Checking | RecoveryStage::Recovering
        )
    }
}
impl Default for Recovery {
    fn default() -> Self {
        Self {
            records: std::collections::BTreeMap::default(),
            stage: RecoveryStage::Idle,
            waiting: false,
            completed: 0,
            total: 0,
            recovered: vec![],
            affected: vec![],
            issues: vec![],
            pending: vec![],
            started_ms: 0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub stage: RecoveryStage,
    pub reason: Option<RecoveryReason>,
    pub confidence: TimingConfidence,
}
pub fn files(spool: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = vec![];
    for entry in std::fs::read_dir(spool)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && matches!(
                entry
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("mp4" | "mkv" | "mov" | "flv" | "ts")
            )
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}
pub fn fingerprint(path: &Path) -> Result<(u64, i64)> {
    let meta = std::fs::metadata(path)?;
    Ok((
        meta.len(),
        i64::try_from(meta.modified()?.duration_since(UNIX_EPOCH)?.as_millis())?,
    ))
}
pub fn corroborate(start_ms: i64, duration_ms: i64, modified_ms: i64) -> bool {
    start_ms > 0
        && duration_ms > 0
        && start_ms
            .checked_add(duration_ms)
            .is_some_and(|end| end.abs_diff(modified_ms) <= 5000)
}
pub async fn inspect(
    media: &Media,
    path: &Path,
    anchor: Option<&Anchor>,
) -> Result<(Segment, TimingConfidence)> {
    let before = fingerprint(path)?;
    let info = media.probe(path).await?;
    if fingerprint(path)? != before {
        bail!("source_changed");
    }
    let (start_ms, session, managed, confidence) = if let Some(anchor) = anchor {
        if anchor.invalid || !corroborate(anchor.start_ms, info.duration_ms, before.1) {
            bail!("timing_uncertain");
        }
        if let Some(created) = &info.creation_time {
            let created = chrono::DateTime::parse_from_rfc3339(created)?.timestamp_millis();
            if created.abs_diff(anchor.start_ms) > 5000 {
                bail!("source_identity_changed");
            }
        }
        (
            anchor.start_ms,
            anchor.session,
            anchor.managed,
            TimingConfidence::Anchored,
        )
    } else {
        let created = info
            .creation_time
            .as_deref()
            .context("creation_time_missing")?;
        let start = chrono::DateTime::parse_from_rfc3339(created)
            .context("creation_time_invalid")?
            .timestamp_millis();
        if !corroborate(start, info.duration_ms, before.1) {
            bail!("timing_uncertain");
        }
        (start, Uuid::new_v4(), false, TimingConfidence::Corroborated)
    };
    let end_ms = start_ms
        .checked_add(info.duration_ms)
        .context("timestamp_overflow")?;
    Ok((
        Segment {
            id: Uuid::new_v4(),
            session,
            path: path.into(),
            start_ms,
            end_ms,
            closed: true,
            managed,
        },
        confidence,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_requires_correlated_end_time() {
        assert!(corroborate(100_000, 30_000, 130_100));
        assert!(!corroborate(100_000, 30_000, 3_730_000)); // Pause or timezone mismatch.
        assert!(!corroborate(i64::MAX, 10, 100));
        assert!(!corroborate(0, 30_000, 30_000));
    }
    #[test]
    fn scan_ignores_sidecars_and_directories() {
        let root = tempfile::tempdir().expect("root");
        std::fs::write(root.path().join("source.mp4"), b"fixture").expect("source");
        std::fs::write(root.path().join("source.json"), b"{}").expect("sidecar");
        std::fs::create_dir(root.path().join("folder.mkv")).expect("dir");
        assert_eq!(files(root.path()).expect("scan").len(), 1);
    }
}
