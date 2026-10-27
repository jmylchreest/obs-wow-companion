//! FFmpeg/ffprobe adapter. Source files are read-only; publication follows validation.
mod encoding;
use anyhow::{Context, Result, bail};
pub use encoding::{Encoder, ExportInfo};
use owc_domain::status::{MediaStage, Progress};
use owc_domain::{ClipPlan, Coverage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write as _;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{fs, process::Command, time::timeout};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    #[serde(skip)]
    pub progress: Option<tokio::sync::watch::Sender<Progress>>,
    #[serde(skip)]
    pub cancel: Option<tokio::sync::watch::Receiver<bool>>,
    #[serde(default)]
    pub encoder: Encoder,
    #[serde(default = "default_vaapi_device")]
    pub vaapi_device: PathBuf,
}
impl Default for Media {
    fn default() -> Self {
        Self {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            progress: None,
            cancel: None,
            encoder: Encoder::default(),
            vaapi_device: default_vaapi_device(),
        }
    }
}
fn audio_title(stream: &Value) -> String {
    let tags = stream["tags"].as_object();
    let tag = |name: &str| {
        tags.and_then(|tags| tags.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)))
            .and_then(|(_, value)| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    tag("title")
        .or_else(|| tag("name"))
        .or_else(|| {
            tag("handler_name").filter(|name| {
                !["SoundHandler", "AudioHandler", "OBS Audio Handler"]
                    .iter()
                    .any(|generic| name.eq_ignore_ascii_case(generic))
            })
        })
        .unwrap_or_default()
        .to_owned()
}

fn default_vaapi_device() -> PathBuf {
    "/dev/dri/renderD128".into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<ExportInfo>,
    #[serde(skip)]
    stream_layout: Vec<Value>,
    #[serde(default)]
    pub creation_time: Option<String>,
    pub duration_ms: i64,
    pub audio_tracks: usize,
    pub video_codec: String,
    #[serde(default)]
    pub pixel_format: String,
    #[serde(default)]
    pub container: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub frame_rate: String,
    #[serde(default)]
    pub audio_codecs: Vec<String>,
    #[serde(default)]
    pub audio_titles: Vec<String>,
    pub width: u64,
    pub height: u64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CutMode {
    #[default]
    Fast,
    Precise,
}
pub struct ReviewCopy {
    pub probe: Probe,
    pub origin_ms: i64,
    pub converted: bool,
}
fn concat_list(plan: &ClipPlan) -> Result<String> {
    let mut list = String::from("ffconcat version 1.0\n");
    for range in &plan.sources {
        let path = range.segment.path.canonicalize()?;
        let path = path.to_str().context("Source path is not UTF-8")?;
        // ffconcat has its own quoting rules; paths never enter a shell.
        let path = if cfg!(windows) {
            path.strip_prefix(r"\\?\UNC\").map_or_else(
                || path.trim_start_matches(r"\\?\").replace('\\', "/"),
                |unc| format!("//{}", unc.replace('\\', "/")),
            )
        } else {
            path.to_owned()
        };
        anyhow::ensure!(
            !path.contains(['\n', '\r']),
            "Source path contains a line break"
        );
        let path = path.replace('\'', "'\\''");
        writeln!(list, "file '{path}'")?;
        writeln!(
            list,
            "duration {}",
            seconds(range.segment.end_ms - range.segment.start_ms)
        )?;
    }
    Ok(list)
}
async fn checked(command: &mut Command) -> Result<Vec<u8>> {
    command.kill_on_drop(true).stdin(Stdio::null());
    let out = timeout(Duration::from_hours(1), command.output())
        .await
        .context("media process exceeded one-hour limit")??;
    if !out.status.success() {
        bail!(
            "media process failed: {}",
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(4096)
                .collect::<String>()
        );
    }
    Ok(out.stdout)
}
fn seconds(ms: i64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}
fn direct_copy_compatible(plan: &ClipPlan, inputs: &[Probe]) -> Result<bool> {
    anyhow::ensure!(
        inputs
            .iter()
            .all(|input| input.stream_layout == inputs[0].stream_layout),
        "Source codecs or stream layout changed; lossless concatenation is unavailable"
    );
    Ok(plan.sources.iter().enumerate().all(|(i, range)| {
        (i == 0 || range.offset_ms == 0)
            && (i + 1 == plan.sources.len()
                || range.offset_ms + range.duration_ms
                    == range.segment.end_ms - range.segment.start_ms)
    }))
}
async fn run_ffmpeg_progress(
    command: &mut Command,
    progress: Option<tokio::sync::watch::Sender<Progress>>,
) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let Some(progress) = progress else {
        checked(command).await?;
        return Ok(());
    };
    let stage = progress.borrow().clone();
    command
        .args(["-progress", "pipe:1", "-nostats", "-stats_period", "1"])
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("start FFmpeg")?;
    let stdout = child.stdout.take().context("FFmpeg progress pipe")?;
    let read_progress = async {
        let mut lines = BufReader::new(stdout).lines();
        let mut processed = 0_i64;
        let mut speed = String::new();
        while let Some(line) = lines.next_line().await? {
            if let Some(value) = line.strip_prefix("out_time_us=") {
                processed = value.parse::<i64>().unwrap_or(0).max(0) / 1_000_000;
            }
            if let Some(value) = line.strip_prefix("speed=") {
                speed = value.chars().take(24).collect();
            }
            if line.starts_with("progress=") {
                let mut update = stage.clone();
                update.processed_seconds = Some(processed);
                update.speed = speed
                    .trim()
                    .trim_end_matches('x')
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite());
                progress.send_replace(update);
            }
        }
        Ok::<_, std::io::Error>(())
    };
    let ((), output) = timeout(Duration::from_hours(1), async {
        tokio::try_join!(read_progress, child.wait_with_output())
    })
    .await
    .context("media process exceeded one-hour limit")??;
    if !output.status.success() {
        bail!(
            "media process failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(4096)
                .collect::<String>()
        );
    }
    Ok(())
}

async fn run_cancellable_ffmpeg(
    command: &mut Command,
    progress: Option<tokio::sync::watch::Sender<Progress>>,
    cancel: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<()> {
    if let Some(mut cancel) = cancel {
        anyhow::ensure!(!*cancel.borrow(), "Export cancelled. Retry when ready.");
        tokio::select! {
            result = run_ffmpeg_progress(command, progress) => result,
            _ = cancel.wait_for(|cancelled| *cancelled) => bail!("Export cancelled. Retry when ready."),
        }
    } else {
        run_ffmpeg_progress(command, progress).await
    }
}

impl Media {
    async fn run_ffmpeg(&self, command: &mut Command) -> Result<()> {
        run_cancellable_ffmpeg(command, self.progress.clone(), self.cancel.clone()).await
    }

    fn report(&self, stage: MediaStage) {
        if let Some(progress) = &self.progress {
            progress.send_replace(Progress::new(stage));
        }
    }

    fn report_fragment(&self, mode: CutMode, index: usize, count: usize) {
        if let Some(progress) = &self.progress {
            let mut update = Progress::new(if matches!(mode, CutMode::Fast) {
                MediaStage::Copying
            } else {
                MediaStage::Encoding
            });
            update.completed = u64::try_from(index + 1).ok();
            update.total = u64::try_from(count).ok();
            progress.send_replace(update);
        }
    }
    /// Expands a fast cut to the preceding keyframe and records its real timeline origin.
    pub async fn align_fast_cut(&self, plan: &mut ClipPlan) -> Result<()> {
        let Some(first) = plan.sources.first_mut() else {
            return Ok(());
        };
        if first.offset_ms == 0 {
            return Ok(());
        }
        let output = checked(
            Command::new(&self.ffprobe)
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-skip_frame",
                    "nokey",
                    "-show_frames",
                    "-show_entries",
                    "frame=best_effort_timestamp_time",
                    "-of",
                    "json",
                ])
                .arg("-read_intervals")
                .arg(format!(
                    "{}%{}",
                    seconds(first.offset_ms.saturating_sub(30_000).max(0)),
                    seconds(first.offset_ms + 1000)
                ))
                .arg(&first.segment.path),
        )
        .await?;
        let frames: Value = serde_json::from_slice(&output)?;
        let mut keyframe: Option<i64> = None;
        for frame in frames["frames"]
            .as_array()
            .context("keyframe list unavailable")?
        {
            if let Some(value) = frame["best_effort_timestamp_time"].as_str() {
                let seconds: f64 = value.parse()?;
                if seconds.is_finite() && seconds >= 0.0 {
                    let millis: i64 = format!("{:.0}", seconds * 1000.0).parse()?;
                    if millis <= first.offset_ms {
                        keyframe = Some(keyframe.map_or(millis, |old| old.max(millis)));
                    }
                }
            }
        }
        let keyframe =
            keyframe.context("No preceding keyframe found; precise cuts are required")?;
        first.duration_ms += first.offset_ms - keyframe;
        first.offset_ms = keyframe;
        plan.actual_start_ms = Some(first.segment.start_ms + keyframe);
        Ok(())
    }
    pub async fn probe(&self, path: &Path) -> Result<Probe> {
        let data = checked(
            Command::new(&self.ffprobe)
                .args([
                    "-v",
                    "error",
                    "-show_format",
                    "-show_streams",
                    "-show_data_hash",
                    "sha256",
                    "-of",
                    "json",
                ])
                .arg(path),
        )
        .await?;
        let v: Value = serde_json::from_slice(&data)?;
        let duration = v["format"]["duration"]
            .as_str()
            .context("missing duration")?
            .parse::<f64>()?;
        if !duration.is_finite() || duration <= 0.0 || duration > 86_400.0 * 30.0 {
            bail!("invalid media duration");
        }
        let streams = v["streams"].as_array().context("missing streams")?;
        let video = streams
            .iter()
            .find(|s| s["codec_type"] == "video")
            .context("no video stream")?;
        Ok(Probe {
            creation_time: v["format"]["tags"]["creation_time"]
                .as_str()
                .map(str::to_owned),
            export: None,
            stream_layout: streams
                .iter()
                .filter(|stream| matches!(stream["codec_type"].as_str(), Some("video" | "audio")))
                .map(|stream| {
                    let keys = [
                        "codec_type",
                        "codec_name",
                        "profile",
                        "level",
                        "width",
                        "height",
                        "pix_fmt",
                        "time_base",
                        "sample_rate",
                        "channels",
                        "channel_layout",
                        "extradata_hash",
                    ];
                    Value::Object(
                        keys.into_iter()
                            .map(|key| (key.into(), stream[key].clone()))
                            .collect(),
                    )
                })
                .collect(),
            duration_ms: format!("{:.0}", duration * 1000.0).parse()?,
            audio_tracks: streams
                .iter()
                .filter(|s| s["codec_type"] == "audio")
                .count(),
            video_codec: video["codec_name"].as_str().unwrap_or("unknown").into(),
            pixel_format: video["pix_fmt"].as_str().unwrap_or_default().into(),
            container: v["format"]["format_name"]
                .as_str()
                .unwrap_or_default()
                .into(),
            size_bytes: v["format"]["size"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            frame_rate: video["avg_frame_rate"].as_str().unwrap_or_default().into(),
            audio_codecs: streams
                .iter()
                .filter(|s| s["codec_type"] == "audio")
                .map(|s| s["codec_name"].as_str().unwrap_or("unknown").to_owned())
                .collect(),
            audio_titles: streams
                .iter()
                .filter(|s| s["codec_type"] == "audio")
                .map(audio_title)
                .collect(),
            width: video["width"].as_u64().unwrap_or(0),
            height: video["height"].as_u64().unwrap_or(0),
        })
    }
    pub async fn export(&self, plan: &ClipPlan, dest: &Path, mode: CutMode) -> Result<Probe> {
        self.report(MediaStage::Reading);
        if matches!(plan.coverage, Coverage::Unavailable | Coverage::Gapped)
            || plan.sources.is_empty()
        {
            bail!("no continuous footage available for export");
        }
        if dest.exists() {
            let source = self.probe(&plan.sources[0].segment.path).await?;
            return self
                .validate_export(plan, dest, mode, source.audio_tracks)
                .await;
        }
        let parent = dest.parent().context("destination requires parent")?;
        fs::create_dir_all(parent).await?;
        let job_staging = parent.join(format!(".{}-export", plan.activity_id));
        // A killed parent can leave FFmpeg running. A retry must never share its output paths.
        let staging = job_staging.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&staging).await?;
        let result = self.export_staged(plan, dest, mode, &staging).await;
        let result = match result {
            Ok(mut probe) => {
                probe.export = Some(ExportInfo {
                    encoder: if matches!(mode, CutMode::Fast) {
                        "copy".into()
                    } else {
                        self.encoder.name().into()
                    },
                    fallback: None,
                });
                Ok(probe)
            }
            Err(error) if self.cancel.as_ref().is_some_and(|cancel| *cancel.borrow()) => Err(error),
            Err(error) if self.encoder != Encoder::Cpu || matches!(mode, CutMode::Fast) => {
                self.report(MediaStage::CpuFallback);
                let mut software = self.clone();
                software.encoder = Encoder::Cpu;
                software
                    .export_staged(plan, dest, CutMode::Precise, &staging)
                    .await
                    .with_context(|| {
                        format!("Preferred export failed ({error:#}); CPU fallback also failed")
                    })
                    .map(|mut probe| {
                        probe.export = Some(ExportInfo {
                            encoder: "libx264".into(),
                            fallback: Some(format!("Preferred export failed; used CPU: {error:#}")),
                        });
                        probe
                    })
            }
            Err(error) => Err(error),
        };
        // Failed jobs must not retain potentially hours of temporary PCM/video on disk.
        let _ = fs::remove_dir_all(&job_staging).await;
        result
    }
    async fn export_staged(
        &self,
        plan: &ClipPlan,
        dest: &Path,
        mode: CutMode,
        staging: &Path,
    ) -> Result<Probe> {
        let mut inputs = Vec::with_capacity(plan.sources.len());
        for range in &plan.sources {
            inputs.push(self.probe(&range.segment.path).await?);
        }
        if matches!(mode, CutMode::Fast) && direct_copy_compatible(plan, &inputs)? {
            return self
                .export_concat_copy(plan, dest, staging, inputs[0].audio_tracks)
                .await;
        }
        let mut expected_audio = None;
        let mut list = String::new();
        for (i, (range, input)) in plan.sources.iter().zip(&inputs).enumerate() {
            self.report_fragment(mode, i, plan.sources.len());
            if expected_audio.is_some_and(|n| n != input.audio_tracks) {
                bail!("audio layout changed between source segments");
            }
            expected_audio = Some(input.audio_tracks);
            let part = staging.join(format!("part-{i:04}.mkv"));
            let mut cmd = Command::new(&self.ffmpeg);
            cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"]);
            if matches!(mode, CutMode::Fast) {
                cmd.args(["-ss", &seconds(range.offset_ms)]);
            }
            if matches!(mode, CutMode::Precise) && self.encoder == Encoder::Vaapi {
                cmd.arg("-vaapi_device").arg(&self.vaapi_device);
            }
            cmd.arg("-i").arg(&range.segment.path);
            if matches!(mode, CutMode::Precise) {
                cmd.args(["-ss", &seconds(range.offset_ms)]);
            }
            cmd.args([
                "-t",
                &seconds(range.duration_ms),
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
            ]);
            match mode {
                CutMode::Fast => {
                    cmd.args(["-c", "copy", "-avoid_negative_ts", "make_zero"]);
                }
                CutMode::Precise => {
                    // Retain the demuxer's timestamp precision. Frame-rate time bases
                    // can round an initial A/V offset up and drop the last cut frame.
                    cmd.args(["-enc_time_base:v", "demux"]);
                    self.encoder.configure(&mut cmd);
                    cmd.args(["-c:a", "pcm_s16le"]);
                }
            }
            cmd.arg(&part);
            self.run_ffmpeg(&mut cmd).await?;
            writeln!(list, "file 'part-{i:04}.mkv'")?;
            if matches!(mode, CutMode::Precise) {
                // Keep each boundary on the requested timeline, independent of frame rounding.
                writeln!(list, "duration {}", seconds(range.duration_ms))?;
            }
        }
        let listpath = staging.join("parts.txt");
        fs::write(&listpath, list).await?;
        let pending = staging.join("output.mkv");
        self.report(MediaStage::Joining);
        let mut concat = Command::new(&self.ffmpeg);
        concat
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "concat",
                "-safe",
                "1",
                "-i",
            ])
            .arg(&listpath)
            .args(["-map", "0:v:0", "-map", "0:a?", "-c:v", "copy"]);
        // Encode audio once: per-part AAC priming otherwise accumulates at every split.
        match mode {
            CutMode::Precise => {
                concat.args(["-c:a", "aac", "-b:a", "192k"]);
            }
            CutMode::Fast => {
                concat.args(["-c:a", "copy"]);
            }
        }
        concat.arg(&pending);
        self.run_ffmpeg(&mut concat).await?;
        let probe = self
            .validate_export(
                plan,
                &pending,
                mode,
                expected_audio.context("missing audio layout")?,
            )
            .await?;
        fs::rename(&pending, dest)
            .await
            .context("publish validated media")?;
        Ok(probe)
    }
    /// Prepares continuous review without waiting for the final export's full decode check.
    pub async fn preview_plan(
        &self,
        plan: &ClipPlan,
        dest: &Path,
        track: usize,
    ) -> Result<ReviewCopy> {
        anyhow::ensure!(
            !plan.sources.is_empty()
                && !matches!(plan.coverage, Coverage::Gapped | Coverage::Unavailable),
            "No continuous retained footage"
        );
        anyhow::ensure!(
            plan.sources.iter().all(|r| r.segment.closed),
            "Source recording is still open"
        );
        let staging = dest
            .parent()
            .context("Preview directory missing")?
            .join(format!(".review-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&staging).await?;
        let result = self.preview_plan_staged(plan, dest, track, &staging).await;
        let _ = fs::remove_dir_all(staging).await;
        result
    }
    async fn preview_plan_staged(
        &self,
        plan: &ClipPlan,
        dest: &Path,
        track: usize,
        staging: &Path,
    ) -> Result<ReviewCopy> {
        let mut aligned = plan.clone();
        self.align_fast_cut(&mut aligned).await?;
        let mut inputs = Vec::new();
        for range in &aligned.sources {
            let probe = self.probe(&range.segment.path).await?;
            anyhow::ensure!(
                track == 0 && probe.audio_tracks == 0 || track < probe.audio_tracks,
                "Audio track unavailable"
            );
            inputs.push(probe);
        }
        if !direct_copy_compatible(&aligned, &inputs).unwrap_or(false) {
            let source = staging.join("compatible.mkv");
            self.export(plan, &source, CutMode::Precise).await?;
            self.preview(&source, dest, track).await?;
            return Ok(ReviewCopy {
                probe: self.probe(dest).await?,
                origin_ms: plan.actual_start_ms.context("Missing source origin")?,
                converted: true,
            });
        }
        let first = &inputs[0];
        let converted = first.video_codec != "h264" || first.pixel_format != "yuv420p";
        let list = staging.join("sources.ffconcat");
        fs::write(&list, concat_list(&aligned)?).await?;
        let pending = staging.join("playback.mp4");
        let duration: i64 = aligned.sources.iter().map(|r| r.duration_ms).sum();
        let mut command = self.concat_command(&list, aligned.sources[0].offset_ms);
        command.args(["-t", &seconds(duration), "-map", "0:v:0"]);
        if first.audio_tracks > 0 {
            command.args([
                "-map",
                &format!("0:a:{track}"),
                "-c:a",
                if first.audio_codecs.get(track).is_some_and(|c| c == "aac") {
                    "copy"
                } else {
                    "aac"
                },
            ]);
        }
        if converted {
            command.args([
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "22", "-pix_fmt", "yuv420p",
            ]);
        } else {
            command.args(["-c:v", "copy"]);
        }
        command
            .args(["-avoid_negative_ts", "make_zero", "-movflags", "+faststart"])
            .arg(&pending);
        self.report(if converted {
            MediaStage::Converting
        } else {
            MediaStage::Joining
        });
        self.run_ffmpeg(&mut command).await?;
        let probe = self.probe(&pending).await?;
        anyhow::ensure!(
            (probe.duration_ms - duration).abs() <= 500 * i64::try_from(aligned.sources.len())?
                && probe.audio_tracks == usize::from(first.audio_tracks > 0),
            "Playback copy has unexpected duration or audio layout"
        );
        fs::rename(pending, dest).await?;
        Ok(ReviewCopy {
            probe,
            origin_ms: aligned.actual_start_ms.context("Missing source origin")?,
            converted,
        })
    }
    fn concat_command(&self, list: &Path, offset_ms: i64) -> Command {
        let mut command = Command::new(&self.ffmpeg);
        command.args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
        ]);
        // Seeking to zero can discard the only keyframe in a video-only MKV.
        if offset_ms > 0 {
            command.args(["-ss", &seconds(offset_ms)]);
        }
        command.arg("-i").arg(list);
        command
    }
    async fn export_concat_copy(
        &self,
        plan: &ClipPlan,
        dest: &Path,
        staging: &Path,
        audio_tracks: usize,
    ) -> Result<Probe> {
        self.report(MediaStage::Copying);
        let list = concat_list(plan)?;
        let listpath = staging.join("sources.ffconcat");
        fs::write(&listpath, list).await?;
        let pending = staging.join("output.mkv");
        let duration: i64 = plan.sources.iter().map(|range| range.duration_ms).sum();
        self.run_ffmpeg(
            self.concat_command(&listpath, plan.sources[0].offset_ms)
                .args([
                    "-t",
                    &seconds(duration),
                    "-map",
                    "0:v:0",
                    "-map",
                    "0:a?",
                    "-c",
                    "copy",
                    "-avoid_negative_ts",
                    "make_zero",
                ])
                .arg(&pending),
        )
        .await?;
        let probe = self
            .validate_export(plan, &pending, CutMode::Fast, audio_tracks)
            .await?;
        fs::rename(pending, dest)
            .await
            .context("publish copied media")?;
        Ok(probe)
    }
    async fn validate_export(
        &self,
        plan: &ClipPlan,
        path: &Path,
        mode: CutMode,
        expected_audio: usize,
    ) -> Result<Probe> {
        self.report(MediaStage::Checking);
        let probe = self.probe(path).await?;
        if probe.audio_tracks != expected_audio {
            bail!("export lost audio tracks");
        }
        let wanted: i64 = plan.sources.iter().map(|r| r.duration_ms).sum();
        let tolerance = if matches!(mode, CutMode::Precise) {
            250
        } else {
            500 * i64::try_from(plan.sources.len())?
        };
        if (probe.duration_ms - wanted).abs() > tolerance {
            bail!("export duration differs from requested range");
        }
        self.verify_decodable(path).await?;
        Ok(probe)
    }
    /// Decodes every video frame and audio stream before publishing a finished asset.
    pub async fn verify_decodable(&self, path: &Path) -> Result<()> {
        self.report(MediaStage::Checking);
        self.run_ffmpeg(
            Command::new(&self.ffmpeg)
                .args(["-nostdin", "-v", "error", "-xerror", "-i"])
                .arg(path)
                .args(["-map", "0:v:0", "-map", "0:a?", "-f", "null", "-"]),
        )
        .await?;
        Ok(())
    }
    /// Renders one JPEG frame, at most 640 pixels wide, without modifying the source.
    pub async fn thumbnail(&self, source: &Path, at_ms: i64) -> Result<Vec<u8>> {
        let info = self.probe(source).await?;
        anyhow::ensure!(
            at_ms >= 0 && at_ms < info.duration_ms,
            "Thumbnail time is outside the video"
        );
        self.report(MediaStage::Poster);
        let staging = tempfile::tempdir()?;
        let image = staging.path().join("poster.jpg");
        self.run_ffmpeg(
            Command::new(&self.ffmpeg)
                .args(["-nostdin", "-v", "error", "-ss", &seconds(at_ms), "-i"])
                .arg(source)
                .args([
                    "-map",
                    "0:v:0",
                    "-frames:v",
                    "1",
                    "-an",
                    "-vf",
                    "scale=w='min(640,iw)':h=-2",
                    "-q:v",
                    "3",
                    "-update",
                    "1",
                ])
                .arg(&image),
        )
        .await?;
        let bytes = fs::read(image).await?;
        anyhow::ensure!(
            bytes.starts_with(&[0xff, 0xd8]) && bytes.ends_with(&[0xff, 0xd9]),
            "Thumbnail is not a complete JPEG"
        );
        Ok(bytes)
    }

    /// Makes a browser-playable copy with one selected audio stream, retaining the original file.
    pub async fn preview(&self, source: &Path, dest: &Path, track: usize) -> Result<()> {
        self.preview_mix(source, dest, &[track]).await
    }
    /// Makes a browser copy with selected tracks mixed into one stream; an empty selection is silent.
    pub async fn preview_mix(&self, source: &Path, dest: &Path, tracks: &[usize]) -> Result<()> {
        if dest.exists() {
            return Ok(());
        }
        let info = self.probe(source).await?;
        if info.audio_tracks > 0 && tracks.iter().any(|track| *track >= info.audio_tracks) {
            bail!("audio track out of range");
        }
        self.report(MediaStage::Playback);
        let pending = dest.with_extension("partial.mp4");
        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(source)
            .args(["-map", "0:v:0"]);
        let mut tracks = tracks.to_vec();
        tracks.sort_unstable();
        tracks.dedup();
        if info.audio_tracks == 0 {
            tracks.clear();
        }
        if tracks.len() > 1 {
            use std::fmt::Write as _;
            let mut inputs = String::new();
            for track in &tracks {
                write!(inputs, "[0:a:{track}]")?;
            }
            cmd.args([
                "-filter_complex",
                &format!(
                    "{inputs}amix=inputs={}:duration=longest:dropout_transition=0:normalize=1[mix]",
                    tracks.len()
                ),
                "-map",
                "[mix]",
            ]);
        } else if let Some(track) = tracks.first() {
            cmd.args(["-map", &format!("0:a:{track}")]);
        } else {
            cmd.arg("-an");
        }
        if info.video_codec == "h264" && info.pixel_format == "yuv420p" {
            cmd.args(["-c:v", "copy"]);
        } else {
            cmd.args([
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "22", "-pix_fmt", "yuv420p",
            ]);
        }
        cmd.args([
            "-c:a",
            if tracks.len() == 1
                && info
                    .audio_codecs
                    .get(tracks[0])
                    .is_some_and(|codec| codec == "aac")
            {
                "copy"
            } else {
                "aac"
            },
            "-movflags",
            "+faststart",
        ])
        .arg(&pending);
        self.run_ffmpeg(&mut cmd).await?;
        self.probe(&pending).await?;
        fs::rename(pending, dest).await?;
        Ok(())
    }
    #[cfg(test)]
    async fn test_fixture(&self, dest: &Path) -> Result<Probe> {
        checked(
            Command::new(&self.ffmpeg)
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=320x180:rate=30",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=48000",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=880:sample_rate=48000",
                    "-t",
                    "3",
                    "-map",
                    "0:v",
                    "-map",
                    "1:a",
                    "-map",
                    "2:a",
                    "-c:v",
                    "libx264",
                    "-g",
                    "30",
                    "-pix_fmt",
                    "yuv420p",
                    "-c:a",
                    "aac",
                    "-metadata:s:a:0",
                    "title=Game (440 Hz)",
                    "-metadata:s:a:1",
                    "title=Voice (880 Hz)",
                ])
                .arg(dest),
        )
        .await?;
        self.probe(dest).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use owc_domain::{Activity, Kind, Outcome, Segment, plan_clip};
    #[tokio::test]
    async fn mixed_preview_contains_both_tracks_and_supports_silent_export() {
        let dir = tempfile::tempdir().expect("temp");
        let media = Media::default();
        let source = dir.path().join("source.mkv");
        media.test_fixture(&source).await.expect("fixture");
        assert_eq!(
            media.probe(&source).await.expect("probe").audio_titles,
            ["Game (440 Hz)", "Voice (880 Hz)"]
        );
        let mixed = dir.path().join("mixed.mp4");
        media
            .preview_mix(&source, &mixed, &[0, 1])
            .await
            .expect("mix");
        assert_eq!(media.probe(&mixed).await.expect("probe").audio_tracks, 1);
        let decoded = Command::new(&media.ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(&mixed)
            .args([
                "-map", "0:a:0", "-f", "f32le", "-ac", "1", "-ar", "48000", "pipe:1",
            ])
            .output()
            .await
            .expect("decode");
        assert!(decoded.status.success());
        let samples: Vec<_> = decoded
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        for frequency in [440.0, 880.0] {
            let (sin, cos) =
                samples
                    .iter()
                    .enumerate()
                    .fold((0.0, 0.0), |(sin, cos), (i, sample)| {
                        let phase = std::f64::consts::TAU
                            * frequency
                            * f64::from(u32::try_from(i).expect("fixture length"))
                            / 48000.0;
                        (
                            f64::from(*sample).mul_add(phase.sin(), sin),
                            f64::from(*sample).mul_add(phase.cos(), cos),
                        )
                    });
            assert!(
                sin.hypot(cos) / f64::from(u32::try_from(samples.len()).expect("fixture length"))
                    > 0.01,
                "missing tone {frequency}"
            );
        }
        let silent = dir.path().join("silent.mp4");
        media
            .preview_mix(&source, &silent, &[])
            .await
            .expect("silent");
        assert_eq!(media.probe(&silent).await.expect("probe").audio_tracks, 0);
        assert!(
            media
                .preview_mix(&source, &dir.path().join("bad.mp4"), &[2])
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn concat_copy_preserves_frames_tracks_and_paths() {
        async fn frames(path: &Path) -> Vec<String> {
            let bytes = checked(
                Command::new("ffmpeg")
                    .args(["-v", "error", "-i"])
                    .arg(path)
                    .args([
                        "-map",
                        "0:v:0",
                        "-fps_mode",
                        "passthrough",
                        "-f",
                        "framemd5",
                        "-",
                    ]),
            )
            .await
            .expect("frame hashes");
            String::from_utf8(bytes)
                .expect("text")
                .lines()
                .filter(|line| !line.starts_with('#'))
                .map(|line| line.rsplit(',').next().expect("hash").trim().to_owned())
                .collect()
        }

        let dir = tempfile::tempdir().expect("temp");
        let media = Media::default();
        let src = dir.path().join("source ' quoted.mkv");
        let info = media.test_fixture(&src).await.expect("fixture");
        let mut activity = Activity::new(Kind::Manual, "copy".into(), 0, "manual");
        activity.finish(info.duration_ms * 3, Outcome::Unknown);
        let segments: Vec<_> = (0..3)
            .map(|n| Segment {
                id: uuid::Uuid::new_v4(),
                session: activity.id,
                path: src.clone(),
                start_ms: n * info.duration_ms,
                end_ms: (n + 1) * info.duration_ms,
                closed: true,
                managed: true,
            })
            .collect();
        let plan = plan_clip(&activity, &segments, 0, 0);
        let dest = dir.path().join("copied.mkv");
        let output = media
            .export(&plan, &dest, CutMode::Fast)
            .await
            .expect("copy");
        assert_eq!(output.export.expect("method").encoder, "copy");
        assert_eq!(output.audio_tracks, 2);
        let original = frames(&src).await;
        let expected: Vec<_> = (0..3).flat_map(|_| original.iter().cloned()).collect();
        let actual = frames(&dest).await;
        assert_eq!(actual.len(), expected.len());
        assert!(actual == expected, "copy changed decoded frames");
        let preview = dir.path().join("early.mp4");
        let review = media
            .preview_plan(&plan, &preview, 1)
            .await
            .expect("retained playback");
        assert_eq!(review.origin_ms, 0);
        assert!(!review.converted);
        assert_eq!(review.probe.audio_tracks, 1);
        assert_eq!(frames(&preview).await, expected);
        assert!((review.probe.duration_ms - info.duration_ms * 3).abs() < 250);
        assert!(
            media
                .preview_plan(&plan, &dir.path().join("invalid.mp4"), 4)
                .await
                .is_err()
        );
        let mut inputs = vec![media.probe(&src).await.expect("probe")];
        inputs.push(inputs[0].clone());
        inputs[1].stream_layout[0]["width"] = serde_json::json!(999);
        assert!(direct_copy_compatible(&plan, &inputs).is_err());
    }
    #[test]
    fn audio_titles_read_obs_track_names() {
        for (key, name) in [
            ("NAME", "Full Mix"),
            ("name", "Game"),
            ("Name", "Mic"),
            ("NAME", "Chat"),
        ] {
            let stream = serde_json::json!({"tags":{key:name,"HANDLER_NAME":"OBS Audio Handler"}});
            assert_eq!(audio_title(&stream), name);
        }
        assert_eq!(
            audio_title(&serde_json::json!({"tags":{"title":" ","NAME":" Game "}})),
            "Game"
        );
        assert_eq!(
            audio_title(
                &serde_json::json!({"tags":{"TITLE":"Edited title","NAME":"Original name"}})
            ),
            "Edited title"
        );
    }

    #[test]
    fn audio_titles_ignore_container_handlers_and_preserve_real_names() {
        for handler in ["SoundHandler", "AudioHandler", "OBS Audio Handler"] {
            assert_eq!(
                audio_title(&serde_json::json!({"tags":{"HANDLER_NAME":handler}})),
                ""
            );
        }
        assert_eq!(
            audio_title(
                &serde_json::json!({"tags":{"TITLE":" Game ","handler_name":"OBS Audio Handler"}})
            ),
            "Game"
        );
        assert_eq!(
            audio_title(&serde_json::json!({"tags":{"handler_name":"Microphone"}})),
            "Microphone"
        );
        assert_eq!(
            audio_title(&serde_json::json!({"tags":{"title":" ","HANDLER_NAME":"Chat"}})),
            "Chat"
        );
    }

    #[tokio::test]
    async fn every_encoder_exports_or_reports_cpu_fallback() {
        let dir = tempfile::tempdir().expect("temp");
        let src = dir.path().join("source.mkv");
        let fixture = Media::default().test_fixture(&src).await.expect("fixture");
        let mut activity = Activity::new(Kind::Manual, "encoder matrix".into(), 0, "manual");
        activity.finish(3000, Outcome::Unknown);
        let segments: Vec<_> = (0..2)
            .map(|n| Segment {
                id: uuid::Uuid::new_v4(),
                session: activity.id,
                path: src.clone(),
                start_ms: n * fixture.duration_ms,
                end_ms: (n + 1) * fixture.duration_ms,
                closed: true,
                managed: true,
            })
            .collect();
        let plan = plan_clip(&activity, &segments, 0, 0);
        for encoder in [
            Encoder::Cpu,
            Encoder::Nvidia,
            Encoder::Intel,
            Encoder::Amd,
            Encoder::Vaapi,
        ] {
            let media = Media {
                encoder,
                ..Media::default()
            };
            for (name, mode) in [("precise", CutMode::Precise), ("copy", CutMode::Fast)] {
                let output = media
                    .export(
                        &plan,
                        &dir.path().join(format!("{}-{name}.mkv", encoder.name())),
                        mode,
                    )
                    .await
                    .expect("usable export");
                assert_eq!(output.audio_tracks, 2);
                assert!(
                    (output.duration_ms - 3000).abs() < 150,
                    "{} {name}: {}",
                    encoder.name(),
                    output.duration_ms
                );
                let actual = output.export.expect("actual encoder metadata");
                if let Some(reason) = actual.fallback {
                    assert_eq!(actual.encoder, "libx264");
                    assert!(!reason.is_empty());
                } else {
                    assert_eq!(
                        actual.encoder,
                        if matches!(mode, CutMode::Fast) {
                            "copy"
                        } else {
                            encoder.name()
                        }
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn unavailable_gpu_retries_cpu_and_reports_actual_encoder() {
        let dir = tempfile::tempdir().expect("temp");
        let media = Media {
            encoder: Encoder::Vaapi,
            vaapi_device: dir.path().join("missing-device"),
            ..Media::default()
        };
        let src = dir.path().join("source.mkv");
        let info = media.test_fixture(&src).await.expect("fixture");
        let mut activity = Activity::new(Kind::Manual, "fallback".into(), 0, "manual");
        activity.finish(1000, Outcome::Unknown);
        let segment = Segment {
            id: activity.id,
            session: activity.id,
            path: src,
            start_ms: 0,
            end_ms: info.duration_ms,
            closed: true,
            managed: true,
        };
        let plan = plan_clip(&activity, &[segment], 0, 0);
        let output = media
            .export(&plan, &dir.path().join("fallback.mkv"), CutMode::Precise)
            .await
            .expect("CPU fallback");
        let export = output.export.expect("method");
        assert_eq!(export.encoder, "libx264");
        assert!(export.fallback.is_some());
        assert_eq!(output.audio_tracks, 2);
    }
    async fn continuity_fixture(media: &Media, dest: &Path) {
        // PCM avoids repeating source AAC priming silence. Keep a non-frame-aligned
        // video offset to exercise FFmpeg 6's encoder timestamp rounding.
        checked(
            Command::new(&media.ffmpeg)
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=320x180:rate=30",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=48000",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=880:sample_rate=48000",
                    "-t",
                    "3",
                    "-map",
                    "0:v",
                    "-map",
                    "1:a",
                    "-map",
                    "2:a",
                    "-c:v",
                    "libx264",
                    "-bf",
                    "0",
                    "-g",
                    "30",
                    "-pix_fmt",
                    "yuv420p",
                    "-vf",
                    "settb=1/1000,setpts=PTS+21",
                    "-enc_time_base:v",
                    "1/1000",
                    "-fps_mode",
                    "passthrough",
                    "-c:a",
                    "pcm_s16le",
                ])
                .arg(dest),
        )
        .await
        .expect("continuous source fixture");
    }

    #[tokio::test]
    async fn precise_many_segments_do_not_accumulate_encoder_delay() {
        let dir = tempfile::tempdir().expect("temp");
        let encoder = std::env::var("OWC_TEST_ENCODER")
            .ok()
            .map(|value| serde_json::from_value(Value::String(value)).expect("encoder name"))
            .unwrap_or_default();
        let media = Media {
            encoder,
            ..Media::default()
        };
        let src = dir.path().join("source.mkv");
        continuity_fixture(&media, &src).await;
        let mut activity = Activity::new(Kind::Manual, "many splits".into(), 0, "manual");
        activity.finish(20_000, Outcome::Unknown);
        let segments: Vec<_> = (0..20)
            .map(|n| Segment {
                id: activity.id,
                session: activity.id,
                path: src.clone(),
                start_ms: n * 1000,
                end_ms: (n + 1) * 1000,
                closed: true,
                managed: true,
            })
            .collect();
        let plan = plan_clip(&activity, &segments, 0, 0);
        let probe = media
            .export(&plan, &dir.path().join("many.mkv"), CutMode::Precise)
            .await
            .expect("many segment export");
        assert_eq!(
            probe.export.as_ref().expect("encoder").encoder,
            encoder.name()
        );
        assert!(probe.export.as_ref().expect("encoder").fallback.is_none());
        assert!(
            (probe.duration_ms - 20_000).abs() < 100,
            "duration {}",
            probe.duration_ms
        );
        assert_eq!(probe.audio_tracks, 2);
        let output = dir.path().join("many.mkv");
        let frame_data = checked(
            Command::new(&media.ffprobe)
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_frames",
                    "-show_entries",
                    "frame=best_effort_timestamp_time",
                    "-of",
                    "json",
                ])
                .arg(&output),
        )
        .await
        .expect("frame timestamps");
        let frames: Value = serde_json::from_slice(&frame_data).expect("frames");
        let times: Vec<f64> = frames["frames"]
            .as_array()
            .expect("array")
            .iter()
            .map(|f| {
                f["best_effort_timestamp_time"]
                    .as_str()
                    .expect("timestamp")
                    .parse()
                    .expect("number")
            })
            .collect();
        assert_eq!(times.len(), 600);
        assert!(
            times
                .windows(2)
                .all(|t| (0.032..=0.035).contains(&(t[1] - t[0])))
        );
        for track in ["0:a:0", "0:a:1"] {
            let pcm = checked(
                Command::new(&media.ffmpeg)
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(&output)
                    .args([
                        "-map", track, "-ac", "1", "-ar", "48000", "-f", "f32le", "-",
                    ]),
            )
            .await
            .expect("decode audio");
            let samples: Vec<_> = pcm
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            assert!((960_000..965_000).contains(&samples.len()));
            // Ten-millisecond windows expose silence introduced by per-segment encoder priming.
            for window in samples[4800..955_200].as_chunks::<480>().0 {
                let energy: f32 = window.iter().map(|sample| sample * sample).sum();
                assert!(energy > 0.5, "audio gap in {track}");
            }
        }
    }
    #[tokio::test]
    #[ignore = "requires private 4 MiB tmpfs; run scripts/disk-full-smoke.py"]
    async fn disk_full_export_preserves_source_and_retries_after_space_is_freed() {
        let limited =
            PathBuf::from(std::env::var("OWC_LIMITED_FILESYSTEM").expect("private tmpfs"));
        let source_root = tempfile::tempdir().expect("source directory");
        let media = Media::default();
        let source = source_root.path().join("source.mkv");
        let info = media.test_fixture(&source).await.expect("fixture");
        let before = std::fs::read(&source).expect("source");
        let filler = limited.join("filler");
        std::fs::write(&filler, vec![0_u8; 4 * 1024 * 1024 - 4096])
            .expect("fill private filesystem");
        let mut activity = Activity::new(Kind::Manual, "Disk full".into(), 0, "manual");
        activity.finish(2000, Outcome::Unknown);
        let plan = plan_clip(
            &activity,
            &[Segment {
                id: activity.id,
                session: activity.id,
                path: source.clone(),
                start_ms: 0,
                end_ms: info.duration_ms,
                closed: true,
                managed: false,
            }],
            0,
            0,
        );
        let destination = limited.join("clip.mkv");
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            media.export(&plan, &destination, CutMode::Precise),
        )
        .await
        .expect("bounded failure");
        let error = result.expect_err("full filesystem must reject export");
        assert!(
            format!("{error:#}").contains("No space left on device"),
            "{error:#}"
        );
        assert!(!destination.exists());
        assert!(!limited.join(format!(".{}-export", activity.id)).exists());
        assert_eq!(std::fs::read(&source).expect("preserved source"), before);
        std::fs::remove_file(filler).expect("free space");
        media
            .export(&plan, &destination, CutMode::Precise)
            .await
            .expect("retry");
        media
            .verify_decodable(&destination)
            .await
            .expect("valid retry");
    }

    #[tokio::test]
    async fn thumbnail_is_decodable_bounded_and_does_not_change_source() {
        let root = tempfile::tempdir().expect("root");
        let media = Media::default();
        let source = root.path().join("source.mkv");
        let info = media.test_fixture(&source).await.expect("fixture");
        let before = std::fs::read(&source).expect("source");
        let bytes = media.thumbnail(&source, 1000).await.expect("thumbnail");
        let poster = root.path().join("poster.jpg");
        std::fs::write(&poster, bytes).expect("image");
        media.verify_decodable(&poster).await.expect("decode image");
        let data = checked(
            Command::new(&media.ffprobe)
                .args(["-v", "error", "-show_streams", "-of", "json"])
                .arg(&poster),
        )
        .await
        .expect("image dimensions");
        let data: Value = serde_json::from_slice(&data).expect("JSON");
        assert_eq!(data["streams"][0]["width"], 320);
        assert_eq!(data["streams"][0]["height"], 180);
        assert!(media.thumbnail(&source, -1).await.is_err());
        assert!(media.thumbnail(&source, info.duration_ms).await.is_err());
        assert_eq!(std::fs::read(&source).expect("source"), before);
    }

    #[tokio::test]
    async fn cancelled_export_cleans_staging_preserves_source_and_retries() {
        let dir = tempfile::tempdir().expect("temp");
        let mut media = Media::default();
        let source = dir.path().join("source.mkv");
        let info = media.test_fixture(&source).await.expect("fixture");
        let before = std::fs::read(&source).expect("source");
        let mut activity = Activity::new(Kind::Manual, "test".into(), 0, "manual");
        activity.finish(2000, Outcome::Unknown);
        let plan = plan_clip(
            &activity,
            &[Segment {
                id: activity.id,
                session: activity.id,
                path: source.clone(),
                start_ms: 0,
                end_ms: info.duration_ms,
                closed: true,
                managed: true,
            }],
            0,
            0,
        );
        let destination = dir.path().join("clip.mkv");
        let (cancel, cancelled) = tokio::sync::watch::channel(true);
        media.cancel = Some(cancelled);
        let (progress, messages) =
            tokio::sync::watch::channel(Progress::new(MediaStage::Preparing));
        media.progress = Some(progress);
        let error = media
            .export(&plan, &destination, CutMode::Fast)
            .await
            .expect_err("cancel");
        assert!(error.to_string().contains("cancelled"));
        assert_ne!(messages.borrow().stage, MediaStage::CpuFallback);
        assert!(!destination.exists());
        assert!(!dir.path().join(format!(".{}-export", activity.id)).exists());
        assert_eq!(std::fs::read(&source).expect("preserved source"), before);
        cancel.send_replace(false);
        media
            .export(&plan, &destination, CutMode::Fast)
            .await
            .expect("retry");
        media
            .verify_decodable(&destination)
            .await
            .expect("playable retry");
    }

    #[tokio::test]
    async fn fast_cut_seeks_near_late_encounters_and_keeps_preceding_keyframe() {
        let dir = tempfile::tempdir().expect("temp");
        let src = dir.path().join("long.mp4");
        checked(
            Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=10",
                    "-t",
                    "120",
                    "-c:v",
                    "libx264",
                    "-g",
                    "100",
                    "-keyint_min",
                    "100",
                    "-sc_threshold",
                    "0",
                ])
                .arg(&src),
        )
        .await
        .expect("long fixture");
        let mut activity = Activity::new(Kind::Manual, "late".into(), 71_500, "manual");
        activity.finish(76_000, Outcome::Unknown);
        let segment = Segment {
            id: activity.id,
            session: activity.id,
            path: src,
            start_ms: 1000,
            end_ms: 121_000,
            closed: true,
            managed: true,
        };
        let mut plan = plan_clip(&activity, &[segment], 0, 0);
        Media::default()
            .align_fast_cut(&mut plan)
            .await
            .expect("late seek");
        assert_eq!(plan.sources[0].offset_ms, 70_000);
        assert_eq!(plan.actual_start_ms, Some(71_000));
        assert_eq!(plan.sources[0].duration_ms, 5000);
    }

    #[tokio::test]
    async fn real_media_precise_cut_retains_tracks_and_preview() {
        async fn video_hash(path: &Path) -> Vec<u8> {
            checked(
                Command::new("ffmpeg")
                    .args(["-v", "error", "-i"])
                    .arg(path)
                    .args(["-map", "0:v:0", "-c:v", "copy", "-f", "hash", "-"]),
            )
            .await
            .expect("video stream hash")
        }
        let dir = tempfile::tempdir().expect("temp");
        let (progress, progress_rx) =
            tokio::sync::watch::channel(Progress::new(MediaStage::Preparing));
        let media = Media {
            progress: Some(progress),
            ..Media::default()
        };
        let src = dir.path().join("source.mkv");
        let original = media
            .test_fixture(&src)
            .await
            .expect("ffmpeg fixture requires ffmpeg/ffprobe on PATH");
        assert_eq!(original.audio_tracks, 2);
        let mut a = Activity::new(Kind::Manual, "test".into(), 500, "manual");
        a.finish(2500, Outcome::Unknown);
        let s = Segment {
            id: a.id,
            session: a.id,
            path: src,
            start_ms: 0,
            end_ms: original.duration_ms,
            closed: true,
            managed: true,
        };
        let plan = plan_clip(&a, &[s], 0, 0);
        let mut fast = plan.clone();
        media.align_fast_cut(&mut fast).await.expect("align");
        assert!(fast.actual_start_ms.expect("origin") < 500);
        assert!(fast.sources[0].duration_ms > 2000);
        let copied = media
            .export(&fast, &dir.path().join("fast.mkv"), CutMode::Fast)
            .await
            .expect("fast cut");
        assert_eq!(copied.audio_tracks, 2);
        assert_eq!(copied.export.expect("method").encoder, "copy");
        let dest = dir.path().join("clip.mkv");
        let p = media
            .export(&plan, &dest, CutMode::Precise)
            .await
            .expect("cut");
        assert_eq!(p.audio_tracks, 2);
        assert_eq!(p.audio_titles, ["Game (440 Hz)", "Voice (880 Hz)"]);
        assert!((p.duration_ms - 2000).abs() < 250);
        let preview = dir.path().join("preview.mp4");
        media.preview(&dest, &preview, 1).await.expect("preview");
        assert!(progress_rx.borrow().processed_seconds.is_some());
        assert_eq!(media.probe(&preview).await.expect("probe").audio_tracks, 1);
        assert_eq!(video_hash(&dest).await, video_hash(&preview).await);
        let modified = fs::metadata(&preview)
            .await
            .expect("cache")
            .modified()
            .expect("time");
        media
            .preview(&dest, &preview, 1)
            .await
            .expect("reuse cache");
        assert_eq!(
            modified,
            fs::metadata(&preview)
                .await
                .expect("cache")
                .modified()
                .expect("time")
        );

        // A crash after publication is retryable, but an unrelated existing file is not trusted.
        media
            .export(&plan, &dest, CutMode::Precise)
            .await
            .expect("validated retry");
        let wrong = dir.path().join("wrong.mkv");
        media
            .test_fixture(&wrong)
            .await
            .expect("wrong duration fixture");
        assert!(media.export(&plan, &wrong, CutMode::Precise).await.is_err());
        let mut missing = plan.clone();
        missing.sources.push(missing.sources[0].clone());
        missing.sources[1].segment.path = dir.path().join("missing.mkv");
        assert!(
            media
                .export(&missing, &dir.path().join("failed.mkv"), CutMode::Precise)
                .await
                .is_err()
        );
        assert!(
            !dir.path()
                .join(format!(".{}-export", plan.activity_id))
                .exists()
        );
    }
}
