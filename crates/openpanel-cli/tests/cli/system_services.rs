use super::common::CliRunner;

#[tokio::test]
async fn cli_services_inventory_actions_history_and_logs() {
    let runner = CliRunner::new().await;
    let env = [("OPENPANEL__SERVICES__CONTROLLER", "/bin/true")];
    for args in [
        vec!["services", "list"],
        vec!["services", "status", "--id", "nginx"],
        vec![
            "services", "preview", "--id", "nginx", "--action", "restart",
        ],
        vec!["services", "start", "--id", "nginx"],
        vec!["services", "stop", "--id", "nginx", "--confirm"],
        vec!["services", "restart", "--id", "nginx", "--confirm"],
        vec!["services", "reload", "--id", "nginx"],
        vec!["services", "enable", "--id", "nginx"],
        vec!["services", "disable", "--id", "nginx"],
        vec!["services", "history", "--id", "nginx"],
        vec!["services", "logs", "--id", "nginx", "--limit", "20"],
    ] {
        let result = runner.run_with_env(&args, &env);
        assert_eq!(result.code, 0, "{:?}: {}", args, result.stderr);
    }
}
