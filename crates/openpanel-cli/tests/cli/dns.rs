use super::common::CliRunner;

#[tokio::test]
async fn cli_dns_provider_zone_record_sync_check_and_delete_workflow() {
    let runner = CliRunner::new().await;
    let env = [("OPENPANEL__DNS__PROVIDER", "fake")];
    for args in [
        vec![
            "dns",
            "provider-add",
            "--kind",
            "fake",
            "--name",
            "Primary",
            "--credential",
            "cli-secret",
        ],
        vec!["dns", "providers"],
        vec!["dns", "provider-test"],
        vec![
            "dns",
            "provider-rotate",
            "--credential",
            "rotated-cli-secret",
        ],
        vec!["dns", "provider-disable"],
        vec!["dns", "provider-enable"],
        vec!["dns", "sync"],
        vec!["dns", "zones"],
        vec![
            "dns",
            "record-add",
            "--zone",
            "example.test",
            "--name",
            "www.example.test",
            "--kind",
            "A",
            "--value",
            "192.0.2.8",
            "--ttl",
            "300",
        ],
        vec!["dns", "records", "--zone", "example.test"],
        vec![
            "dns",
            "record-update",
            "--zone",
            "example.test",
            "--record",
            "www.example.test",
            "--value",
            "192.0.2.9",
            "--ttl",
            "300",
        ],
        vec!["dns", "check", "--zone", "example.test"],
        vec![
            "dns",
            "record-delete",
            "--zone",
            "example.test",
            "--record",
            "www.example.test",
            "--confirm",
        ],
        vec!["dns", "provider-delete", "--confirm"],
    ] {
        let result = runner.run_with_env(&args, &env);
        assert_eq!(result.code, 0, "{:?}: {}", args, result.stderr);
        assert!(!result.stdout.contains("cli-secret"));
        assert!(!result.stderr.contains("cli-secret"));
        assert!(!result.stdout.contains("rotated-cli-secret"));
        assert!(!result.stderr.contains("rotated-cli-secret"));
    }
}
