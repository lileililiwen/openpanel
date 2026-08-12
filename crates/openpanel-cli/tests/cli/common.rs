//! Shared helpers for CLI E2E tests.
//!
//! Each test spawns a fresh `openpanel` binary against a fresh SQLite
//! DB file in a per-test temp directory. The tests are order-independent
//! and parallel-safe (each gets its own DB and port).

use std::{
    path::PathBuf,
    process::{Command, Output},
};

use openpanel_test_support::TestDb;

pub struct CliRunner {
    pub bin: PathBuf,
    pub env: Vec<(String, String)>,
    pub workdir: PathBuf,
    /// Kept alive for its `Drop` cleanup of the per-test DB file.
    pub db: TestDb,
}

pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl CliRunner {
    pub async fn new() -> Self {
        let bin = PathBuf::from(env!("CARGO_BIN_EXE_openpanel"));
        let db = TestDb::new().await;
        let workdir =
            std::env::temp_dir().join(format!("openpanel-cli-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&workdir).expect("create workdir");

        let env = vec![
            ("OPENPANEL__DATABASE__URL".into(), db.url()),
            (
                "OPENPANEL__DATABASE__MASTER_KEY".into(),
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0u8; 32]),
            ),
            ("RUST_LOG".into(), "warn".into()),
        ];

        Self {
            bin,
            env,
            workdir,
            db,
        }
    }

    pub fn run(&self, args: &[&str]) -> RunResult {
        self.run_with_env(args, &[])
    }

    pub fn run_with_env(&self, args: &[&str], extra_env: &[(&str, &str)]) -> RunResult {
        let mut cmd = Command::new(&self.bin);
        cmd.args(args)
            .envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .envs(extra_env.iter().map(|(k, v)| (*k, *v)))
            .current_dir(&self.workdir);

        let output: Output = match cmd.output() {
            Ok(o) => o,
            Err(e) => panic!("failed to spawn openpanel: {e}"),
        };

        RunResult {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code: output.status.code().unwrap_or(-1),
        }
    }
}

impl Drop for CliRunner {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.workdir);
    }
}
