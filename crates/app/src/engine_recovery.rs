use super::Engine;
use crate::recovery::{Anchor, Issue, Record, Recovery};
use anyhow::{Context, Result};
use owc_domain::status::{RecoveryReason, RecoveryStage, TimingConfidence};
use owc_domain::{Coverage, plan_clip};

impl Engine {
    pub(super) fn load_recovery(db: &owc_library::Library) -> Result<Recovery> {
        let mut recovery: Recovery = db
            .setting("source_recovery")?
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default();
        // Older builds kept the banner open for historical, terminal recording results.
        if recovery.stage == RecoveryStage::NeedsAttention && !recovery.needs_attention() {
            recovery.stage = RecoveryStage::Complete;
            db.set_setting("source_recovery", &serde_json::to_value(&recovery)?)?;
        }
        Ok(recovery)
    }

    pub(super) fn automatic_status(&self) -> owc_domain::status::AutomaticStatus {
        use owc_domain::status::AutomaticStatus;
        if self.automatic_paused {
            AutomaticStatus::Paused
        } else if self.obs.is_none() {
            AutomaticStatus::ObsBlocked
        } else if !self.setup_completed
            || self.setup["checks"]
                .as_array()
                .is_some_and(|checks| checks.iter().any(|check| check["status"] == "fail"))
        {
            AutomaticStatus::SetupBlocked
        } else {
            AutomaticStatus::On
        }
    }

    pub(super) fn source_anchors(&self) -> Result<Vec<Anchor>> {
        Ok(self
            .db
            .setting("source_anchors")?
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default())
    }
    pub(super) fn persist_source_anchor(&self) -> Result<()> {
        let Some(recording) = &self.recording else {
            return Ok(());
        };
        let Some(path) = &recording.path else {
            return Ok(());
        };
        let mut anchors = self.source_anchors()?;
        // A reused filename must not inherit ownership or timing from a previous source.
        anchors.retain(|anchor| anchor.path != *path);
        anchors.push(Anchor {
            path: path.clone(),
            session: recording.session,
            start_ms: recording.start_ms,
            managed: recording.managed,
            invalid: recording.invalid,
        });
        self.db
            .set_setting("source_anchors", &serde_json::to_value(anchors)?)
    }
    pub(super) fn save_recovery(&self) -> Result<()> {
        self.db
            .set_setting("source_recovery", &serde_json::to_value(&self.recovery)?)
    }
    pub(super) fn begin_source_recovery(&mut self) -> Result<()> {
        anyhow::ensure!(
            !self.export_running && !self.share_running,
            "Wait for current exports to finish before checking sources"
        );
        let indexed = self.db.segments()?;
        let files = crate::recovery::files(&self.config.spool)?
            .into_iter()
            .filter(|path| !indexed.iter().any(|segment| segment.path == *path))
            .collect::<Vec<_>>();
        let affected = self
            .db
            .list()?
            .iter()
            .filter(|a| {
                matches!(a["status"].as_str(), Some("unavailable" | "pending"))
                    || (a["status"] == "failed"
                        && a["id"]
                            .as_str()
                            .is_some_and(|id| self.recovery.records.contains_key(id)))
            })
            .filter_map(|a| a["id"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        self.recovery = Recovery {
            stage: RecoveryStage::Scanning,
            total: files.len(),
            pending: files,
            affected,
            started_ms: Self::now(),
            ..Recovery::default()
        };
        for id in &self.recovery.affected {
            self.recovery.records.insert(
                id.clone(),
                Record {
                    stage: RecoveryStage::Checking,
                    reason: None,
                    confidence: TimingConfidence::Uncertain,
                },
            );
        }
        self.recovery_checked = true;
        self.save_recovery()
    }
    pub(super) async fn delete_recovery_file(
        &mut self,
        path: &std::path::Path,
        confirm_closed: bool,
    ) -> Result<()> {
        anyhow::ensure!(
            !self.recovery.is_running()
                && !self.export_running
                && !self.share_running
                && !self.preview_running
                && self.recording.is_none(),
            "Wait until recording and media work have stopped"
        );
        let issue = self
            .recovery
            .issues
            .iter()
            .find(|issue| issue.path == path)
            .context("File is not awaiting recovery review")?;
        anyhow::ensure!(
            issue.reason == RecoveryReason::EmptySource,
            "This file may still contain recoverable footage"
        );
        let current = crate::recovery::fingerprint(path)?;
        anyhow::ensure!(
            Some(current) == issue.fingerprint && current.0 == 0,
            "File changed; check recordings again before deleting it"
        );
        anyhow::ensure!(
            std::fs::symlink_metadata(path)?.file_type().is_file(),
            "Only regular source files can be deleted"
        );
        let canonical = path.canonicalize()?;
        let spool = self.config.spool.canonicalize()?;
        anyhow::ensure!(
            canonical.parent() == Some(spool.as_path()),
            "File is outside the recording folder"
        );
        anyhow::ensure!(
            !self
                .db
                .segments()?
                .iter()
                .any(|segment| segment.path.canonicalize().ok().as_ref() == Some(&canonical)),
            "Source file is still indexed by recordings"
        );
        if let Some(obs) = &self.obs {
            anyhow::ensure!(
                !obs.output_active("GetRecordStatus").await?,
                "Stop OBS recording before deleting source files"
            );
        } else {
            anyhow::ensure!(confirm_closed, "Confirm OBS has stopped recording");
        }
        // Recheck after the asynchronous OBS query so a growing file is not discarded.
        anyhow::ensure!(
            crate::recovery::fingerprint(path)? == current,
            "File changed; check recordings again"
        );
        std::fs::remove_file(&canonical)?;
        self.recovery.issues.retain(|issue| issue.path != path);
        self.finish_recovery();
        self.save_recovery()
    }
    fn has_new_recovery_work(&self) -> Result<bool> {
        let indexed = self.db.segments()?;
        let new_files = crate::recovery::files(&self.config.spool)?
            .iter()
            .any(|path| {
                !indexed.iter().any(|segment| segment.path == *path)
                    && !self.recovery.issues.iter().any(|issue| issue.path == *path)
            });
        Ok(new_files
            || self.db.list()?.iter().any(|row| {
                matches!(row["status"].as_str(), Some("pending" | "unavailable"))
                    && row["id"]
                        .as_str()
                        .is_some_and(|id| !self.recovery.records.contains_key(id))
            }))
    }
    pub(super) async fn source_recovery_tick(&mut self) -> Result<()> {
        if self.shutting_down || self.export_running || self.share_running {
            return Ok(());
        }
        if !self.recovery_checked {
            self.recovery_checked = true;
            if self.recovery.stage != RecoveryStage::Idle && !self.has_new_recovery_work()? {
                return Ok(());
            }
            self.begin_source_recovery()?;
            if self.recovery.pending.is_empty() && self.recovery.affected.is_empty() {
                self.recovery.stage = RecoveryStage::Idle;
                return self.save_recovery();
            }
        }
        if !self.recovery.is_running() {
            return Ok(());
        }
        if self.recovery.stage == RecoveryStage::Recovering {
            return self.update_recovery_exports();
        }
        self.recovery.waiting = false;
        if !self.recovery.pending.is_empty() || !self.recovery.affected.is_empty() {
            if self.recording.is_some() {
                self.recovery.waiting = true;
                return Ok(());
            }
            if let Some(obs) = &self.obs {
                if obs.output_active("GetRecordStatus").await? {
                    self.recovery.waiting = true;
                    return Ok(());
                }
            } else if !self.recovery_confirmed {
                self.recovery.waiting = true;
                return Ok(());
            }
        }
        if let Some(path) = self.recovery.pending.first().cloned() {
            self.recovery.stage = RecoveryStage::Checking;
            let anchors = self.source_anchors()?;
            let anchor = anchors.iter().find(|anchor| anchor.path == path);
            let canonical = path.canonicalize();
            let result = if canonical
                .as_ref()
                .is_ok_and(|p| p.parent() == self.config.spool.canonicalize().ok().as_deref())
            {
                crate::recovery::inspect(&self.config.media, &path, anchor).await
            } else {
                Err(anyhow::anyhow!("source_outside_spool"))
            };
            match result {
                Ok((segment, confidence)) => {
                    // Index once by path, including when a crash happened before progress was saved.
                    if !self.db.segments()?.iter().any(|old| old.path == path) {
                        self.db.add_segment(&segment)?;
                    }
                    self.db.set_setting(
                        &format!("source_confidence:{}", segment.id),
                        &serde_json::to_value(confidence)?,
                    )?;
                }
                Err(error) => {
                    self.event_log.push(
                        "Recovery",
                        "warning",
                        &format!("{}: {error:#}", path.display()),
                    );
                    let fingerprint = crate::recovery::fingerprint(&path).ok();
                    let reason = if fingerprint.is_some_and(|(bytes, _)| bytes == 0) {
                        RecoveryReason::EmptySource
                    } else {
                        RecoveryReason::TimingUncertain
                    };
                    self.recovery.issues.push(Issue {
                        path,
                        reason,
                        fingerprint,
                    });
                }
            }
            self.recovery.pending.remove(0);
            self.recovery.completed += 1;
            self.save_recovery()?;
            return Ok(());
        }
        self.plan_recovered_sources()
    }
    fn update_recovery_exports(&mut self) -> Result<()> {
        let rows = self.db.list()?;
        let mut waiting = false;
        for id in self.recovery.recovered.clone() {
            let Some(row) = rows.iter().find(|a| a["id"] == id) else {
                self.recovery.records.remove(&id);
                continue;
            };
            let record = self
                .recovery
                .records
                .get_mut(&id)
                .context("recovery record")?;
            match row["status"].as_str() {
                Some("ready") => record.stage = RecoveryStage::Complete,
                Some("failed" | "unavailable") => {
                    record.stage = RecoveryStage::NeedsAttention;
                    record.reason = Some(RecoveryReason::ExportFailed);
                }
                _ => waiting = true,
            }
        }
        if !waiting {
            self.finish_recovery();
        }
        self.save_recovery()
    }
    fn plan_recovered_sources(&mut self) -> Result<()> {
        let segments = self
            .db
            .segments()?
            .into_iter()
            .filter(|segment| segment.path.is_file())
            .collect::<Vec<_>>();
        for id in self.recovery.affected.clone() {
            let Ok(activity) = self.db.activity(&id) else {
                continue;
            };
            let plan = plan_clip(
                &activity,
                &segments,
                i64::from(self.config.pre_seconds) * 1000,
                i64::from(self.config.post_seconds) * 1000,
            );
            let record = self
                .recovery
                .records
                .get_mut(&id)
                .context("recovery record")?;
            match plan.coverage {
                Coverage::Complete | Coverage::Truncated => {
                    record.stage = RecoveryStage::Recovering;
                    record.reason =
                        (plan.coverage == Coverage::Truncated).then_some(RecoveryReason::Partial);
                    record.confidence = if plan.sources.iter().all(|s| s.segment.managed) {
                        TimingConfidence::Anchored
                    } else {
                        TimingConfidence::Corroborated
                    };
                    self.db.retry_recovery(&id)?;
                    self.recovery.recovered.push(id);
                }
                Coverage::Gapped | Coverage::Unavailable => {
                    // Unverified files might still contain the encounter. Do not call those lost.
                    record.stage = if self
                        .recovery
                        .issues
                        .iter()
                        .all(|issue| issue.reason == RecoveryReason::EmptySource)
                    {
                        RecoveryStage::Unrecoverable
                    } else {
                        RecoveryStage::NeedsAttention
                    };
                    record.reason = Some(
                        if self
                            .recovery
                            .issues
                            .iter()
                            .any(|issue| issue.reason != RecoveryReason::EmptySource)
                        {
                            RecoveryReason::TimingUncertain
                        } else if plan.coverage == Coverage::Gapped {
                            RecoveryReason::Gapped
                        } else {
                            RecoveryReason::MissingSource
                        },
                    );
                }
            }
        }
        self.recovery.stage = RecoveryStage::Recovering;
        if self.recovery.recovered.is_empty() {
            self.finish_recovery();
        }
        self.save_recovery()
    }
    fn finish_recovery(&mut self) {
        self.recovery.stage = if self.recovery.needs_attention() {
            RecoveryStage::NeedsAttention
        } else {
            RecoveryStage::Complete
        };
    }
}
