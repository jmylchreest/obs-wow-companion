use anyhow::{Context, Result};
use owc_domain::{Activity, Kind, Outcome, Segment, plan_clip};
use owc_media::{CutMode, Media};
use std::{path::PathBuf, time::Instant};
#[tokio::main]
async fn main() -> Result<()> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("benchmark directory required")?,
    );
    let mut media = Media::default();
    if let Some(encoder) = std::env::args().nth(2) {
        media.encoder = serde_json::from_value(serde_json::Value::String(encoder))?;
    }
    let src = root.join("source.mkv");
    let probe = media.probe(&src).await?;
    let mut activity = Activity::new(Kind::Manual, "benchmark".into(), 500, "benchmark");
    activity.finish(probe.duration_ms * 3 - 500, Outcome::Unknown);
    let segments: Vec<_> = (0..3)
        .map(|n| Segment {
            id: uuid::Uuid::new_v4(),
            session: activity.id,
            path: src.clone(),
            start_ms: n * probe.duration_ms,
            end_ms: (n + 1) * probe.duration_ms,
            closed: true,
            managed: true,
        })
        .collect();
    let plan = plan_clip(&activity, &segments, 0, 0);
    let mut results = vec![];
    for (name, mode) in [("precise", CutMode::Precise), ("copy", CutMode::Fast)] {
        let mut plan = plan.clone();
        let start = Instant::now();
        if matches!(mode, CutMode::Fast) {
            media.align_fast_cut(&mut plan).await?;
        }
        let dest = root.join(format!("{name}-{}.mkv", uuid::Uuid::new_v4()));
        let output = media.export(&plan, &dest, mode).await?;
        results.push(serde_json::json!({"mode":name,"elapsed_ms":start.elapsed().as_millis(),"output":output,"file":dest}));
    }
    std::fs::write(
        root.join(format!("result-{}.json", uuid::Uuid::new_v4())),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}
