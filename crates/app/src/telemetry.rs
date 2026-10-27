use anyhow::Result;
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

fn console_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
}

#[cfg(not(feature = "profiling"))]
pub fn initialise() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(console_filter()))
        .try_init()?;
    Ok(())
}

#[cfg(feature = "profiling")]
static TRACE: std::sync::Mutex<Option<tracing_chrome::FlushGuard>> = std::sync::Mutex::new(None);

#[cfg(feature = "profiling")]
pub struct Trace;

#[cfg(feature = "profiling")]
impl Drop for Trace {
    fn drop(&mut self) {
        flush();
    }
}

#[cfg(feature = "profiling")]
pub fn initialise(path: Option<&std::path::Path>) -> Result<Trace> {
    let layer = if let Some(path) = path {
        // Create explicitly so an unwritable destination is reported rather than panicking.
        let file = std::fs::File::create(path)?;
        let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
            .writer(file)
            .include_args(false)
            .include_locations(false)
            .build();
        *TRACE
            .lock()
            .map_err(|_| anyhow::anyhow!("trace lock poisoned"))? = Some(guard);
        Some(
            layer.with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                metadata.is_span() && metadata.target() == "owc_profile"
            })),
        )
    } else {
        None
    };
    tracing_subscriber::registry()
        .with(layer)
        .with(tracing_subscriber::fmt::layer().with_filter(console_filter()))
        .try_init()?;
    Ok(Trace)
}

#[cfg(feature = "profiling")]
pub fn flush() {
    // Tao exits the process without unwinding main on some platforms.
    if let Ok(mut guard) = TRACE.lock() {
        drop(guard.take());
    }
}
