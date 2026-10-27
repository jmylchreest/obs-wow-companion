use anyhow::{Context, Result};
use base64::Engine as _;
use owc_domain::status::{MediaStage, Progress};
use owc_domain::{Activity, Kind, Outcome, Segment, plan_clip};
use owc_media::{CutMode, Media};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::PathBuf,
};
use tokio::sync::watch;
use uuid::Uuid;

pub struct ShareRequest {
    pub activity: Activity,
    pub tags: Vec<String>,
    pub source: PathBuf,
    pub origin_ms: i64,
    pub range: Option<(i64, i64)>,
    pub tracks: Vec<usize>,
    pub destination: PathBuf,
    pub cancel: watch::Receiver<bool>,
}
fn portable_metadata(
    activity: &Activity,
    tags: &[String],
    origin: i64,
    duration: i64,
    tracks: &[usize],
) -> Value {
    let markers: Vec<_> = activity.markers.iter().filter_map(|marker| {
        let at = marker.at_ms.checked_sub(origin)?;
        (at >= 0 && at <= duration).then(||json!({"at_ms":at,"kind":marker.kind,"label":marker.label,"recent_hits":marker.recent_hits}))
    }).collect();
    json!({"schema_version":1,"format":"owc-share","video":{"file":"video.mp4","poster":"poster.jpg","duration_ms":duration,"source_audio_tracks":tracks.iter().map(|track| track+1).collect::<Vec<_>>()},
        "activity":{"name":activity.name,"kind":activity.kind,"outcome":activity.outcome,"difficulty":activity.difficulty,"key_level":activity.key_level,"keystone_upgrade":activity.keystone_upgrade,"character":activity.character,"start_ms":activity.start_ms-origin,"end_ms":activity.end_ms.map(|end|end-origin)},"markers":markers,"tags":tags})
}
fn viewer_html(metadata: &Value) -> Result<String> {
    let embedded = serde_json::to_string(metadata)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    Ok(include_str!("../share/index.html").replace("__OWC_METADATA__", &embedded))
}
fn validate_shared_layout(
    request: &ShareRequest,
    info: &owc_media::Probe,
    probe: &owc_media::Probe,
) -> Result<()> {
    let expected_duration = if let Some((start, end)) = request.range {
        end.min(info.duration_ms) - start
    } else {
        info.duration_ms
    };
    anyhow::ensure!(
        (probe.duration_ms - expected_duration).abs() <= 250,
        "Shared video duration differs from the selected range"
    );
    anyhow::ensure!(
        probe.audio_tracks == usize::from(!request.tracks.is_empty()),
        "Shared video has an unexpected audio track count"
    );
    Ok(())
}
async fn cancellable<T>(
    cancel: &mut watch::Receiver<bool>,
    work: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    anyhow::ensure!(!*cancel.borrow(), "Export cancelled");
    tokio::select! {
        result = work => result,
        _ = cancel.wait_for(|cancelled| *cancelled) => anyhow::bail!("Export cancelled"),
    }
}
fn validate_destination(dest: &std::path::Path) -> Result<()> {
    anyhow::ensure!(
        dest.is_absolute()
            && dest
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("zip")),
        "Choose an absolute .zip file path"
    );
    anyhow::ensure!(
        !dest.exists(),
        "That ZIP already exists. Choose another file name."
    );
    Ok(())
}
pub async fn export(
    request: ShareRequest,
    mut media: Media,
    progress: watch::Sender<Progress>,
) -> Result<Value> {
    let mut cancel = request.cancel.clone();
    anyhow::ensure!(!*cancel.borrow(), "Export cancelled");
    let dest = &request.destination;
    validate_destination(dest)?;
    let parent = dest.parent().context("Missing destination directory")?;
    tokio::fs::create_dir_all(parent).await?;
    let staging = tempfile::Builder::new()
        .prefix(".owc-share-")
        .tempdir_in(parent)?;
    media.progress = Some(progress.clone());
    let info = cancellable(&mut cancel, media.probe(&request.source)).await?;
    anyhow::ensure!(
        request
            .tracks
            .iter()
            .all(|track| *track < info.audio_tracks),
        "Audio track unavailable"
    );
    let (source, origin) = if let Some((start, end)) = request.range {
        let end = end.min(info.duration_ms);
        anyhow::ensure!(
            start >= 0 && end > start && end <= info.duration_ms,
            "Export range is outside the video"
        );
        let mut clip = Activity::new(Kind::Manual, "Share".into(), start, "share");
        clip.finish(end, Outcome::Unknown);
        let plan = plan_clip(
            &clip,
            &[Segment {
                id: Uuid::new_v4(),
                session: Uuid::new_v4(),
                path: request.source.clone(),
                start_ms: 0,
                end_ms: info.duration_ms,
                closed: true,
                managed: false,
            }],
            0,
            0,
        );
        let cut = staging.path().join("cut.mkv");
        cancellable(&mut cancel, media.export(&plan, &cut, CutMode::Precise)).await?;
        (
            cut,
            request
                .origin_ms
                .checked_add(start)
                .context("Invalid export origin")?,
        )
    } else {
        (request.source.clone(), request.origin_ms)
    };
    let video = staging.path().join("video.mp4");
    cancellable(
        &mut cancel,
        media.preview_mix(&source, &video, &request.tracks),
    )
    .await?;
    let probe = media.probe(&video).await?;
    validate_shared_layout(&request, &info, &probe)?;
    cancellable(&mut cancel, media.verify_decodable(&video)).await?;
    let poster = cancellable(&mut cancel, media.thumbnail(&video, 0)).await?;
    let metadata = portable_metadata(
        &request.activity,
        &request.tags,
        origin,
        probe.duration_ms,
        &request.tracks,
    );
    let html = viewer_html(&metadata)?;
    let metadata = serde_json::to_vec_pretty(&metadata)?;
    let mut details = json!({"path":request.destination,"video":probe,"source_tracks":request.tracks.iter().map(|track| track+1).collect::<Vec<_>>(),"source_track_names":request.tracks.iter().map(|track| info.audio_titles.get(*track).cloned().unwrap_or_default()).collect::<Vec<_>>(),"range":request.range,"source_origin_ms":request.origin_ms});
    let destination = request.destination;
    tokio::task::spawn_blocking(move || {
        let _staging = staging;
        pack(
            &destination,
            &video,
            &html,
            &metadata,
            &progress,
            &cancel,
            &poster,
        )?;
        details["size_bytes"] = std::fs::metadata(&destination)?.len().into();
        details["exported_at_ms"] = chrono::Utc::now().timestamp_millis().into();
        Ok::<_, anyhow::Error>(details)
    })
    .await?
}

fn pack(
    destination: &std::path::Path,
    video: &std::path::Path,
    html: &str,
    metadata: &[u8],
    progress: &watch::Sender<Progress>,
    cancel: &watch::Receiver<bool>,
    poster: &[u8],
) -> Result<()> {
    anyhow::ensure!(!*cancel.borrow(), "Export cancelled");
    let parent = destination
        .parent()
        .context("Missing destination directory")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut zip = zip::ZipWriter::new(temporary.as_file_mut());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let font = base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("../ui/vendor/videojs.woff"));
    let player_css = include_str!("../ui/vendor/video-js.min.css").replace(
        "/vendor/videojs.woff",
        &format!("data:font/woff;base64,{font}"),
    );
    for (name, bytes) in [
        ("index.html", html.as_bytes()),
        ("poster.jpg", poster),
        ("recording.json", metadata),
        (
            "assets/videojs.woff",
            include_bytes!("../ui/vendor/videojs.woff").as_slice(),
        ),
        (
            "COMPANION-LICENSE.txt",
            include_bytes!("../../../LICENSE").as_slice(),
        ),
        (
            "assets/messages.js",
            include_bytes!("../ui/messages.js").as_slice(),
        ),
        (
            "assets/viewer.js",
            include_bytes!("../share/viewer.js").as_slice(),
        ),
        (
            "assets/viewer.css",
            include_bytes!("../share/viewer.css").as_slice(),
        ),
        (
            "assets/video.min.js",
            include_bytes!("../ui/vendor/video.min.js").as_slice(),
        ),
        ("assets/video-js.min.css", player_css.as_bytes()),
        (
            "README.txt",
            include_bytes!("../share/README.txt").as_slice(),
        ),
        (
            "LICENSES.txt",
            include_bytes!("../ui/vendor/VIDEOJS-LICENSE").as_slice(),
        ),
    ] {
        zip.start_file(name, options)?;
        zip.write_all(bytes)?;
    }
    let total = std::fs::metadata(video)?.len();
    zip.start_file("video.mp4", options.large_file(true))?;
    let mut input = std::fs::File::open(video)?;
    let mut buffer = vec![0; 1024 * 1024];
    let mut copied = 0_u64;
    loop {
        anyhow::ensure!(!*cancel.borrow(), "Export cancelled");
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        zip.write_all(&buffer[..count])?;
        copied += u64::try_from(count)?;
        let mut update = Progress::new(MediaStage::WritingZip);
        update.completed = Some(copied);
        update.total = Some(total);
        progress.send_replace(update);
    }
    anyhow::ensure!(!*cancel.borrow(), "Export cancelled");
    zip.finish()?;
    temporary.as_file().sync_all()?;
    anyhow::ensure!(!*cancel.borrow(), "Export cancelled");
    temporary
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelling_in_flight_work_drops_the_worker_future() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        struct Worker(Arc<AtomicBool>);
        impl Drop for Worker {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let (cancel, mut cancelled) = watch::channel(false);
        let (started, ready) = tokio::sync::oneshot::channel();
        let marker = dropped.clone();
        let task = tokio::spawn(async move {
            cancellable(&mut cancelled, async move {
                let _worker = Worker(marker);
                started.send(()).expect("started");
                std::future::pending::<Result<()>>().await
            })
            .await
        });
        ready.await.expect("worker ready");
        cancel.send_replace(true);
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), task)
            .await
            .expect("bounded cancellation")
            .expect("task");
        assert!(
            result
                .expect_err("cancelled")
                .to_string()
                .contains("cancelled")
        );
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[test]
    #[ignore = "requires private 4 MiB tmpfs; run scripts/disk-full-smoke.py"]
    fn disk_full_zip_preserves_source_and_does_not_publish_partial_bundle() {
        let limited =
            PathBuf::from(std::env::var("OWC_LIMITED_FILESYSTEM").expect("private tmpfs"));
        let root = tempfile::tempdir().expect("source root");
        let video = root.path().join("source.mp4");
        let original = vec![42_u8; 2 * 1024 * 1024];
        std::fs::write(&video, &original).expect("source");
        let filler = limited.join("filler");
        std::fs::write(&filler, vec![0_u8; 3 * 1024 * 1024]).expect("fill private filesystem");
        let before = std::fs::read_dir(&limited).expect("directory").count();
        let destination = limited.join("share.zip");
        let (progress, _) = watch::channel(Progress::new(MediaStage::Preparing));
        let (_cancel, cancelled) = watch::channel(false);
        let error = pack(
            &destination,
            &video,
            "viewer",
            b"{}",
            &progress,
            &cancelled,
            b"poster",
        )
        .expect_err("full filesystem must reject ZIP");
        assert!(
            format!("{error:#}").contains("No space left on device"),
            "{error:#}"
        );
        assert!(!destination.exists());
        assert_eq!(
            std::fs::read_dir(&limited).expect("directory").count(),
            before
        );
        assert_eq!(std::fs::read(&video).expect("preserved source"), original);
        std::fs::remove_file(filler).expect("free space");
        pack(
            &destination,
            &video,
            "viewer",
            b"{}",
            &progress,
            &cancelled,
            b"poster",
        )
        .expect("retry");
        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(destination).expect("ZIP")).expect("archive");
        let mut restored = vec![];
        archive
            .by_name("video.mp4")
            .expect("video")
            .read_to_end(&mut restored)
            .expect("read");
        assert_eq!(restored, original);
    }

    #[test]
    fn cancellation_during_zip_writing_removes_partial_output() {
        let root = tempfile::tempdir().expect("root");
        let video = root.path().join("source.mp4");
        let destination = root.path().join("share.zip");
        std::fs::write(&video, vec![42_u8; 2 * 1024 * 1024]).expect("source");
        let (progress, updates) = watch::channel(Progress::new(MediaStage::Preparing));
        let (cancel, cancelled) = watch::channel(false);
        // Hold the progress read lock so the writer cannot pass its first chunk.
        let held_progress = updates.borrow();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                pack(
                    &destination,
                    &video,
                    "viewer",
                    b"{}",
                    &progress,
                    &cancelled,
                    b"poster",
                )
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            // Directory-entry sizes may stay cached while a Windows writer is open.
            // Query a file handle and require the first video chunk, not just headers.
            let started = loop {
                let started = std::fs::read_dir(root.path())
                    .expect("directory")
                    .filter_map(Result::ok)
                    .any(|entry| {
                        entry.path() != video
                            && std::fs::File::open(entry.path())
                                .and_then(|file| file.metadata())
                                .is_ok_and(|metadata| metadata.len() >= 1024 * 1024)
                    });
                if started || std::time::Instant::now() >= deadline {
                    break started;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            cancel.send_replace(true);
            drop(held_progress);
            assert!(worker.join().expect("worker").is_err());
            assert!(started, "ZIP writer never started");
        });
        assert!(!destination.exists());
        assert_eq!(
            std::fs::read_dir(root.path()).expect("directory").count(),
            1
        );
        assert_eq!(
            std::fs::metadata(video).expect("source").len(),
            2 * 1024 * 1024
        );
    }

    #[test]
    fn cancelled_bundle_does_not_publish_and_can_retry() {
        let root = tempfile::tempdir().expect("root");
        let video = root.path().join("source.mp4");
        let destination = root.path().join("share.zip");
        std::fs::write(&video, b"source footage").expect("source");
        let (progress, _) = watch::channel(Progress::new(MediaStage::Preparing));
        let (cancel, cancelled) = watch::channel(true);
        assert!(
            pack(
                &destination,
                &video,
                "viewer",
                b"{}",
                &progress,
                &cancelled,
                b"poster"
            )
            .is_err()
        );
        assert!(!destination.exists());
        assert_eq!(std::fs::read(&video).expect("source"), b"source footage");
        cancel.send_replace(false);
        pack(
            &destination,
            &video,
            "viewer",
            b"{}",
            &progress,
            &cancelled,
            b"poster",
        )
        .expect("retry");
        assert!(destination.exists());
    }

    #[tokio::test]
    async fn real_bundle_validates_media_and_rejects_damaged_video() {
        let root = tempfile::tempdir().expect("root");
        let source = root.path().join("source.mp4");
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x90:rate=30",
                "-t",
                "1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(&source)
            .output()
            .expect("ffmpeg");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let media = Media::default();
        let (_cancel, cancelled) = watch::channel(false);
        let make_request = |destination| ShareRequest {
            activity: Activity::new(Kind::Manual, "Review".into(), 0, "test"),
            tags: vec![],
            source: source.clone(),
            origin_ms: 0,
            range: None,
            tracks: vec![],
            destination,
            cancel: cancelled.clone(),
        };
        let (progress, _) = watch::channel(Progress::new(MediaStage::Preparing));
        let destination = root.path().join("good.zip");
        export(
            make_request(destination.clone()),
            media.clone(),
            progress.clone(),
        )
        .await
        .expect("valid export");
        assert!(destination.is_file());
        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(&destination).expect("ZIP")).expect("archive");
        let mut poster = vec![];
        archive
            .by_name("poster.jpg")
            .expect("poster")
            .read_to_end(&mut poster)
            .expect("image");
        assert!(poster.starts_with(&[0xff, 0xd8]) && poster.ends_with(&[0xff, 0xd9]));
        let mut bytes = std::fs::read(&source).expect("video");
        let mdat = bytes.windows(4).position(|b| b == b"mdat").expect("mdat");
        bytes[mdat + 4..].fill(0);
        std::fs::write(&source, bytes).expect("damage payload");
        // Container metadata remains readable; probing alone cannot certify playback.
        assert!(media.probe(&source).await.is_ok());
        assert!(media.verify_decodable(&source).await.is_err());
        let destination = root.path().join("damaged.zip");
        assert!(
            export(make_request(destination.clone()), media, progress)
                .await
                .is_err()
        );
        assert!(!destination.exists());
        assert!(
            std::fs::read_dir(root.path())
                .expect("directory")
                .all(|entry| !entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".owc-share-"))
        );
    }

    #[test]
    fn bundle_contains_local_assets_and_never_overwrites_an_existing_file() {
        let root = tempfile::tempdir().expect("root");
        let video = root.path().join("input.mp4");
        std::fs::write(&video, b"fixture video bytes").expect("video");
        let destination = root.path().join("share.zip");
        let (progress, _) = watch::channel(Progress::new(MediaStage::Preparing));
        pack(
            &destination,
            &video,
            "<html>fixture</html>",
            b"{}",
            &progress,
            &watch::channel(false).1,
            b"poster",
        )
        .expect("pack");
        let before = std::fs::read(&destination).expect("zip");
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&destination).expect("open"))
            .expect("archive");
        for name in [
            "index.html",
            "recording.json",
            "video.mp4",
            "assets/viewer.js",
            "assets/video.min.js",
            "LICENSES.txt",
        ] {
            assert!(archive.by_name(name).is_ok(), "missing {name}");
        }
        let mut bytes = Vec::new();
        archive
            .by_name("video.mp4")
            .expect("video")
            .read_to_end(&mut bytes)
            .expect("read");
        assert_eq!(bytes, b"fixture video bytes");
        assert!(
            pack(
                &destination,
                &video,
                "replacement",
                b"{}",
                &progress,
                &watch::channel(false).1,
                b"poster",
            )
            .is_err()
        );
        assert_eq!(std::fs::read(destination).expect("zip"), before);
    }

    #[test]
    fn portable_metadata_removes_machine_details_and_aligns_markers() {
        let mut activity = owc_domain::Activity::new(
            owc_domain::Kind::Raid,
            "</script><script>bad()</script>".into(),
            1000,
            "/private/install",
        );
        activity.finish(5000, owc_domain::Outcome::Success);
        activity.marker(2500, "death", "Player".into());
        activity.marker(4500, "manual", "After cut".into());
        let data = portable_metadata(&activity, &["review".to_owned()], 2000, 2000, &[0, 1]);
        assert_eq!(data["markers"][0]["at_ms"], 500);
        assert_eq!(data["markers"].as_array().expect("markers").len(), 1);
        assert!(!data.to_string().contains("/private/install"));
        let html = viewer_html(&data).expect("html");
        assert!(!html.contains("</script><script>bad()"));
        assert!(html.contains("\\u003c/script"));
    }
}
