//! STD-001/STD-003: stdio transport, bounded framing, errors, notifications and public exports.
use crate::{
    ApplicationDeliveryService, ConvergeRegistry, DeploymentAdapterService, DockerBuildAdapter,
    DockerRuntimeAdapter, ForgeProviderAudit, ForgeProviderProcessHandle,
    ForgeProviderProcessStatus, ForgePublishProvider, ForgePublishProviderConfig, MAX_LINE_LEN,
    docker::testing::{FakeDocker, FakeNetwork},
    run_loop,
};
use openpanel_core::{
    SqliteAuditService,
    audit::{AuditService, NoopAuditService},
};
use openpanel_domain::{Email, Password, Role, User, Username};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

fn caller() -> Arc<User> {
    Arc::new(User::new(
        Uuid::new_v4(),
        Username::new("owner").unwrap(),
        Email::new("owner@example.com").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    ))
}
fn frame(operation: &str, id: Option<i64>) -> String {
    let mut v = json!({"jsonrpc":"2.0","method":"forge.publish","params":{
        "protocol":"forge-publish-provider/0.1.0","operation":operation,
        "project":{"slug":"acme/web"},"revision":{"kind":"git",
        "url":"https://example.com/acme/web.git","source_ref":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b",
        "release_digest":"sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08","environment":"production"},
        "operation_id":{"operation_id":Uuid::new_v4()},"secret_set":{"secret_set":"prod"},"dry_run":false}});
    if let Some(id) = id {
        v["id"] = json!(id);
    }
    v.to_string()
}
/// Compose the provider over the production docker adapters backed by
/// the shared docker double: the transport suite exercises the real
/// composition, not a fail-closed marker.
fn provider(audit: Arc<ForgeProviderAudit>) -> Arc<ForgePublishProvider> {
    let docker = Arc::new(FakeDocker::new());
    let runtime = Arc::new(
        DockerRuntimeAdapter::new(
            docker.clone(),
            Arc::new(FakeNetwork::new()),
            "registry.example/openpanel",
            Arc::new(ConvergeRegistry::new()),
        )
        .expect("valid runtime adapter"),
    );
    Arc::new(ForgePublishProvider::new(
        ForgePublishProviderConfig::default(),
        Arc::new(ApplicationDeliveryService::new(
            Arc::new(
                DockerBuildAdapter::new(docker, "registry.example/openpanel".into(), 1024 * 1024)
                    .expect("valid build adapter"),
            ),
            runtime,
            audit.clone(),
        )),
        Arc::new(DeploymentAdapterService::new(vec![], audit.clone())),
        audit,
    ))
}
async fn exchange(
    input: &[u8],
    audit: Arc<dyn AuditService>,
) -> (Vec<Value>, ForgeProviderProcessStatus) {
    let audit = Arc::new(ForgeProviderAudit::new(audit));
    let mut lifecycle = ForgeProviderProcessHandle::new(true);
    lifecycle.start_provider();
    let mut output = Vec::new();
    run_loop(
        input,
        &mut output,
        provider(audit.clone()),
        caller(),
        &mut lifecycle,
        audit,
    )
    .await
    .unwrap();
    assert!(output.is_empty() || output.last() == Some(&b'\n'));
    let text = String::from_utf8(output).unwrap();
    (
        text.lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect(),
        lifecycle.provider_status(),
    )
}
#[tokio::test]
async fn valid_publish_request_round_trips_and_clean_eof() {
    let (out, status) = exchange(
        format!("{}\n", frame("publish", Some(17))).as_bytes(),
        Arc::new(NoopAuditService),
    )
    .await;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["id"], 17);
    assert_eq!(out[0]["result"]["status"], "invalid_request");
    assert_eq!(status, ForgeProviderProcessStatus::Stopped);
}
#[tokio::test]
async fn garbage_line_does_not_kill_provider_and_errors_keep_ids() {
    let input = format!(
        "garbage\n{}\n{}\n{}\n",
        json!({"jsonrpc":"2.0","id":2,"method":"secret-value-do-not-echo","params":{}}),
        json!({"jsonrpc":"2.0","id":3,"method":"forge.publish","params":{"password":"secret-value-do-not-echo"}}),
        frame("capabilities", Some(4))
    );
    let (out, _) = exchange(input.as_bytes(), Arc::new(NoopAuditService)).await;
    assert_eq!(out.len(), 4);
    for (v, code) in out.iter().zip([-32700, -32601, -32602]) {
        assert_eq!(v["error"]["code"], code);
    }
    assert_eq!(out[1]["id"], 2);
    assert_eq!(out[2]["id"], 3);
    assert_eq!(out[3]["id"], 4);
    assert!(
        !serde_json::to_string(&out)
            .unwrap()
            .contains("secret-value-do-not-echo")
    );
}
#[tokio::test]
async fn notifications_dispatch_without_output_and_audit_once() {
    let db = openpanel_test_support::TestDb::new().await;
    let audit = Arc::new(SqliteAuditService::new(db.pool().clone()));
    audit.ensure_schema().await.unwrap();
    let (out, _) = exchange(
        format!("{}\n", frame("capabilities", None)).as_bytes(),
        audit.clone(),
    )
    .await;
    assert!(out.is_empty());
    let raw: Vec<(String, String)> = sqlx::query_as("SELECT action, actor FROM audit_log")
        .fetch_all(&db.pool())
        .await
        .unwrap();
    let events = audit.recent(100).await.unwrap();
    assert_eq!(events.len(), 1, "stored rows: {raw:?}");
    assert_eq!(events[0].actor, "forge-publish-provider");
    let (out, _) = exchange(
        format!("{}\n", frame("publish", Some(1))).as_bytes(),
        audit.clone(),
    )
    .await;
    assert_eq!(out.len(), 1);
    assert_eq!(audit.recent(100).await.unwrap().len(), 2);
}
#[tokio::test]
async fn oversized_line_preserves_next_request_and_invalid_utf8_recovers() {
    let mut input = vec![b'x'; MAX_LINE_LEN + 10];
    input.extend_from_slice(format!("\n{}\n", frame("capabilities", Some(9))).as_bytes());
    input.extend_from_slice(&[255, b'\n']);
    let (out, _) = exchange(&input, Arc::new(NoopAuditService)).await;
    assert_eq!(out.len(), 3);
    assert_eq!(out[0]["error"]["code"], -32602);
    assert_eq!(out[1]["id"], 9);
    assert_eq!(out[2]["error"]["code"], -32700);
}
#[test]
fn transport_composes_only_public_items() {
    let _: crate::ForgeProviderRequestEnvelope =
        serde_json::from_str(&frame("capabilities", Some(1))).unwrap();
    let v: Value = serde_json::from_str(&frame("capabilities", Some(1))).unwrap();
    crate::decode_envelope(&v["params"]).unwrap();
    assert!(
        crate::encode_error(&crate::JsonRpcParseError::Frame("invalid".into()), None)
            .unwrap()
            .contains("-32700")
    );
}

#[tokio::test]
async fn invalid_notifications_never_reply() {
    let input = format!(
        "{}\n{}\n",
        json!({"jsonrpc":"2.0","method":"unknown","params":{}}),
        json!({"jsonrpc":"2.0","method":"forge.publish","params":{}})
    );
    assert!(
        exchange(input.as_bytes(), Arc::new(NoopAuditService))
            .await
            .0
            .is_empty()
    );
}

struct BrokenWriter;
impl tokio::io::AsyncWrite for BrokenWriter {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "secret diagnostic",
        )))
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
#[tokio::test]
async fn output_failure_ends_loop_and_marks_failed() {
    let audit = Arc::new(ForgeProviderAudit::new(Arc::new(NoopAuditService)));
    let mut lifecycle = ForgeProviderProcessHandle::new(true);
    lifecycle.start_provider();
    let input = format!("{}\n", frame("capabilities", Some(1)));
    let err = run_loop(
        input.as_bytes(),
        BrokenWriter,
        provider(audit.clone()),
        caller(),
        &mut lifecycle,
        audit,
    )
    .await
    .unwrap_err();
    assert!(!err.to_string().contains("secret diagnostic"));
    assert!(!format!("{err:?}").contains("secret diagnostic"));
    assert!(std::error::Error::source(&err).is_none());
    assert_eq!(
        lifecycle.provider_status(),
        ForgeProviderProcessStatus::Failed
    );
}

struct BrokenAudit {
    panic: bool,
}
#[async_trait::async_trait]
impl AuditService for BrokenAudit {
    async fn record(&self, _: openpanel_core::audit::AuditEvent) -> openpanel_core::CoreResult<()> {
        if self.panic {
            panic!("fixture handler panic");
        }
        Err(openpanel_core::CoreError::Io(std::io::Error::other(
            "payload-secret-needle",
        )))
    }
    async fn recent(
        &self,
        _: i64,
    ) -> openpanel_core::CoreResult<Vec<openpanel_core::audit::AuditEvent>> {
        Ok(vec![])
    }
    async fn query(
        &self,
        _: openpanel_core::audit::AuditQuery,
    ) -> openpanel_core::CoreResult<openpanel_core::audit::AuditPage> {
        Ok(openpanel_core::audit::AuditPage {
            events: vec![],
            next_cursor: None,
        })
    }
}
#[tokio::test]
async fn handler_panic_and_audit_failure_are_internal_errors_and_loop_survives() {
    for panic in [false, true] {
        let input = format!(
            "{}\n{}\n",
            frame("capabilities", Some(1)),
            frame("capabilities", Some(2))
        );
        let (out, _) = exchange(input.as_bytes(), Arc::new(BrokenAudit { panic })).await;
        assert_eq!(out.len(), 2);
        for v in &out {
            assert_eq!(v["error"]["code"], -32603);
        }
        assert!(
            !serde_json::to_string(&out)
                .unwrap()
                .contains("payload-secret-needle")
        );
    }
}
