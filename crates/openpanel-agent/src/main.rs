//! openpanel-agent — runs as a separate process for multi-host deployments.
//! For v0.1 it embeds the same handlers as the CLI's `serve` subcommand.

#![deny(rustdoc::broken_intra_doc_links)]

use openpanel_core::{Config, init_tracing};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    init_tracing(config.log().level.as_str());
    openpanel_cli::handlers::serve(config).await
}
