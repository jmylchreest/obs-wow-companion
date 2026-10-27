//! Stable wire codes; presentation belongs to the message catalogue.
use serde::{Deserialize, Serialize};
macro_rules! codes {
    ($name:ident { $($variant:ident => $code:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant,)+ #[serde(other)] Unknown }
        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $code,)+ Self::Unknown => "unknown" }
            }
        }
    };
}
codes!(RecordingStatus { Pending=>"pending", Exporting=>"exporting", Ready=>"ready", Failed=>"failed", Unavailable=>"unavailable" });
codes!(AssetStatus { Generating=>"generating", Ready=>"ready", Failed=>"failed", Cancelled=>"cancelled" });
codes!(ConnectionCode { Unreachable=>"unreachable", PasswordMissing=>"password_missing", CredentialStore=>"credential_store", ConnectFailed=>"connect_failed", Disconnected=>"disconnected", StatusFailed=>"status_failed" });
codes!(CredentialCode { Saved=>"saved", Session=>"session", Reconnected=>"reconnected", Removed=>"removed", StoreFailed=>"store_failed" });
codes!(ProcessingStage { Cancelling=>"cancelling", Exporting=>"exporting", PostRoll=>"post_roll", Reconnecting=>"reconnecting", ObservedSource=>"observed_source", UncertainSource=>"uncertain_source", SourceOpen=>"source_open", Queued=>"queued", Preparing=>"preparing" });
codes!(RecoveryStage { Idle=>"idle", Scanning=>"scanning", Checking=>"checking", Recovering=>"recovering", Complete=>"complete", Unrecoverable=>"unrecoverable", NeedsAttention=>"needs_attention" });
codes!(RecoveryReason { EmptySource=>"empty_source", Busy=>"busy", MissingSource=>"missing_source", TimingUncertain=>"timing_uncertain", SourceChanged=>"source_changed", ProbeFailed=>"probe_failed", Gapped=>"gapped", Partial=>"partial", ExportFailed=>"export_failed" });
codes!(TimingConfidence { Anchored=>"anchored", Corroborated=>"corroborated", Uncertain=>"uncertain" });
codes!(AutomaticStatus { On=>"on", Paused=>"paused", ObsBlocked=>"obs_blocked", SetupBlocked=>"setup_blocked" });
codes!(CheckStatus { Pass=>"pass", Warn=>"warn", Fail=>"fail", Pending=>"pending" });
codes!(ConnectionState { Connected=>"connected", Unreachable=>"unreachable", PasswordRequired=>"password_required", Available=>"available", Unsupported=>"unsupported", Checking=>"checking" });
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice<C> {
    pub code: C,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}
impl<C> Notice<C> {
    pub const fn new(code: C) -> Self {
        Self {
            code,
            diagnostic: None,
        }
    }
    pub const fn diagnostic(code: C, diagnostic: String) -> Self {
        Self {
            code,
            diagnostic: Some(diagnostic),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_codes_and_unknown_future_values() {
        assert_eq!(
            serde_json::to_string(&RecordingStatus::Unavailable).expect("code"),
            "\"unavailable\""
        );
        assert_eq!(
            serde_json::from_str::<RecoveryStage>("\"future_phase\"").expect("fallback"),
            RecoveryStage::Unknown
        );
    }
}

codes!(MediaStage { Preparing=>"preparing", Reading=>"reading", Copying=>"copying", Encoding=>"encoding", CpuFallback=>"cpu_fallback", Joining=>"joining", Converting=>"converting", Checking=>"checking", Poster=>"poster", Playback=>"playback", Bundle=>"bundle", WritingZip=>"writing_zip" });
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub stage: MediaStage,
    pub completed: Option<u64>,
    pub total: Option<u64>,
    pub processed_seconds: Option<i64>,
    pub speed: Option<f64>,
}
impl Progress {
    pub const fn new(stage: MediaStage) -> Self {
        Self {
            stage,
            completed: None,
            total: None,
            processed_seconds: None,
            speed: None,
        }
    }
}
impl std::fmt::Display for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "stage={} completed={:?} total={:?} processed_seconds={:?} speed={:?}",
            self.stage.as_str(),
            self.completed,
            self.total,
            self.processed_seconds,
            self.speed
        )
    }
}
