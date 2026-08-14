//! `print-thresholds` — emit the effective file-length thresholds
//! resolved from env / file / defaults. Used by
//! `scripts/check-file-length.sh` so the shell and the Rust app
//! share one source of truth.
//!
//! Usage:
//!   print-thresholds                     → print all fields
//!   print-thresholds --json              → JSON
//!   print-thresholds --soft              → just the soft limit
//!   print-thresholds --hard              → just the hard limit
//!   print-thresholds --config <path>     → load from <path> (default: lint-extra.toml)

use std::path::PathBuf;

use clap::Parser;
use openpanel_core::FileLengthThresholds;

#[derive(Debug, Parser)]
#[command(
    name = "print-thresholds",
    about = "Print effective file-length thresholds"
)]
struct Args {
    /// Path to the lint-extra.toml file (defaults to `./lint-extra.toml`).
    #[arg(long, default_value = "lint-extra.toml")]
    config: PathBuf,
    /// Emit JSON instead of key=value lines.
    #[arg(long)]
    json: bool,
    /// Print only the soft limit.
    #[arg(long)]
    soft: bool,
    /// Print only the hard limit.
    #[arg(long)]
    hard: bool,
}

fn main() {
    let args = Args::parse();
    let thresholds = FileLengthThresholds::load(&args.config);

    if args.soft {
        println!("{}", thresholds.soft_limit);
        return;
    }
    if args.hard {
        println!("{}", thresholds.hard_limit);
        return;
    }
    if args.json {
        let json = serde_json::json!({
            "soft_limit": thresholds.soft_limit,
            "hard_limit": thresholds.hard_limit,
            "exclude": thresholds.exclude,
        });
        match serde_json::to_string_pretty(&json) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                eprintln!("print-thresholds: failed to serialise JSON: {err}");
                std::process::exit(1);
            }
        }
        return;
    }
    println!("soft_limit={}", thresholds.soft_limit);
    println!("hard_limit={}", thresholds.hard_limit);
    for entry in &thresholds.exclude {
        println!("exclude={entry}");
    }
}
