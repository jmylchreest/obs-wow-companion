use owc_domain::{Kind, Outcome};
use owc_wow_log::{Detector, Flavour, parse};

#[test]
fn bounded_tailer_restarts_mid_backlog_without_loss() {
    use owc_wow_log::Tailer;
    let directory = tempfile::tempdir().expect("directory");
    let mut tailer = Tailer::default();
    assert!(
        tailer
            .poll(directory.path())
            .expect("initialize")
            .is_empty()
    );
    let line = format!("{}\n", "x".repeat(255));
    std::fs::write(
        directory.path().join("WoWCombatLog-fixture.txt"),
        line.repeat(10_000),
    )
    .expect("backlog");
    let first = tailer.poll(directory.path()).expect("bounded poll");
    assert!(first.len() < 10_000);
    let mut count = first.len();
    let mut tailer: Tailer =
        serde_json::from_value(serde_json::to_value(tailer).expect("checkpoint")).expect("restart");
    for _ in 0..10 {
        let lines = tailer.poll(directory.path()).expect("poll");
        if lines.is_empty() {
            break;
        }
        count += lines.len();
    }
    assert_eq!(count, 10_000);
}

#[test]
fn retail_fixture_preserves_character_markers_and_whole_run() {
    let mut detector = Detector::new("Retail".into(), Flavour::Retail);
    let activities: Vec<_> = include_str!("fixtures/retail.log")
        .lines()
        .flat_map(|line| detector.ingest(&parse(line, 0).expect("fixture timestamp")))
        .collect();
    assert_eq!(activities.len(), 3);
    assert_eq!(activities[0].outcome, Outcome::Failure);
    assert_eq!(activities[0].character.as_deref(), Some("Éowyn-Realm"));
    assert_eq!(activities[0].markers[0].kind, "death");
    assert_eq!(activities[1].outcome, Outcome::Success);
    assert_eq!(activities[2].kind, Kind::MythicPlus);
    assert_eq!(
        activities[2].end_ms.expect("end") - activities[2].start_ms,
        45 * 60 * 1000
    );
    assert_eq!(activities[2].markers.len(), 2);
}

#[test]
fn classic_fixture_uses_explicit_unknown_zone_outcomes() {
    for flavour in [Flavour::Classic, Flavour::Era] {
        let mut detector = Detector::new("Classic".into(), flavour);
        let activities: Vec<_> = include_str!("fixtures/classic.log")
            .lines()
            .flat_map(|line| detector.ingest(&parse(line, 0).expect("fixture timestamp")))
            .collect();
        assert_eq!(activities.len(), 3);
        assert_eq!(activities[0].kind, Kind::Raid);
        assert_eq!(activities[1].kind, Kind::Arena);
        assert_eq!(activities[2].kind, Kind::Battleground);
        assert_eq!(activities[2].outcome, Outcome::Unknown);
    }
}
