use owc_media::Media;
use std::{env, path::PathBuf};

// The Python driver isolates PATH per process rather than mutating the test runner's environment.
#[tokio::test]
#[ignore = "run through scripts/media-tools-smoke.py"]
async fn configured_tools() {
    let root = PathBuf::from(env::var_os("OWC_MEDIA_FIXTURE").expect("fixture directory"));
    let source = root.join("source.mkv");
    let original = std::fs::read(&source).expect("source");
    let media = Media {
        ffmpeg: env::var_os("OWC_TEST_FFMPEG").expect("ffmpeg").into(),
        ffprobe: env::var_os("OWC_TEST_FFPROBE").expect("ffprobe").into(),
        ..Media::default()
    };
    match env::var("OWC_MEDIA_EXPECT").expect("expectation").as_str() {
        "missing_probe" => assert!(media.probe(&source).await.is_err()),
        "missing_encoder" => {
            assert_eq!(media.probe(&source).await.expect("probe").audio_tracks, 2);
            assert!(
                media
                    .preview_mix(&source, &root.join("missing.mp4"), &[0, 1])
                    .await
                    .is_err()
            );
            assert!(!root.join("missing.mp4").exists());
        }
        "ready" => {
            let probe = media.probe(&source).await.expect("probe");
            assert_eq!(probe.audio_tracks, 2);
            let mixed = root.join("mixed output – café.mp4");
            media
                .preview_mix(&source, &mixed, &[0, 1])
                .await
                .expect("mix");
            assert_eq!(
                media.probe(&mixed).await.expect("mixed probe").audio_tracks,
                1
            );
            media.verify_decodable(&mixed).await.expect("decode");
            assert!(
                !media
                    .thumbnail(&mixed, 500)
                    .await
                    .expect("thumbnail")
                    .is_empty()
            );
            std::fs::remove_file(mixed).expect("processes release their output files");
        }
        other => panic!("unknown expectation {other}"),
    }
    assert_eq!(std::fs::read(source).expect("source preserved"), original);
}
