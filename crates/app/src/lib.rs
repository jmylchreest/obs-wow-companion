//! Local application service. All mutations are serialized by the engine actor.
pub mod autostart;
pub mod config;
mod credentials;
pub mod engine;
mod event_log;
mod events;
pub mod file_manager;
mod installations;
mod launcher_paths;
mod log_watcher;
pub mod readiness;
mod sharing;
mod sidecar;
use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{Request as HttpRequest, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use engine::{Engine, Request};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio::sync::{mpsc, oneshot, watch};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

#[derive(Clone)]
struct Web {
    tx: mpsc::Sender<Request>,
    status: watch::Receiver<Value>,
    root: PathBuf,
    token: String,
    host: String,
    shutdown: watch::Sender<bool>,
}
async fn call(web: &Web, data: Value) -> Result<Value> {
    let (reply, rx) = oneshot::channel();
    web.tx
        .send(Request { data, reply })
        .await
        .context("service stopped")?;
    tokio::time::timeout(Duration::from_secs(30), rx)
        .await
        .context("operation still running; check status before retrying")?
        .context("service stopped")?
}
async fn authorize(State(web): State<Web>, req: HttpRequest<Body>, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok());
    if host != Some(web.host.as_str()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Some(origin) = req.headers().get(header::ORIGIN)
        && origin.to_str().ok() != Some(format!("http://{}", web.host).as_str())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let cookie = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !cookie
        .split(';')
        .any(|v| v.trim() == format!("owc_session={}", web.token))
    {
        return (
            StatusCode::UNAUTHORIZED,
            "Open the private session link from the application launcher.",
        )
            .into_response();
    }
    if req.method() != axum::http::Method::GET
        && req
            .headers()
            .get("x-owc-request")
            .and_then(|v| v.to_str().ok())
            != Some("1")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().expect("constant"));
    response.headers_mut().insert(
        "x-content-type-options",
        "nosniff".parse().expect("constant"),
    );
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().expect("constant"));
    response.headers_mut().insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self'; media-src 'self' blob:; img-src 'self' data:; frame-ancestors 'none'".parse().expect("constant"));
    response
}
async fn login(State(web): State<Web>, Path(token): Path<String>) -> Response {
    if token != web.token {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        format!(
            "owc_session={}; HttpOnly; SameSite=Strict; Path=/",
            web.token
        )
        .parse()
        .expect("UUID cookie"),
    );
    response
}
async fn event_stream(
    State(web): State<Web>,
) -> axum::response::Sse<
    impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    let stream = futures_util::stream::unfold(
        (web.status, web.shutdown.subscribe(), true),
        |(mut status, mut shutdown, first)| async move {
            if *shutdown.borrow() {
                return None;
            }
            if !first {
                tokio::select! {
                    biased;
                    _ = shutdown.changed() => return None,
                    changed = status.changed() => { if changed.is_err() {return None;} }
                }
            }
            let data = status.borrow_and_update().to_string();
            Some((
                Ok(axum::response::sse::Event::default()
                    .event("state")
                    .data(data)),
                (status, shutdown, false),
            ))
        },
    );
    axum::response::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}
async fn action(State(web): State<Web>, Json(v): Json<Value>) -> Response {
    if v["action"]
        .as_str()
        .is_none_or(|a| a.starts_with('_') || a == "media_path")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if v["action"] == "quit" {
        if let Err(error) = call(&web, json!({"action":"shutdown"})).await {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":format!("{error:#}")})),
            )
                .into_response();
        }
        let mut status = web.status.clone();
        // Wait for owned OBS closure and cancellation cleanup, never for full encoding.
        let settled = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if status.borrow_and_update()["shutdown_ready"] == true {
                    break;
                }
                if status.changed().await.is_err() {
                    break;
                }
            }
        })
        .await;
        if settled.is_err() {
            tracing::warn!("Shutdown deadline reached; durable jobs will recover on restart");
        }
        web.shutdown.send_replace(true);
        return Json(json!({"ok":true})).into_response();
    }
    match call(&web, v).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":format!("{e:#}")})),
        )
            .into_response(),
    }
}
async fn media(State(web): State<Web>, Path(id): Path<String>, req: HttpRequest<Body>) -> Response {
    if Uuid::parse_str(&id).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    match call(&web, json!({"action":"media_path","id":id})).await {
        Ok(v) => match v["path"].as_str() {
            Some(p) => ServeFile::new(p).oneshot(req).await.map_or_else(
                |_| StatusCode::NOT_FOUND.into_response(),
                IntoResponse::into_response,
            ),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
#[allow(clippy::case_sensitive_file_extension_comparisons)] // Only generated lowercase preview names are served.
async fn preview(
    State(web): State<Web>,
    Path(file): Path<String>,
    req: HttpRequest<Body>,
) -> Response {
    if file.contains('/') || file.contains('\\') || file.contains("..") || !file.ends_with(".mp4") {
        return StatusCode::NOT_FOUND.into_response();
    }
    ServeFile::new(web.root.join("preview").join(file))
        .oneshot(req)
        .await
        .map_or_else(
            |_| StatusCode::NOT_FOUND.into_response(),
            IntoResponse::into_response,
        )
}
fn ui_routes() -> Router<Web> {
    let mut router = Router::new().route(
        "/",
        get(|| async { Html(include_str!("../ui/index.html")) }),
    );
    router = router.route(
        "/vendor/videojs.woff",
        get(|| async {
            (
                [(header::CONTENT_TYPE, "font/woff")],
                include_bytes!("../ui/vendor/videojs.woff").as_slice(),
            )
        }),
    );
    for (path, content_type, source) in [
        (
            "/vendor/video.min.js",
            "text/javascript",
            include_str!("../ui/vendor/video.min.js"),
        ),
        (
            "/vendor/video-js.min.css",
            "text/css",
            include_str!("../ui/vendor/video-js.min.css"),
        ),
        (
            "/layout.js",
            "application/javascript",
            include_str!("../ui/layout.js"),
        ),
        (
            "/player.js",
            "text/javascript",
            include_str!("../ui/player.js"),
        ),
        (
            "/review.js",
            "text/javascript",
            include_str!("../ui/review.js"),
        ),
        (
            "/diagnostics.js",
            "text/javascript",
            include_str!("../ui/diagnostics.js"),
        ),
        (
            "/date-range.js",
            "text/javascript",
            include_str!("../ui/date-range.js"),
        ),
        ("/app.js", "text/javascript", include_str!("../ui/app.js")),
        (
            "/setup.js",
            "text/javascript",
            include_str!("../ui/setup.js"),
        ),
        ("/style.css", "text/css", include_str!("../ui/style.css")),
    ] {
        router = router.route(
            path,
            get(move || async move { ([(header::CONTENT_TYPE, content_type)], source) }),
        );
    }
    router
}

/// Runs a loopback-only service. The UI is bundled; no external frontend or CDN is required.
#[allow(clippy::literal_string_with_formatting_args)] // Axum route parameters use braces.
pub async fn serve(
    root: PathBuf,
    port: u16,
    open: bool,
    ready: Option<std::sync::mpsc::SyncSender<String>>,
) -> Result<()> {
    std::fs::create_dir_all(&root)?;
    let root = root
        .canonicalize()
        .context("resolve application data directory")?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("instance.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .context("another companion is already using this data directory")?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let host = listener.local_addr()?.to_string();
    let token = Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel(32);
    let (status_tx, status) = watch::channel(json!({"message":"Starting"}));
    let engine = Engine::new(root.clone(), tx.clone(), status_tx.clone())?;
    // The desktop can request state before OBS reconnect finishes. Publish the
    // full schema before exposing the server, rather than a partial placeholder.
    status_tx.send_replace(engine.snapshot()?);
    tokio::spawn(engine.run(rx));
    let (shutdown, mut shutdown_rx) = watch::channel(false);
    let shutdown_signal = shutdown.clone();
    let web = Web {
        tx,
        status,
        root,
        token: token.clone(),
        host: host.clone(),
        shutdown,
    };
    let protected = ui_routes()
        .route(
            "/api/status",
            get(|State(w): State<Web>| async move { Json(w.status.borrow().clone()) }),
        )
        .route("/api/events", get(event_stream))
        .route("/api/action", post(action))
        .route("/media/{id}", get(media))
        .route("/preview/{file}", get(preview))
        .layer(middleware::from_fn_with_state(web.clone(), authorize));
    let app = Router::new()
        .route("/session/{token}", get(login))
        .merge(protected)
        .layer(axum::extract::DefaultBodyLimit::max(128 * 1024))
        .with_state(web);
    let url = format!("http://{host}/session/{token}");
    tracing::info!(url=%url,"Open this local private session URL");
    if open {
        open_browser(&url)?;
    }
    if let Some(ready) = ready {
        let _ = ready.send(url);
    }
    let mut draining = shutdown_signal.subscribe();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = shutdown_rx.changed() => {} }
        shutdown_signal.send_replace(true);
    });
    let server = std::future::IntoFuture::into_future(server);
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result?,
        _ = draining.changed() => {
            // A stalled browser/media connection must not keep the desktop alive.
            if let Ok(result) = tokio::time::timeout(std::time::Duration::from_secs(2), &mut server).await {
                result?;
            } else {
                tracing::info!("closing remaining HTTP connections during shutdown");
            }
        }
    }
    drop(lock);
    Ok(())
}
pub fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    Ok(())
}
