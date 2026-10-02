//! STD-001/STD-002: real subprocess framing, config refusals and persisted audit.
use super::common::CliRunner;
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn command(runner: &CliRunner) -> Command {
    let mut cmd = Command::new(&runner.bin);
    cmd.arg("forge-provider")
        .envs(runner.env.iter().map(|(k, v)| (k, v)))
        .env_remove("OPENPANEL_FORGE_PUBLISH_PROVIDER")
        .env_remove("OPENPANEL_FORGE_PUBLISH_OPERATOR")
        .current_dir(&runner.workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}
fn request() -> String {
    json!({"jsonrpc":"2.0","id":17,"method":"forge.publish","params":{
        "protocol":"forge-publish-provider/0.1.0","operation":"publish","project":{"slug":"acme/web"},
        "revision":{"kind":"git","url":"https://example.com/acme/web.git","source_ref":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b",
        "release_digest":"sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08","environment":"production"},
        "operation_id":{"operation_id":uuid::Uuid::new_v4()},"secret_set":{"secret_set":"prod"},"dry_run":false}}).to_string()
}
async fn seed(runner: &CliRunner, role: &str) {
    let result = runner.run(&[
        "user",
        "create",
        "--username",
        "publisher",
        "--email",
        "publisher@example.com",
        "--password",
        "correct horse battery staple",
        "--role",
        role,
    ]);
    assert_eq!(result.code, 0, "{}", result.stderr);
}
#[tokio::test]
async fn cli_forge_provider_startup_refusals_never_read_stdin() {
    let runner = CliRunner::new().await;
    seed(&runner, "user").await;
    for (enable, operator) in [
        (None, None),
        (Some("garbage"), None),
        (Some("0"), None),
        (Some("1"), None),
        (Some("1"), Some("unknown")),
        (Some("1"), Some("publisher")),
    ] {
        let mut cmd = command(&runner);
        if let Some(v) = enable {
            cmd.env("OPENPANEL_FORGE_PUBLISH_PROVIDER", v);
        }
        if let Some(v) = operator {
            cmd.env("OPENPANEL_FORGE_PUBLISH_OPERATOR", v);
        }
        let mut child = cmd.spawn().unwrap();
        // Keep stdin open: startup must exit without waiting for EOF or a frame.
        let stdin = child.stdin.take().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() > deadline {
                child.kill().unwrap();
                panic!("startup consumed/waited on stdin");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        drop(stdin);
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(78));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("refusing startup"));
    }
    assert_eq!(runner.run(&["user", "list"]).code, 0);
}
#[tokio::test]
async fn cli_forge_provider_owner_and_admin_frames_and_audit() {
    for role in ["owner", "admin"] {
        let runner = CliRunner::new().await;
        seed(&runner, role).await;
        let mut child = command(&runner)
            .env("OPENPANEL_FORGE_PUBLISH_PROVIDER", "1")
            .env("OPENPANEL_FORGE_PUBLISH_OPERATOR", "publisher")
            .spawn()
            .unwrap();
        let input = format!(
            "garbage\n{}\n{}\n{}\n",
            json!({"jsonrpc":"2.0","id":2,"method":"payload-secret-needle","params":{}}),
            json!({"jsonrpc":"2.0","id":3,"method":"forge.publish","params":{"password":"payload-secret-needle"}}),
            request()
        );
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        let frames: Vec<Value> = stdout
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0]["error"]["code"], -32700);
        assert_eq!(frames[1]["error"]["code"], -32601);
        assert_eq!(frames[2]["error"]["code"], -32602);
        assert_eq!(frames[3]["id"], 17);
        assert_eq!(frames[3]["result"]["status"], "invalid_request");
        assert!(!stdout.contains("payload-secret-needle"));
        assert!(!stderr.contains("payload-secret-needle"));
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE actor = 'forge-publish-provider'",
        )
        .fetch_one(&runner.db.pool())
        .await
        .unwrap();
        assert_eq!(count, 1);
    }
}

/// The publish path against the double-backed harness (DCK-004).
///
/// How the double is injected: `openpanel_app::forge_publish_provider::testing::double_harness`
/// is a *test-only composition function* in the app crate. It builds the
/// exact production graph the `forge-provider` entry point builds —
/// `DockerBuildAdapter`, `DockerRuntimeAdapter`, `DockerDeploymentAdapter`,
/// `DeliveryTargetResolver` — but over the recording docker double instead
/// of a Bollard engine client. Nothing in the shipped binary reads a
/// "pretend to publish" switch, so the subprocess tests above keep proving
/// the real entry point; this test proves the frames the real transport
/// now answers once the `Unavailable` seam is gone.
#[tokio::test]
async fn cli_forge_provider_publish_frame_succeeds_against_the_double() {
    use openpanel_app::ForgeProviderProcessHandle;
    use openpanel_core::audit::NoopAuditService;

    let docker = std::sync::Arc::new(openpanel_app::docker::testing::FakeDocker::new());
    let caller = openpanel_app::forge_publish_provider::testing::noop_operator();
    let harness = openpanel_app::forge_publish_provider::testing::double_harness(
        caller.clone(),
        docker.clone(),
        "web",
        openpanel_app::forge_publish_provider::testing::HARNESS_DIGEST,
    );
    let input = format!(
        "{}\n{}\n",
        frame("preflight", true),
        frame("publish", false),
    );
    let mut output = Vec::new();
    let audit = std::sync::Arc::new(openpanel_app::ForgeProviderAudit::new(std::sync::Arc::new(
        NoopAuditService,
    )));
    let mut lifecycle = ForgeProviderProcessHandle::new(true);
    lifecycle.start_provider();
    openpanel_app::run_loop(
        tokio::io::BufReader::new(std::io::Cursor::new(input.into_bytes())),
        &mut output,
        harness.provider.clone(),
        std::sync::Arc::new(caller),
        &mut lifecycle,
        audit,
    )
    .await
    .expect("transport loop completes");
    let stdout = String::from_utf8(output).expect("utf8 frames");
    let frames: Vec<Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("json frame"))
        .collect();
    assert_eq!(frames.len(), 2, "one response per frame: {stdout}");
    assert_eq!(frames[0]["result"]["status"], "dry_run");
    assert_eq!(
        frames[1]["result"]["status"], "succeeded",
        "publish frame: {stdout}"
    );
    assert_eq!(
        frames[1]["result"]["terminal"]["release_digest"],
        openpanel_app::forge_publish_provider::testing::HARNESS_DIGEST
    );
    assert!(
        docker
            .calls()
            .iter()
            .any(|call| call.starts_with("create_labeled")),
        "publish converged a labeled container: {:?}",
        docker.calls()
    );
    assert!(!stdout.contains("password"), "{stdout}");
}

/// A forge frame for the harness application, addressed by the slug
/// the harness registered.
fn frame(operation: &str, dry_run: bool) -> String {
    let namespace = openpanel_app::forge_publish_provider::testing::HARNESS_REGISTRY_NAMESPACE;
    json!({"jsonrpc":"2.0","id":1,"method":"forge.publish","params":{
        "protocol":"forge-publish-provider/0.1.0","operation":operation,
        "project":{"slug":"acme/web"},
        "revision":{"kind":"oci","url":format!("oci://{namespace}/web"),
            "source_ref":openpanel_app::forge_publish_provider::testing::HARNESS_DIGEST,
            "release_digest":openpanel_app::forge_publish_provider::testing::HARNESS_DIGEST,
            "environment":"production"},
        "operation_id":{"operation_id":uuid::Uuid::new_v4()},
        "secret_set":{"secret_set":"prod-secrets"},"dry_run":dry_run}})
    .to_string()
}
