use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    version,
    about = "Local OBS companion for automatic World of Warcraft encounter recordings"
)]
struct Args {
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 0)]
    port: u16,
    /// Run without a desktop window or opening the browser.
    #[arg(long)]
    no_open: bool,
    /// Open in the default browser instead of the native window.
    #[arg(long)]
    browser: bool,
}
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    let root = args.data_dir.unwrap_or(
        directories_next::ProjectDirs::from("org", "obs-wow-companion", "OBS WoW Companion")
            .context("cannot locate application data directory; use --data-dir")?
            .data_local_dir()
            .to_owned(),
    );
    let runtime = tokio::runtime::Runtime::new()?;
    #[cfg(feature = "desktop")]
    if !args.no_open && !args.browser {
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let task = runtime.spawn(owc_app::serve(root, args.port, false, Some(ready_tx)));
        let url = ready_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .context("desktop service did not start; try --no-open for diagnostics")?;
        return desktop(&url, runtime, task);
    }
    runtime.block_on(owc_app::serve(root, args.port, !args.no_open, None))
}
#[cfg(feature = "desktop")]
#[derive(Debug)]
enum DesktopEvent {
    Quit,
    FolderOpened {
        error: Option<String>,
    },
    SavePath {
        recording: String,
        path: Option<PathBuf>,
    },
}
#[cfg(feature = "desktop")]
fn desktop(
    url: &str,
    runtime: tokio::runtime::Runtime,
    task: tokio::task::JoinHandle<Result<()>>,
) -> Result<()> {
    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        window::WindowBuilder,
    };
    let events = EventLoopBuilder::<DesktopEvent>::with_user_event().build();
    let proxy = events.create_proxy();
    runtime.spawn(async move {
        match task.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::error!(%error, "desktop service stopped with an error"),
            Err(error) => tracing::error!(%error, "desktop service task failed"),
        }
        let _ = proxy.send_event(DesktopEvent::Quit);
    });
    let window = WindowBuilder::new()
        .with_title("OBS WoW Companion")
        .with_inner_size(tao::dpi::LogicalSize::new(1360.0, 900.0))
        .build(&events)?;
    let base = url
        .split("/session/")
        .next()
        .context("local URL")?
        .to_owned();
    let quit_proxy = events.create_proxy();
    let dialog_runtime = runtime.handle().clone();
    let builder = wry::WebViewBuilder::new()
        .with_initialization_script("window.companionDesktop = true;")
        .with_ipc_handler(move |request| {
            if request.body() == "quit-accepted" {
                let _ = quit_proxy.send_event(DesktopEvent::Quit);
                return;
            }
            let Ok(message) = serde_json::from_str::<serde_json::Value>(request.body()) else {
                return;
            };
            if message["action"] == "open-containing-folder" {
                let path = PathBuf::from(message["path"].as_str().unwrap_or_default());
                let proxy = quit_proxy.clone();
                dialog_runtime.spawn(async move {
                    let error = owc_app::file_manager::open_containing_folder(&path)
                        .await
                        .err()
                        .map(|error| format!("{error:#}"));
                    let _ = proxy.send_event(DesktopEvent::FolderOpened { error });
                });
            }
            if message["action"] == "save-bundle-path" {
                choose_bundle_path(&message, &dialog_runtime, quit_proxy.clone());
            }
        })
        .with_url(url)
        .with_navigation_handler(move |next| next.starts_with(&format!("{base}/")));
    #[cfg(target_os = "linux")]
    let view = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(
            window
                .default_vbox()
                .context("desktop content container missing")?,
        )?
    };
    #[cfg(not(target_os = "linux"))]
    let view = builder.build(&window)?;
    events.run(move |event, _, control| {
        let _keep_alive = (&runtime, &view);
        *control = ControlFlow::Wait;
        if let Event::UserEvent(event) = event {
            match event {
                DesktopEvent::Quit => *control = ControlFlow::Exit,
                DesktopEvent::FolderOpened { error } => {
                    let payload = serde_json::json!({"error":error});
                    if let Err(error) = view.evaluate_script(&format!("window.folderOpened?.({payload})")) {
                        tracing::warn!(%error, "could not return folder status");
                    }
                }
                DesktopEvent::SavePath { recording, path } => {
                    let payload = serde_json::json!({"recording":recording,"path":path});
                    if let Err(error) = view.evaluate_script(&format!("window.bundlePathChosen?.({payload})")) {
                        tracing::warn!(%error, "could not return save path");
                    }
                }
            }
            return;
        }
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
            && let Err(error) = view.evaluate_script(
                "if (window.requestDesktopQuit) { window.requestDesktopQuit(); } else { window.pendingDesktopQuit = true; }",
            ) {
            tracing::error!(%error, "could not request desktop shutdown");
        }
    });
}

#[cfg(feature = "desktop")]
fn choose_bundle_path(
    message: &serde_json::Value,
    runtime: &tokio::runtime::Handle,
    proxy: tao::event_loop::EventLoopProxy<DesktopEvent>,
) {
    let recording = message["recording"].as_str().unwrap_or_default().to_owned();
    let path = PathBuf::from(message["path"].as_str().unwrap_or("recording.zip"));
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title("Save player bundle")
        .add_filter("Player bundle", &["zip"]);
    if let Some(parent) = path.parent().filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(parent);
    }
    if let Some(name) = path.file_name() {
        dialog = dialog.set_file_name(name.to_string_lossy());
    }
    let future = dialog.save_file();
    let proxy = proxy;
    runtime.spawn(async move {
        let path = future.await.map(|file| {
            let mut path = file.path().to_owned();
            if path.extension().is_none() {
                path.set_extension("zip");
            }
            path
        });
        let _ = proxy.send_event(DesktopEvent::SavePath { recording, path });
    });
}
