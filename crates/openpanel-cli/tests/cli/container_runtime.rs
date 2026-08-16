//! Container-runtime CLI E2E: quota show/set, metrics, registry
//! credentials, egress limit. Runs against the local daemon.

use super::common::CliRunner;

fn owner(runner: &CliRunner) {
    let user = runner.run(&[
        "user",
        "create",
        "--username",
        "owner",
        "--email",
        "owner@example.test",
        "--password",
        "correct horse battery staple",
        "--role",
        "owner",
    ]);
    assert_eq!(user.code, 0, "{}", user.stderr);
}

#[tokio::test]
async fn cli_container_quota_show_set_and_egress_limit() {
    let runner = CliRunner::new().await;
    owner(&runner);

    let shown = runner.run(&["container-runtime", "quota-show"]);
    assert_eq!(shown.code, 0, "{}", shown.stderr);
    let parsed: serde_json::Value = serde_json::from_str(&shown.stdout).expect("json");
    assert_eq!(parsed["quota"]["max_concurrent"], 8);

    let set = runner.run(&[
        "container-runtime",
        "quota-set",
        "--max-concurrent",
        "2",
        "--max-total",
        "2",
    ]);
    assert_eq!(set.code, 0, "{}", set.stderr);

    let reshown = runner.run(&["container-runtime", "quota-show"]);
    assert_eq!(reshown.code, 0, "{}", reshown.stderr);
    let reparsed: serde_json::Value = serde_json::from_str(&reshown.stdout).expect("json");
    assert_eq!(reparsed["quota"]["max_concurrent"], 2);
    assert_eq!(reparsed["quota"]["max_total"], 2);

    let raised = runner.run(&[
        "container-runtime",
        "raise-egress-limit",
        "--bytes-per-month",
        "2000000000",
    ]);
    assert_eq!(raised.code, 0, "{}", raised.stderr);
}

#[tokio::test]
async fn cli_container_registry_credentials_lifecycle() {
    let runner = CliRunner::new().await;
    owner(&runner);

    let added = runner.run(&[
        "container-runtime",
        "registry-credential-add",
        "--registry",
        "registry.example.com",
        "--user",
        "robot",
        "--password",
        "aB1!very-long-password",
    ]);
    assert_eq!(added.code, 0, "{}", added.stderr);
    assert!(
        added.stdout.contains("aB1!very-long-password"),
        "plaintext returned exactly once on add: {}",
        added.stdout
    );

    let listed = runner.run(&["container-runtime", "registry-credential-list"]);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert!(listed.stdout.contains("registry.example.com"));
    assert!(
        !listed.stdout.contains("aB1!very-long-password"),
        "plaintext must never be echoed on list"
    );

    let parsed: serde_json::Value = serde_json::from_str(&listed.stdout).expect("json");
    let id = parsed[0]["id"].as_str().expect("id").to_owned();
    let removed = runner.run(&["container-runtime", "registry-credential-remove", &id]);
    assert_eq!(removed.code, 0, "{}", removed.stderr);
}
