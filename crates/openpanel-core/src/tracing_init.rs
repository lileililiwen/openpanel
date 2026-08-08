//! Initialize tracing with an `EnvFilter` honoring `RUST_LOG` and the
//! `log.level` config field.

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the global tracing subscriber using `RUST_LOG` (falling back to
/// the given `level` for openpanel/tower_http/axum). Safe to call multiple
/// times; subsequent calls are ignored.
pub fn init_tracing(level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!("openpanel={level},tower_http={level},axum={level}"))
    });

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true));

    if subscriber.try_init().is_err() {
        // Subscriber already set (e.g. by tests); safe to ignore.
    }
}
