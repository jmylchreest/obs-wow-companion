use anyhow::{Context, Result};
use owc_obs_client::Client;
use serde_json::json;
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    let root = std::env::args().nth(1).context("test directory argument")?;
    let client = Client::connect(4457, "owc-isolated-test").await?;
    let mut events = client.subscribe();
    tokio::spawn(async move {
        while let Ok(e) = events.recv().await {
            tracing::info!(?e, "OBS event");
        }
    });
    tracing::info!(version=?client.request("GetVersion",json!({})).await?);
    let video = client.request("GetVideoSettings", json!({})).await?;
    anyhow::ensure!(
        video["outputWidth"] == 1280 && video["outputHeight"] == 720,
        "starter profile not loaded: {video}"
    );
    let scenes = client.request("GetSceneList", json!({})).await?;
    anyhow::ensure!(
        scenes["scenes"]
            .as_array()
            .context("scenes")?
            .iter()
            .any(|s| s["sceneName"] == "WoW: add capture and audio"),
        "starter scene collection not loaded"
    );
    client
        .request("CreateScene", json!({"sceneName":"OWC Test"}))
        .await?;
    client.request("CreateInput",json!({"sceneName":"OWC Test","inputName":"Test pattern","inputKind":"ffmpeg_source","inputSettings":{"local_file":format!("{root}/pattern.mp4"),"is_local_file":true,"looping":true},"sceneItemEnabled":true})).await?;
    client
        .request("SetCurrentProgramScene", json!({"sceneName":"OWC Test"}))
        .await?;
    anyhow::ensure!(
        owc_obs_client::probe(4457).await?,
        "OBS authentication discovery"
    );
    let mut setup_config = owc_app::config::Config::new(std::path::Path::new(&root));
    setup_config.spool = root.clone().into();
    let setup =
        serde_json::to_value(owc_app::readiness::inspect(&setup_config, Some(&client)).await)?;
    for id in ["directory", "splitting", "scene", "media_tools"] {
        anyhow::ensure!(
            setup["checks"]
                .as_array()
                .context("checks")?
                .iter()
                .any(|c| c["id"] == id && c["status"] == "pass"),
            "readiness check {id}: {setup}"
        );
    }
    std::fs::write(
        std::path::Path::new(&root).join("readiness.json"),
        serde_json::to_vec_pretty(&setup)?,
    )?;
    client.request("StartRecord", json!({})).await?;
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    let running = owc_app::readiness::inspect(&setup_config, Some(&client)).await;
    anyhow::ensure!(running.outputs_active, "active recording not detected");
    anyhow::ensure!(
        client.request("GetRecordStatus", json!({})).await?["outputActive"] == true,
        "readiness interrupted recording"
    );
    tracing::info!(status=?client.request("GetRecordStatus",json!({})).await?);
    tracing::info!(split=?client.request("SplitRecordFile",json!({})).await?);
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    let mut stopped = client.subscribe();
    tracing::info!(stop=?client.request("StopRecord",json!({})).await?);
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let owc_obs_client::Event::Obs { kind, data, .. } = stopped.recv().await?
                && kind == "RecordStateChanged"
                && data["outputState"] == "OBS_WEBSOCKET_OUTPUT_STOPPED"
            {
                return Ok::<_, anyhow::Error>(());
            }
        }
    })
    .await??;
    // Allow the frontend to complete its queued stop handler before a new session.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    verify_engine(std::path::Path::new(&root)).await?;
    Ok(())
}

async fn send(
    tx: &tokio::sync::mpsc::Sender<owc_app::engine::Request>,
    data: serde_json::Value,
) -> Result<()> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    tx.send(owc_app::engine::Request { data, reply }).await?;
    rx.await??;
    Ok(())
}

async fn verify_engine(spool: &std::path::Path) -> Result<()> {
    use std::time::Duration;
    use tokio::sync::{mpsc, watch};
    let root = spool.parent().context("parent")?.join("companion");
    let (tx, rx) = mpsc::channel(32);
    let (status_tx, status) = watch::channel(json!({}));
    let engine = owc_app::engine::Engine::new(root.clone(), tx.clone(), status_tx)?;
    let task = tokio::spawn(engine.run(rx));
    let mut config = owc_app::config::Config::new(&root);
    config.obs_port = 4457;
    config.spool = spool.to_owned();
    config.segment_seconds = 3;
    config.pre_seconds = 1;
    config.post_seconds = 1;
    config.minimum_free_gib = 0;
    let logs = root.join("logs");
    std::fs::create_dir_all(&logs)?;
    config.installations.push(owc_app::config::Installation {
        id: uuid::Uuid::new_v4(),
        name: "Synthetic Retail".into(),
        logs: logs.clone(),
        flavour: owc_wow_log::Flavour::Retail,
        categories: vec![owc_domain::Kind::Raid],
        difficulties: vec![],
        minimum_key: 0,
    });
    send(&tx, json!({"action":"save_config", "config":config})).await?;
    send(
        &tx,
        json!({"action":"connect", "password":"owc-isolated-test"}),
    )
    .await?;
    verify_credentials(&tx, &status, &root).await?;
    for _ in 0..80 {
        if status.borrow()["setup"]["connection"] == "connected"
            && status.borrow()["setup"]["checking"] == false
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::ensure!(
        send(&tx, json!({"action":"finish_setup"})).await.is_ok(),
        "setup did not complete"
    );
    anyhow::ensure!(
        status.borrow()["armed"] != true,
        "setup must not arm capture"
    );
    let (scene_client, original_scene) = start_with_recording_scene(&tx, &status).await?;
    tokio::time::sleep(Duration::from_secs(4)).await;
    send(&tx, json!({"action":"manual_start"})).await?;
    append_event(&logs, "ENCOUNTER_START,2001,Mid-dungeon boss,8,5")?;
    append_event(
        &logs,
        "UNIT_DIED,0000000000000000,nil,0x80000000,0x80000000,Player-1-TEST,Test player,0x511,0x0,0",
    )?;
    append_event(&logs, "ENCOUNTER_END,2001,Mid-dungeon boss,8,5,1")?;
    append_event(&logs, "ENCOUNTER_START,1001,Integration boss,16,20")?;
    tokio::time::sleep(Duration::from_secs(4)).await;
    send(&tx, json!({"action":"manual_stop"})).await?;
    append_event(&logs, "ENCOUNTER_END,1001,Integration boss,16,20,1")?;
    tokio::time::sleep(Duration::from_secs(3)).await;
    send(&tx, json!({"action":"disarm"})).await?;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let state = status.borrow().clone();
        // Closed fragments can finish exporting before OBS confirms its final stop.
        // Scene restoration belongs to that confirmed stop, not clip readiness.
        if state["ownership"] == "idle"
            && state["activities"].as_array().is_some_and(|activities| {
                activities.len() == 2 && activities.iter().all(|a| a["status"] == "ready")
            })
        {
            anyhow::ensure!(
                state["activities"][0]["audio_tracks"] == 2,
                "audio tracks lost: {state}"
            );
            anyhow::ensure!(
                state["segments"].as_u64().unwrap_or(0) >= 2,
                "segments missing: {state}"
            );
            verify_manual_markers(&state)?;
            std::fs::write(
                root.join("evidence.json"),
                serde_json::to_vec_pretty(&state)?,
            )?;
            verify_scene_restored(&scene_client, &original_scene).await?;
            verify_automatic_capture(&tx, &status, &logs).await?;
            verify_delayed_capture(&status, &logs, &root).await?;
            verify_shutdown(&tx, &status).await?;
            task.abort();
            tracing::info!(
                "Companion end-to-end manual and combat-log capture, split, export and two audio tracks passed"
            );
            return Ok(());
        }
    }
    let state = status.borrow().clone();
    task.abort();
    anyhow::bail!("companion did not produce clip: {state}")
}

fn verify_manual_markers(state: &serde_json::Value) -> Result<()> {
    let manual = state["activities"]
        .as_array()
        .and_then(|items| items.iter().find(|a| a["kind"] == "manual"))
        .context("manual clip missing")?;
    let markers = manual["markers"]
        .as_array()
        .context("manual markers missing")?;
    anyhow::ensure!(
        manual["installation_id"].is_string(),
        "manual source identity missing"
    );
    anyhow::ensure!(
        markers.len() == 4,
        "manual marker window or duplicates incorrect: {markers:?}"
    );
    for kind in ["boss_start", "death", "boss_end"] {
        anyhow::ensure!(
            markers.iter().any(|marker| marker["kind"] == kind),
            "manual marker missing: {kind}"
        );
    }
    Ok(())
}

fn append_event(logs: &std::path::Path, event: &str) -> Result<()> {
    append_event_at(logs, event, chrono::Local::now())
}

fn append_event_at(
    logs: &std::path::Path,
    event: &str,
    at: chrono::DateTime<chrono::Local>,
) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(logs.join("WoWCombatLog-synthetic.txt"))?;
    writeln!(file, "{}  {event}", at.format("%m/%d/%Y %H:%M:%S%.3f"))?;
    Ok(())
}

async fn verify_credentials(
    tx: &tokio::sync::mpsc::Sender<owc_app::engine::Request>,
    status: &tokio::sync::watch::Receiver<serde_json::Value>,
    root: &std::path::Path,
) -> Result<()> {
    if std::env::var("OWC_TEST_PRIVATE_KEYRING").as_deref() == Ok("1") {
        send(
            tx,
            json!({"action":"connect","password":"owc-isolated-test","remember":true}),
        )
        .await?;
        anyhow::ensure!(
            status.borrow()["credential_status"] == "Password saved in your system password store.",
            "password was not saved"
        );
        anyhow::ensure!(
            send(
                tx,
                json!({"action":"connect","password":"incorrect-fixture","remember":true})
            )
            .await
            .is_err(),
            "incorrect password accepted"
        );
        send(tx, json!({"action":"connect","use_saved":true})).await?;
        anyhow::ensure!(
            !status.borrow().to_string().contains("owc-isolated-test"),
            "status exposed password"
        );
        let (restart_tx, restart_rx) = tokio::sync::mpsc::channel(32);
        let (restart_status_tx, restart_status) = tokio::sync::watch::channel(json!({}));
        let restarted =
            owc_app::engine::Engine::new(root.to_owned(), restart_tx, restart_status_tx)?;
        let restart_task = tokio::spawn(restarted.run(restart_rx));
        for _ in 0..100 {
            if restart_status.borrow()["connected"] == true {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let restarted_ok = restart_status.borrow()["connected"] == true;
        let stayed_disarmed = restart_status.borrow()["armed"] == false;
        restart_task.abort();
        let _ = restart_task.await;
        anyhow::ensure!(
            restarted_ok && stayed_disarmed,
            "restart did not safely reuse saved credentials"
        );
        send(tx, json!({"action":"forget_password"})).await?;
        anyhow::ensure!(
            send(tx, json!({"action":"connect","use_saved":true}))
                .await
                .is_err(),
            "deleted password still usable"
        );
        send(
            tx,
            json!({"action":"connect","password":"owc-isolated-test"}),
        )
        .await?;
    }
    Ok(())
}

async fn start_with_recording_scene(
    tx: &tokio::sync::mpsc::Sender<owc_app::engine::Request>,
    status: &tokio::sync::watch::Receiver<serde_json::Value>,
) -> Result<(Client, String)> {
    let scene_client = Client::connect(4457, "owc-isolated-test").await?;
    let original_scene = scene_client
        .request("GetCurrentProgramScene", json!({}))
        .await?["currentProgramSceneName"]
        .as_str()
        .context("scene")?
        .to_owned();
    scene_client
        .request(
            "CreateScene",
            json!({"sceneName":"Companion previous scene"}),
        )
        .await?;
    let mut scene_config = status.borrow()["config"].clone();
    scene_config["recording_scene"] = original_scene.clone().into();
    send(tx, json!({"action":"save_config","config":scene_config})).await?;
    scene_client
        .request(
            "SetCurrentProgramScene",
            json!({"sceneName":"Companion previous scene"}),
        )
        .await?;
    send(tx, json!({"action":"arm"})).await?;
    anyhow::ensure!(
        scene_client
            .request("GetCurrentProgramScene", json!({}))
            .await?["currentProgramSceneName"]
            == original_scene,
        "recording scene not selected"
    );
    Ok((scene_client, original_scene))
}

async fn verify_scene_restored(scene_client: &Client, original_scene: &str) -> Result<()> {
    anyhow::ensure!(
        scene_client
            .request("GetCurrentProgramScene", json!({}))
            .await?["currentProgramSceneName"]
            == "Companion previous scene",
        "previous scene not restored"
    );
    scene_client
        .request(
            "SetCurrentProgramScene",
            json!({"sceneName":original_scene}),
        )
        .await?;
    scene_client
        .request(
            "RemoveScene",
            json!({"sceneName":"Companion previous scene"}),
        )
        .await?;
    Ok(())
}

async fn verify_automatic_capture(
    tx: &tokio::sync::mpsc::Sender<owc_app::engine::Request>,
    status: &tokio::sync::watch::Receiver<serde_json::Value>,
    logs: &std::path::Path,
) -> Result<()> {
    let mut config = status.borrow()["config"].clone();
    config["export_mode"] = "fast".into();
    // Keep this short encounter in one file so its export requires the final close.
    config["segment_seconds"] = 60.into();
    send(tx, json!({"action":"save_config", "config":config})).await?;
    send(tx, json!({"action":"pause_automatic"})).await?;
    append_event(logs, "ENCOUNTER_START,1002,Paused boss,16,20")?;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    anyhow::ensure!(status.borrow()["armed"] == false, "paused capture started");
    append_event(logs, "ENCOUNTER_END,1002,Paused boss,16,20,1")?;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    send(tx, json!({"action":"resume_automatic"})).await?;
    let monitor = Client::connect(4457, "owc-isolated-test").await?;
    let (closed_tx, closed_rx) = tokio::sync::watch::channel(None);
    let mut events = monitor.subscribe();
    let closure_task = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let owc_obs_client::Event::Obs { kind, data, .. } = event
                && kind == "RecordStateChanged"
                && data["outputState"] == "OBS_WEBSOCKET_OUTPUT_STOPPED"
            {
                let _ = closed_tx.send(Some(std::time::Instant::now()));
                break;
            }
        }
    });
    append_event(logs, "ENCOUNTER_START,1003,Automatic boss,16,20")?;
    for _ in 0..40 {
        if status.borrow()["armed"] == true {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::ensure!(
        status.borrow()["armed"] == true,
        "combat log did not start OBS"
    );
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    append_event(logs, "ENCOUNTER_END,1003,Automatic boss,16,20,1")?;
    let mut queued_at = None;
    for _ in 0..100 {
        let state = status.borrow().clone();
        if queued_at.is_none()
            && state["activities"].as_array().is_some_and(|items| {
                items.iter().any(|a| {
                    a["name"] == "Automatic boss"
                        && matches!(a["status"].as_str(), Some("exporting" | "ready"))
                })
            })
        {
            queued_at = Some(std::time::Instant::now());
        }
        if state["armed"] == false
            && state["activities"].as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|a| a["name"] == "Automatic boss" && a["status"] == "ready")
            })
        {
            let activity = state["activities"]
                .as_array()
                .context("activities")?
                .iter()
                .find(|a| a["name"] == "Automatic boss")
                .context("automatic clip")?;
            let path = std::path::PathBuf::from(activity["path"].as_str().context("clip path")?)
                .with_extension("json");
            let sidecar: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
            anyhow::ensure!(
                sidecar["media"]["export"]["encoder"] == "copy",
                "Automatic fast export fell back: {sidecar}"
            );
            let closed_at = (*closed_rx.borrow()).context("final OBS closure event")?;
            let latency = queued_at
                .context("export entered queue")?
                .saturating_duration_since(closed_at);
            closure_task.await?;
            record_queue_latency(logs, &monitor, latency).await?;
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    closure_task.abort();
    anyhow::bail!(
        "automatic capture did not stop and export: {}",
        *status.borrow()
    );
}

async fn record_queue_latency(
    logs: &std::path::Path,
    monitor: &Client,
    latency: std::time::Duration,
) -> Result<()> {
    let evidence = json!({
        "scenario":"single-file automatic raid, fast copy, idle export worker",
        "closure_source":"independent OBS RecordStateChanged STOPPED subscription",
        "queue_observation":"first exporting or ready UI snapshot",
        "poll_interval_ms":100,
        "closure_to_export_observed_ms":latency.as_secs_f64()*1000.0,
        "threshold_ms":2000,
        "obs":monitor.request("GetVersion", json!({})).await?,
        "passed":latency <= std::time::Duration::from_secs(2)
    });
    std::fs::write(
        logs.parent()
            .context("evidence directory")?
            .join("queue-latency.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    anyhow::ensure!(
        latency <= std::time::Duration::from_secs(2),
        "closure-to-export exceeded two seconds: {evidence}"
    );
    tracing::info!(?latency, "Native OBS closure-to-export latency passed");
    Ok(())
}

async fn verify_shutdown(
    tx: &tokio::sync::mpsc::Sender<owc_app::engine::Request>,
    status: &tokio::sync::watch::Receiver<serde_json::Value>,
) -> Result<()> {
    send(tx, json!({"action":"manual_start"})).await?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    anyhow::ensure!(
        status.borrow()["armed"] == true,
        "shutdown fixture is not recording"
    );
    send(tx, json!({"action":"shutdown"})).await?;
    for _ in 0..100 {
        if status.borrow()["shutdown_ready"] == true {
            anyhow::ensure!(
                status.borrow()["ownership"] == "idle",
                "owned OBS output did not stop"
            );
            tracing::info!("Native OBS shutdown stopped owned capture and saved unfinished work");
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!("shutdown did not settle: {}", *status.borrow())
}

async fn verify_delayed_capture(
    status: &tokio::sync::watch::Receiver<serde_json::Value>,
    logs: &std::path::Path,
    root: &std::path::Path,
) -> Result<()> {
    for truncated in [false, true] {
        verify_delayed_case(status, logs, root, truncated).await?;
    }
    Ok(())
}

async fn verify_delayed_case(
    status: &tokio::sync::watch::Receiver<serde_json::Value>,
    logs: &std::path::Path,
    root: &std::path::Path,
    truncated: bool,
) -> Result<()> {
    use std::time::Duration;
    if truncated {
        // Keep the requested interval after the previous session, so this tests
        // missing leading footage rather than a real gap between two recordings.
        tokio::time::sleep(Duration::from_secs(35)).await;
    }
    append_event(logs, "ENCOUNTER_START,1005,Earlier boss,16,20")?;
    for _ in 0..40 {
        if status.borrow()["armed"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::ensure!(
        status.borrow()["armed"] == true,
        "earlier encounter did not start capture"
    );
    tokio::time::sleep(Duration::from_secs(4)).await;
    let name = if truncated {
        "Partial boss"
    } else {
        "Delayed boss"
    };
    let start = chrono::Local::now() - chrono::Duration::seconds(if truncated { 30 } else { 0 });
    tokio::time::sleep(Duration::from_secs(if truncated { 1 } else { 30 })).await;
    append_event_at(logs, &format!("ENCOUNTER_START,1004,{name},16,20"), start)?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    append_event(logs, &format!("ENCOUNTER_END,1004,{name},16,20,1"))?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    for _ in 0..160 {
        let state = status.borrow().clone();
        if let Some(activity) = state["activities"].as_array().and_then(|items| {
            items
                .iter()
                .find(|a| a["name"] == name && a["status"] == "ready")
        }) {
            let path =
                std::path::PathBuf::from(activity["path"].as_str().context("delayed clip path")?);
            let sidecar: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path.with_extension("json"))?)?;
            verify_delayed_coverage(&sidecar, start.timestamp_millis(), truncated)?;
            std::fs::write(
                root.join(if truncated {
                    "partial-start.json"
                } else {
                    "delayed-start.json"
                }),
                serde_json::to_vec_pretty(&sidecar)?,
            )?;
            tracing::info!(
                truncated,
                "Delayed encounter recovered with expected footage coverage"
            );
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    anyhow::bail!(
        "delayed encounter did not produce playable footage: {}",
        *status.borrow()
    )
}

fn verify_delayed_coverage(
    sidecar: &serde_json::Value,
    start_ms: i64,
    truncated: bool,
) -> Result<()> {
    let plan: owc_domain::ClipPlan = serde_json::from_value(sidecar["plan"].clone())?;
    if truncated {
        anyhow::ensure!(
            plan.coverage == owc_domain::Coverage::Truncated,
            "partial coverage: {plan:?}"
        );
        anyhow::ensure!(
            plan.actual_start_ms.is_some_and(|at| at > start_ms),
            "invented missing history"
        );
        anyhow::ensure!(
            sidecar["activity"]["warnings"]
                .as_array()
                .is_some_and(|warnings| warnings.iter().any(|warning| warning
                    .as_str()
                    .is_some_and(|text| text.contains("only retained footage")))),
            "missing truncation warning"
        );
        anyhow::ensure!(
            sidecar["media"]["duration_ms"]
                .as_i64()
                .is_some_and(|ms| (5_000..15_000).contains(&ms)),
            "partial clip duration: {sidecar}"
        );
    } else {
        anyhow::ensure!(
            plan.coverage == owc_domain::Coverage::Complete,
            "incomplete retained history: {plan:?}"
        );
        anyhow::ensure!(
            plan.actual_start_ms.is_some_and(|at| at <= start_ms - 1000),
            "delayed clip lost retained pre-roll: {plan:?}"
        );
        anyhow::ensure!(
            plan.sources
                .iter()
                .map(|range| range.duration_ms)
                .sum::<i64>()
                >= 31_000,
            "delayed clip lost footage: {plan:?}"
        );
    }
    Ok(())
}
