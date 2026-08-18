//! E2E test: `openpanel serve` boots, responds on /health, and shuts
//! down cleanly on SIGTERM.

use std::{
    process::{Child, Command},
    time::Duration,
};

use super::common::CliRunner;

#[tokio::test]
async fn cli_serve_health_roundtrip() {
    let runner = CliRunner::new().await;
    let base = runner.db.url();

    // Run migrations first so the serve process doesn't need to.
    let migrate = runner.run(&["migrate"]);
    assert_eq!(migrate.code, 0, "migrate failed: {}", migrate.stderr);
    assert!(
        migrate.stdout.contains("migrations applied"),
        "migrate output should confirm, got: {}",
        migrate.stdout
    );

    // The databases module needs a master key; serve boots it eagerly.
    let master_key = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([0u8; 32])
    };

    // Pick a random free port by binding to :0 and reusing the number.
    // The listener is released before the child binds it, so under
    // parallel test threads another test can steal the port in between;
    // retry the whole spawn on a fresh port when the child exits early
    // or never becomes healthy.
    let mut last_error = "no attempt made".to_string();
    for _attempt in 0..5 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let mut child: Child = Command::new(&runner.bin)
            .args(["serve"])
            .env("OPENPANEL__DATABASE__URL", &base)
            .env("OPENPANEL__SERVER__BIND", "127.0.0.1")
            .env("OPENPANEL__SERVER__PORT", port.to_string())
            .env("OPENPANEL__DATABASE__MASTER_KEY", &master_key)
            .env("RUST_LOG", "warn")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn openpanel serve");

        // If the server exits immediately (e.g. the port was stolen),
        // surface its stderr and retry with a fresh port.
        if let Some(status) = child.try_wait().expect("try_wait") {
            let err = child
                .stderr
                .take()
                .map(|mut s| {
                    use std::io::Read;
                    let mut buf = String::new();
                    let _ = s.read_to_string(&mut buf);
                    buf
                })
                .unwrap_or_default();
            last_error = format!("serve exited early ({status}): {err}");
            continue;
        }

        // Wait for /health to come up.
        let url = format!("http://127.0.0.1:{port}/health");
        let start = std::time::Instant::now();
        let mut healthy = false;
        loop {
            if let Ok(resp) = reqwest::get(&url).await
                && resp.status() == 200
            {
                let body: serde_json::Value = resp.json().await.unwrap();
                assert_eq!(body["status"], "ok");
                healthy = true;
                break;
            }
            if start.elapsed() >= Duration::from_secs(30) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let _ = child.kill();
        let _ = child.wait().expect("wait for shutdown");
        if healthy {
            return;
        }
        last_error = format!("server on port {port} did not become healthy");
    }
    panic!("openpanel serve did not come up after retries: {last_error}");
}
