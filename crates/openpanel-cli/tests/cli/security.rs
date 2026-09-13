use super::common::CliRunner;

#[tokio::test]
async fn cli_security_status_rule_preview_apply_rollback_and_blocks() {
    let runner = CliRunner::new().await;
    let env = [("OPENPANEL__SECURITY__NFT_BINARY", "/bin/true")];
    let status = runner.run_with_env(&["security", "status"], &env);
    assert_eq!(status.code, 0, "{}", status.stderr);
    let add = runner.run_with_env(
        &[
            "security",
            "rule",
            "add",
            "--protocol",
            "tcp",
            "--port",
            "443",
            "--source",
            "0.0.0.0/0",
            "--action",
            "allow",
            "--comment",
            "HTTPS",
        ],
        &env,
    );
    assert_eq!(add.code, 0, "{}", add.stderr);
    let id = add.stdout.split_whitespace().last().unwrap();
    for args in [
        vec!["security", "rule", "list"],
        vec![
            "security",
            "rule",
            "update",
            "--id",
            id,
            "--comment",
            "updated",
        ],
        vec!["security", "rule", "disable", "--id", id],
        vec!["security", "rule", "enable", "--id", id],
        vec!["security", "preview"],
        vec!["security", "apply"],
        vec!["security", "rollback"],
        vec!["security", "blocks"],
        vec!["security", "allowlist", "list"],
        vec!["security", "allowlist", "add", "--network", "192.0.2.0/24"],
        vec![
            "security",
            "allowlist",
            "delete",
            "--network",
            "192.0.2.0/24",
        ],
        vec!["security", "unblock", "--key", "ip:192.0.2.1"],
    ] {
        let result = runner.run_with_env(&args, &env);
        assert_eq!(result.code, 0, "{:?}: {}", args, result.stderr);
    }
    assert_eq!(
        runner
            .run_with_env(&["security", "rule", "delete", "--id", id], &env)
            .code,
        0
    );
}

/// Capability under test: `operator-security-control-plane` (CLI queue,
/// show, preview, suppress, remediate wiring; per-process seed).
#[tokio::test]
async fn cli_security_findings_queue_and_triage_wiring() {
    let runner = CliRunner::new().await;
    // Fresh DB seeds an empty queue; the command still exits zero.
    let queue = runner.run(&["security", "findings", "queue"]);
    assert_eq!(queue.code, 0, "{}", queue.stderr);

    let missing = "00000000-0000-0000-0000-000000000000";
    for args in [
        vec!["security", "findings", "show", "--id", missing],
        vec!["security", "findings", "preview", "--id", missing],
    ] {
        let result = runner.run(&args);
        assert_ne!(result.code, 0, "{args:?} must fail for unknown id");
    }
    let bad_id = runner.run(&["security", "findings", "show", "--id", "nope"]);
    assert_ne!(bad_id.code, 0, "malformed id is rejected");

    let suppress = runner.run(&[
        "security",
        "findings",
        "suppress",
        "--id",
        missing,
        "--reason",
        "noise",
        "--scope",
        "firewall:rules",
    ]);
    assert_ne!(suppress.code, 0, "suppress of unknown id fails");

    let remediate = runner.run(&[
        "security",
        "findings",
        "remediate",
        "--id",
        missing,
        "--idempotency-key",
        "key-1",
        "--confirm",
    ]);
    assert_ne!(remediate.code, 0, "remediate of unknown id fails");
}
