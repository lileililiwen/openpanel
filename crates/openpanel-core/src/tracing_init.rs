//! Initialize tracing with an `EnvFilter` honoring `RUST_LOG` and the
//! `log.level` config field.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub fn init_tracing(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("openpanel={level},tower_http={level},axum={level}")));

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true));

    if subscriber.try_init().is_err() {
        // Subscriber already set (e.g. by tests); safe to ignore.
    }
}