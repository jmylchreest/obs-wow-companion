//! Read-only setup observations; picture and sound still require human verification.
use crate::config::Config;
use owc_obs_client::Client;
use serde::Serialize;
use serde_json::json;
use std::{path::Path, process::Stdio, time::Duration};

#[derive(Serialize)]
pub struct Check {
    id: &'static str,
    title: String,
    status: &'static str,
    detail: String,
}
#[derive(Serialize)]
pub struct Report {
    pub connection: &'static str,
    pub checks: Vec<Check>,
    pub recording_directory: Option<String>,
    pub outputs_active: bool,
    pub checked_at_ms: i64,
}
impl Report {
    fn add(
        &mut self,
        id: &'static str,
        title: impl Into<String>,
        status: &'static str,
        detail: impl Into<String>,
    ) {
        self.checks.push(Check {
            id,
            title: title.into(),
            status,
            detail: detail.into(),
        });
    }
}
async fn available(path: &Path) -> bool {
    let mut command = tokio::process::Command::new(path);
    command
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    matches!(tokio::time::timeout(Duration::from_secs(2), command.status()).await, Ok(Ok(status)) if status.success())
}
async fn parameter(client: &Client, category: &str, name: &str) -> Option<String> {
    client
        .request(
            "GetProfileParameter",
            json!({"parameterCategory":category,"parameterName":name}),
        )
        .await
        .ok()
        .and_then(|v| v["parameterValue"].as_str().map(str::to_owned))
}

pub async fn inspect(config: &Config, client: Option<&Client>) -> Report {
    let mut report = Report {
        connection: "unreachable",
        checks: vec![],
        recording_directory: None,
        outputs_active: false,
        checked_at_ms: chrono::Utc::now().timestamp_millis(),
    };
    if let Some(client) = client {
        report.connection = "connected";
        if tokio::time::timeout(
            Duration::from_secs(8),
            inspect_obs(config, client, &mut report),
        )
        .await
        .is_err()
        {
            report.add(
                "obs_check",
                "OBS settings",
                "fail",
                "Settings check timed out. Check the connection and try again.",
            );
        }
    } else {
        match owc_obs_client::probe(config.obs_port).await {
            Ok(true) => report.connection = "password_required",
            Ok(false) => report.connection = "available",
            Err(owc_obs_client::Error::Protocol(_)) => report.connection = "unsupported",
            Err(_) => {}
        }
    }
    let (ffmpeg, ffprobe) = tokio::join!(
        available(&config.media.ffmpeg),
        available(&config.media.ffprobe)
    );
    report.add("media_tools", "Media tools", if ffmpeg && ffprobe {"pass"} else {"fail"},
        if ffmpeg && ffprobe {"FFmpeg and ffprobe are available."}
        else {"FFmpeg or ffprobe is missing or did not respond. Install both, or set their paths in Setup → Media tools."});
    if config.installations.is_empty() {
        report.add(
            "logs",
            "WoW logs",
            "warn",
            "No installation configured. Add a Logs directory, or choose manual clips only.",
        );
    }
    for install in &config.installations {
        let readable = std::fs::read_dir(&install.logs).is_ok();
        report.add("logs", format!("{} logs", install.name), if readable {"pass"} else {"fail"},
            if readable {format!("Readable: {}. Enable /combatlog in game; detection still needs a supported activity.", install.logs.display())}
            else {format!("Cannot read {}. Check the path and folder permissions.", install.logs.display())});
    }
    report
}

#[allow(clippy::too_many_lines)] // Keep the read-only onboarding observations in their displayed order.
async fn inspect_obs(config: &Config, client: &Client, report: &mut Report) {
    let Ok(version) = client.request("GetVersion", json!({})).await else {
        report.connection = "unreachable";
        report.add(
            "connection",
            "OBS connection",
            "fail",
            "OBS stopped responding. Reconnect and check again.",
        );
        return;
    };
    report.add(
        "connection",
        "OBS connection",
        "pass",
        format!(
            "OBS {} · WebSocket {}",
            version["obsVersion"].as_str().unwrap_or("unknown"),
            version["obsWebSocketVersion"].as_str().unwrap_or("unknown")
        ),
    );
    let mut recording = false;
    for kind in [
        "GetRecordStatus",
        "GetStreamStatus",
        "GetReplayBufferStatus",
    ] {
        if let Ok(active) = client.output_active(kind).await {
            report.outputs_active |= active;
            if kind == "GetRecordStatus" {
                recording = active;
            }
        } else {
            report.outputs_active = true;
            report.add(
                "outputs",
                "OBS outputs",
                "warn",
                "Could not confirm idle outputs. Directory changes are unavailable until checked.",
            );
        }
    }
    if report.outputs_active {
        report.add("outputs", "OBS is already in use", "warn", "Recording, streaming or replay buffer is active. Setup checks do not change it. An existing recording stays observe-only.");
    }
    match client.request("GetRecordDirectory", json!({})).await {
        Ok(value) => {
            report.recording_directory = value["recordDirectory"].as_str().map(str::to_owned);
            let matches = report
                .recording_directory
                .as_ref()
                .and_then(|p| Path::new(p).canonicalize().ok())
                .zip(config.spool.canonicalize().ok())
                .is_some_and(|(a, b)| a == b);
            report.add("directory", "Recording folder", if matches {"pass"} else if recording {"warn"} else {"fail"},
                if matches {"OBS uses the companion spool. Verify both apps can write/read this folder."}
                else {"OBS uses another recording folder. Apply the companion spool while outputs are stopped, or observe an existing recording."});
        }
        Err(_) => report.add(
            "directory",
            "Recording folder",
            "fail",
            "OBS did not provide its recording folder.",
        ),
    }
    let mode = parameter(client, "Output", "Mode").await;
    let split = parameter(client, "AdvOut", "RecSplitFile").await;
    let split_type = parameter(client, "AdvOut", "RecSplitFileType").await;
    let rec_type = parameter(client, "AdvOut", "RecType").await;
    let supported = version["availableRequests"]
        .as_array()
        .is_some_and(|r| r.iter().any(|v| v == "SplitRecordFile"));
    let splitting = supported
        && mode.as_deref() == Some("Advanced")
        && split
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1")
        && split_type.as_deref() == Some("Manual")
        && rec_type.as_deref() == Some("Standard");
    report.add("splitting", "Save clips while you keep playing", if splitting {"pass"} else {"warn"},
        if splitting {"OBS can split recordings into short files for live review and automatic cleanup."}
        else {"In OBS: 1. Open Settings → Output. 2. Set Output Mode to Advanced. 3. Open Recording and choose Standard. 4. Enable Automatic File Splitting and choose Only split manually. 5. Click Apply, then Check settings again here. Without this, clips may wait until you stop recording."});
    match client.request("GetCurrentProgramScene", json!({})).await {
        Ok(scene) => {
            let items = client
                .request(
                    "GetSceneItemList",
                    json!({"sceneName":scene["currentProgramSceneName"]}),
                )
                .await
                .ok();
            let enabled = items
                .as_ref()
                .and_then(|v| v["sceneItems"].as_array())
                .is_some_and(|items| items.iter().any(|v| v["sceneItemEnabled"] == true));
            report.add("scene", "Capture scene", if enabled {"pass"} else {"warn"},
                if enabled {"The active scene has enabled sources. Check the OBS preview yourself: sources can still be black or show the wrong window."}
                else {"In OBS, click + under Sources and add Game Capture (Windows) or a window/screen capture (Linux). Select WoW, confirm it appears in the preview, then click Check settings again here."});
        }
        Err(_) => report.add(
            "scene",
            "Capture scene",
            "warn",
            "Could not inspect the active scene. Verify the scene and sources in OBS.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unavailable_endpoint_and_tools_are_not_reported_ready() {
        let root = tempfile::tempdir().expect("directory");
        let mut config = Config::new(root.path());
        config.obs_port = 0;
        config.media.ffmpeg = root.path().join("missing-ffmpeg");
        config.media.ffprobe = root.path().join("missing-ffprobe");
        let report = inspect(&config, None).await;
        assert_eq!(report.connection, "unreachable");
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.id == "media_tools" && c.status == "fail")
        );
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.id == "logs" && c.status == "warn")
        );
        assert!(report.recording_directory.is_none());
    }
}
