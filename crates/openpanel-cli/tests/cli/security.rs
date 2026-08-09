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
