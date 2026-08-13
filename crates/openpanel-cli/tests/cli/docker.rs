//! Docker CLI lifecycle against the local daemon.

use super::common::CliRunner;

#[tokio::test]
async fn cli_docker_pull_create_start_logs_exec_restart_stop_and_remove() {
    let runner = CliRunner::new().await;
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
    let allow = runner.run(&["docker", "allow", "--pattern", "redis:*"]);
    assert_eq!(allow.code, 0, "{}", allow.stderr);
    let pull = runner.run(&["docker", "pull", "--image", "redis:7.0-alpine"]);
    assert_eq!(pull.code, 0, "{}", pull.stderr);
    let id = uuid::Uuid::new_v4();
    let spec = serde_json::json!({
        "id":id,"name":format!("redis_{}",&id.simple().to_string()[..8]),"image":"redis:7.0-alpine","site_id":id,
        "env":{},"ports":[],"mounts":[],"capabilities":[],"limits":{"cpu_shares":128,"mem_mb":64,"pids_max":64},
        "restart_policy":"no","user_namespace":"999:999"
    }).to_string();
    let create = runner.run(&["docker", "create", "--spec-json", &spec]);
    assert_eq!(create.code, 0, "{}", create.stderr);
    let start = runner.run(&["docker", "start", "--id", &id.to_string()]);
    assert_eq!(start.code, 0, "{}", start.stderr);
    let inspect = runner.run(&["docker", "inspect", "--id", &id.to_string()]);
    assert_eq!(inspect.code, 0, "{}", inspect.stderr);
    assert!(inspect.stdout.contains("running"));
    let logs = runner.run(&["docker", "logs", "--id", &id.to_string(), "--tail", "20"]);
    assert_eq!(logs.code, 0, "{}", logs.stderr);
    let exec = runner.run(&[
        "docker",
        "exec",
        "--id",
        &id.to_string(),
        "--command",
        "redis-cli",
        "--command",
        "ping",
    ]);
    assert_eq!(exec.code, 0, "{}", exec.stderr);
    assert!(exec.stdout.contains("PONG"));
    for action in ["restart", "stop"] {
        let result = runner.run(&["docker", action, "--id", &id.to_string()]);
        assert_eq!(result.code, 0, "{action}: {}", result.stderr);
    }
    let remove = runner.run(&["docker", "rm", "--id", &id.to_string()]);
    assert_eq!(remove.code, 0, "{}", remove.stderr);
}

#[tokio::test]
async fn cli_docker_memory_ceiling_reports_oom_and_keeps_logs_bounded() {
    let runner = CliRunner::new().await;
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
    assert_eq!(
        runner
            .run(&["docker", "allow", "--pattern", "redis:*"])
            .code,
        0
    );
    let pull = runner.run(&["docker", "pull", "--image", "redis:7.0-alpine"]);
    assert_eq!(pull.code, 0, "{}", pull.stderr);
    let id = uuid::Uuid::new_v4();
    let spec = serde_json::json!({
        "id":id,"name":format!("oom_{}",&id.simple().to_string()[..8]),"image":"redis:7.0-alpine","site_id":id,
        "env":{},"ports":[],"mounts":[],"capabilities":[],
        "limits":{"cpu_shares":128,"mem_mb":16,"pids_max":64},"restart_policy":"no","user_namespace":"999:999"
    })
    .to_string();
    let create = runner.run(&["docker", "create", "--spec-json", &spec]);
    assert_eq!(create.code, 0, "{}", create.stderr);
    let start = runner.run(&["docker", "start", "--id", &id.to_string()]);
    assert_eq!(start.code, 0, "{}", start.stderr);
    let _ = runner.run(&[
        "docker",
        "exec",
        "--id",
        &id.to_string(),
        "--command",
        "redis-cli",
        "--command",
        "EVAL",
        "--command",
        "local t={}; for i=1,1000000 do t[i]=string.rep('x',1024) end; return #t",
        "--command",
        "0",
    ]);

    let mut inspected = String::new();
    for _ in 0..20 {
        let result = runner.run(&["docker", "inspect", "--id", &id.to_string()]);
        assert_eq!(result.code, 0, "{}", result.stderr);
        inspected = result.stdout;
        if inspected.contains("\"oom_killed\": true") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let logs = runner.run(&["docker", "logs", "--id", &id.to_string(), "--tail", "20"]);
    let remove = runner.run(&["docker", "rm", "--id", &id.to_string(), "--force"]);
    assert_eq!(remove.code, 0, "{}", remove.stderr);
    assert!(inspected.contains("\"oom_killed\": true"), "{inspected}");
    assert_eq!(logs.code, 0, "{}", logs.stderr);
    assert!(logs.stdout.len() <= 20 * 4096);
}
