//! Pure encounter and media timeline models shared by all adapters.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

pub mod status;

pub type Millis = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Raid,
    MythicPlus,
    Delve,
    Arena,
    SoloShuffle,
    Battleground,
    Manual,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failure,
    Abandoned,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeathHit {
    pub offset_ms: Millis,
    pub source: String,
    pub ability: String,
    pub spell_id: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub at_ms: Millis,
    pub label: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_hits: Vec<DeathHit>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub id: Uuid,
    pub installation: String,
    #[serde(default)]
    pub installation_id: Option<Uuid>,
    pub kind: Kind,
    pub name: String,
    pub start_ms: Millis,
    pub end_ms: Option<Millis>,
    pub outcome: Outcome,
    pub difficulty: Option<u32>,
    pub key_level: Option<u32>,
    #[serde(default)]
    pub challenge_mode_id: Option<u32>,
    #[serde(default)]
    pub keystone_time_limit_ms: Option<Millis>,
    #[serde(default)]
    pub keystone_elapsed_ms: Option<Millis>,
    #[serde(default)]
    pub keystone_upgrade: Option<u8>,
    pub zone: Option<u32>,
    pub encounter: Option<u32>,
    pub character: Option<String>,
    pub markers: Vec<Marker>,
    pub warnings: Vec<String>,
}
impl Activity {
    pub fn new(kind: Kind, name: String, start_ms: Millis, installation: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            installation: installation.into(),
            installation_id: None,
            kind,
            name,
            start_ms,
            end_ms: None,
            outcome: Outcome::Unknown,
            difficulty: None,
            key_level: None,
            challenge_mode_id: None,
            keystone_time_limit_ms: None,
            keystone_elapsed_ms: None,
            keystone_upgrade: None,
            zone: None,
            encounter: None,
            character: None,
            markers: vec![],
            warnings: vec![],
        }
    }
    pub fn finish(&mut self, at: Millis, outcome: Outcome) {
        self.end_ms = Some(at.max(self.start_ms));
        self.outcome = outcome;
    }
    pub fn marker(&mut self, at_ms: Millis, kind: &str, label: String) {
        self.markers.push(Marker {
            at_ms,
            recent_hits: vec![],
            kind: kind.into(),
            label,
        });
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: Uuid,
    pub session: Uuid,
    pub path: PathBuf,
    pub start_ms: Millis,
    pub end_ms: Millis,
    pub closed: bool,
    pub managed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Complete,
    Truncated,
    Gapped,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentRange {
    pub segment: Segment,
    pub offset_ms: Millis,
    pub duration_ms: Millis,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipPlan {
    pub activity_id: Uuid,
    pub requested_start_ms: Millis,
    pub requested_end_ms: Millis,
    pub actual_start_ms: Option<Millis>,
    pub coverage: Coverage,
    pub sources: Vec<SegmentRange>,
}
/// Plans only closed media. Gaps are explicit and must never be exported as seamless footage.
pub fn plan_clip(
    activity: &Activity,
    segments: &[Segment],
    pre_ms: Millis,
    post_ms: Millis,
) -> ClipPlan {
    let start = activity.start_ms.saturating_sub(pre_ms.max(0));
    let end = activity
        .end_ms
        .unwrap_or(activity.start_ms)
        .saturating_add(post_ms.max(0));
    let mut candidates: Vec<_> = segments
        .iter()
        .filter(|s| s.closed && s.end_ms > start && s.start_ms < end)
        .collect();
    candidates.sort_by_key(|s| s.start_ms);
    let mut sources = vec![];
    let mut cursor = start;
    let mut first = None;
    let mut gap = false;
    for s in candidates {
        let from = cursor.max(s.start_ms).max(start);
        let to = end.min(s.end_ms);
        if to <= from {
            continue;
        }
        if first.is_some() && from > cursor.saturating_add(100) {
            gap = true;
        }
        first.get_or_insert(from);
        sources.push(SegmentRange {
            segment: s.clone(),
            offset_ms: from - s.start_ms,
            duration_ms: to - from,
        });
        cursor = to;
    }
    let coverage = match first {
        None => Coverage::Unavailable,
        Some(_) if gap => Coverage::Gapped,
        Some(f) if f > start.saturating_add(100) || cursor < end.saturating_sub(100) => {
            Coverage::Truncated
        }
        Some(_) => Coverage::Complete,
    };
    ClipPlan {
        activity_id: activity.id,
        requested_start_ms: start,
        requested_end_ms: end,
        actual_start_ms: first,
        coverage,
        sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn contiguous_segments_conserve_requested_duration(start in 0_i64..100_000, duration in 1_i64..100_000, width in 1000_i64..30_000) {
            let end = start + duration;
            let mut activity = Activity::new(Kind::Raid, "fixture".into(), start, "retail");
            activity.finish(end, Outcome::Success);
            let mut segments: Vec<_> = (0..=(end / width)).map(|n| segment(n * width, (n + 1) * width)).collect();
            segments.reverse();
            let plan = plan_clip(&activity, &segments, 0, 0);
            prop_assert_eq!(plan.coverage, Coverage::Complete);
            prop_assert_eq!(plan.actual_start_ms, Some(start));
            prop_assert_eq!(plan.sources.iter().map(|r|r.duration_ms).sum::<i64>(), duration);
            for range in plan.sources {
                prop_assert!(range.offset_ms >= 0);
                prop_assert!(range.duration_ms > 0);
                prop_assert!(range.offset_ms + range.duration_ms <= range.segment.end_ms - range.segment.start_ms);
            }
        }
    }
    fn segment(start: i64, end: i64) -> Segment {
        Segment {
            id: Uuid::new_v4(),
            session: Uuid::nil(),
            path: "test.mkv".into(),
            start_ms: start,
            end_ms: end,
            closed: true,
            managed: true,
        }
    }
    #[test]
    fn delayed_start_spans_segments_with_padding() {
        let mut a = Activity::new(Kind::Raid, "Boss".into(), 30_000, "retail");
        a.finish(90_000, Outcome::Success);
        let p = plan_clip(
            &a,
            &[segment(0, 60_000), segment(60_000, 120_000)],
            5_000,
            10_000,
        );
        assert_eq!(p.coverage, Coverage::Complete);
        assert_eq!(p.sources.len(), 2);
        assert_eq!(p.sources[0].offset_ms, 25_000);
        assert_eq!(p.sources[1].duration_ms, 40_000);
    }
    #[test]
    fn missing_and_open_media_are_never_complete() {
        let mut a = Activity::new(Kind::Raid, "Boss".into(), 0, "r");
        a.finish(120_000, Outcome::Unknown);
        assert_eq!(
            plan_clip(&a, &[segment(30_000, 120_000)], 0, 0).coverage,
            Coverage::Truncated
        );
        assert_eq!(
            plan_clip(&a, &[segment(0, 40_000), segment(60_000, 120_000)], 0, 0).coverage,
            Coverage::Gapped
        );
        let mut s = segment(0, 120_000);
        s.closed = false;
        assert_eq!(plan_clip(&a, &[s], 0, 0).coverage, Coverage::Unavailable);
    }
}
