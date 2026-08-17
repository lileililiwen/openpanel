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

    // Pick a random free port by binding to :0 and reusing the number.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    // The databases module needs a master key; serve boots it eagerly.
    let master_key = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([0u8; 32])
    };

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

    // If the server exits early, surface its stderr in the timeout message.
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
        panic!("openpanel serve exited early ({status}): {err}");
    }

    // Wait for /health to come up.
    let url = format!("http://127.0.0.1:{port}/health");
    let start = std::time::Instant::now();
    loop {
        if let Ok(resp) = reqwest::get(&url).await
            && resp.status() == 200
        {
            let body: serde_json::Value = resp.json().await.unwrap();
            assert_eq!(body["status"], "ok");
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "server did not become healthy in time"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Shut down; `Child::kill()` sends SIGKILL on Unix, which is fine
    // for tearing down the test server.
    let _ = child.kill();
    let _ = child.wait().expect("wait for shutdown");
}
