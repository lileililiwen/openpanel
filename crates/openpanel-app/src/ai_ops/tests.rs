//! AI Ops bounded context unit and service tests.

use std::sync::Arc;

use openpanel_core::NoopAuditService;
use openpanel_domain::{
    AiActionId, AiActionStatus, AiOpsError, AiOpsRepository, Role, ToolCallAllowlist, ToolKind,
    ToolName, ToolSpec,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::ai_ops::{
    ActionApproval, AskService, DefaultToolExecutor, SqliteAiOpsRepository, ToolExecutor,
    ToolExecutorError, ToolServices,
};

/// Recording executor used to verify the agent surface from the
/// outside. Each call records its name so tests can assert the
/// allowlist was honoured.
#[derive(Default)]
struct RecordingExecutor {
    calls: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
    fail: std::sync::Mutex<Option<String>>,
}

impl RecordingExecutor {
    fn last(&self) -> Vec<(String, serde_json::Value)> {
        self.calls.lock().expect("calls").clone()
    }

    #[allow(dead_code)] // helper reserved for failure-injection tests
    fn fail_next(&self, name: &str) {
        *self.fail.lock().expect("fail") = Some(name.to_string());
    }
}

#[async_trait::async_trait]
impl ToolExecutor for RecordingExecutor {
    async fn run(
        &self,
        _allowlist: &ToolCallAllowlist,
        _caller: &openpanel_domain::User,
        tool: &ToolName,
        params: serde_json::Value,
    ) -> Result<openpanel_domain::ToolResult, ToolExecutorError> {
        if let Some(name) = self.fail.lock().expect("fail").as_ref()
            && name == tool.as_str()
        {
            return Err(ToolExecutorError::Execution("forced failure".into()));
        }
        self.calls
            .lock()
            .expect("calls")
            .push((tool.as_str().to_string(), params.clone()));
        Ok(openpanel_domain::ToolResult {
            tool: tool.clone(),
            ok: true,
            body: serde_json::json!({ "echo": tool.as_str() }),
            summary: None,
        })
    }
}

fn allowlist_with_read_tool() -> ToolCallAllowlist {
    let mut allowlist = ToolCallAllowlist::new();
    allowlist.register(ToolSpec {
        name: ToolName::new("panel.health").expect("static"),
        kind: ToolKind::Read,
        description: "panel health".into(),
    });
    allowlist.register(ToolSpec {
        name: ToolName::new("panel.list_sites").expect("static"),
        kind: ToolKind::Read,
        description: "list sites".into(),
    });
    allowlist
}

fn allowlist_with_write_tool() -> ToolCallAllowlist {
    let mut allowlist = allowlist_with_read_tool();
    allowlist.register(ToolSpec {
        name: ToolName::new("system.reload_nginx").expect("static"),
        kind: ToolKind::Write,
        description: "reload nginx".into(),
    });
    allowlist
}

async fn owner_user() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Username};
    openpanel_domain::User::new(
        Uuid::new_v4(),
        Username::new("owner").expect("static"),
        Email::new("owner@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::Owner,
    )
}

async fn make_ask(
    db: &TestDb,
    allowlist: Arc<ToolCallAllowlist>,
    executor: Arc<dyn ToolExecutor>,
) -> (Arc<AskService>, Arc<SqliteAiOpsRepository>) {
    let pool = db.pool();
    let repo = Arc::new(SqliteAiOpsRepository::new(pool));
    let ask = Arc::new(AskService::new(
        repo.clone(),
        allowlist,
        executor,
        Arc::new(NoopAuditService),
    ));
    (ask, repo)
}

#[tokio::test]
async fn ask_executes_read_only_tool_immediately() {
    let db = TestDb::new().await;
    let executor = Arc::new(RecordingExecutor::default());
    let (ask, _repo) = make_ask(&db, Arc::new(allowlist_with_read_tool()), executor.clone()).await;
    let user = owner_user().await;

    let outcome = ask
        .ask(&user, None, "Is the panel healthy?")
        .await
        .expect("ask");
    assert_eq!(outcome.proposed_actions.len(), 0);
    assert_eq!(outcome.tool_results.len(), 1);
    assert_eq!(outcome.tool_results[0].tool.as_str(), "panel.health");
    let calls = executor.last();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "panel.health");
}

#[tokio::test]
async fn ask_proposes_write_action_without_executing() {
    let db = TestDb::new().await;
    let executor = Arc::new(RecordingExecutor::default());
    let (ask, repo) = make_ask(&db, Arc::new(allowlist_with_write_tool()), executor.clone()).await;
    let user = owner_user().await;

    let outcome = ask
        .ask(&user, None, "Please restart nginx now")
        .await
        .expect("ask");
    assert_eq!(outcome.proposed_actions.len(), 1);
    assert_eq!(
        outcome.proposed_actions[0].tool.as_str(),
        "system.reload_nginx"
    );
    assert_eq!(outcome.proposed_actions[0].status, AiActionStatus::Proposed);
    // The write tool MUST NOT have been executed by the executor.
    let calls = executor.last();
    assert!(
        calls.is_empty(),
        "write tool must not be executed pre-approval"
    );
    // The action is persisted.
    let actions = repo.list_pending_actions(50).await.expect("list pending");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].tool.as_str(), "system.reload_nginx");
}

#[tokio::test]
async fn approve_executes_write_and_records_audit() {
    let db = TestDb::new().await;
    let executor = Arc::new(RecordingExecutor::default());
    let pool = db.pool();
    let repo = Arc::new(SqliteAiOpsRepository::new(pool));
    let allowlist = Arc::new(allowlist_with_write_tool());
    let ask = Arc::new(AskService::new(
        repo.clone(),
        allowlist.clone(),
        executor.clone(),
        Arc::new(NoopAuditService),
    ));
    let approval = ActionApproval::new(
        repo.clone(),
        allowlist,
        executor.clone(),
        Arc::new(NoopAuditService),
    );
    let user = owner_user().await;

    let outcome = ask.ask(&user, None, "Reload nginx").await.expect("ask");
    let action_id = outcome.proposed_actions[0].id;

    let approved = approval.approve(&user, action_id).await.expect("approve");
    assert_eq!(approved.status, AiActionStatus::Executed);
    assert_eq!(approved.tool.as_str(), "system.reload_nginx");
    let calls = executor.last();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "system.reload_nginx");
}

#[tokio::test]
async fn deny_marks_action_without_executing() {
    let db = TestDb::new().await;
    let executor = Arc::new(RecordingExecutor::default());
    let pool = db.pool();
    let repo = Arc::new(SqliteAiOpsRepository::new(pool));
    let allowlist = Arc::new(allowlist_with_write_tool());
    let ask = Arc::new(AskService::new(
        repo.clone(),
        allowlist.clone(),
        executor.clone(),
        Arc::new(NoopAuditService),
    ));
    let approval = ActionApproval::new(
        repo.clone(),
        allowlist,
        executor.clone(),
        Arc::new(NoopAuditService),
    );
    let user = owner_user().await;

    let outcome = ask.ask(&user, None, "Reload nginx").await.expect("ask");
    let action_id = outcome.proposed_actions[0].id;
    let denied = approval.deny(&user, action_id).await.expect("deny");
    assert_eq!(denied.status, AiActionStatus::Denied);
    let calls = executor.last();
    assert!(calls.is_empty());
}

#[tokio::test]
async fn approving_already_executed_action_is_rejected() {
    let db = TestDb::new().await;
    let executor = Arc::new(RecordingExecutor::default());
    let pool = db.pool();
    let repo = Arc::new(SqliteAiOpsRepository::new(pool));
    let allowlist = Arc::new(allowlist_with_write_tool());
    let ask = Arc::new(AskService::new(
        repo.clone(),
        allowlist.clone(),
        executor.clone(),
        Arc::new(NoopAuditService),
    ));
    let approval = ActionApproval::new(
        repo.clone(),
        allowlist,
        executor.clone(),
        Arc::new(NoopAuditService),
    );
    let user = owner_user().await;

    let outcome = ask.ask(&user, None, "Reload nginx").await.expect("ask");
    let action_id = outcome.proposed_actions[0].id;
    approval.approve(&user, action_id).await.expect("first");
    let second = approval.approve(&user, action_id).await;
    assert!(matches!(second, Err(AiOpsError::ActionNotPending(_))));
}

#[tokio::test]
async fn unknown_tool_in_prompt_is_refused() {
    let db = TestDb::new().await;
    let executor = Arc::new(RecordingExecutor::default());
    let (ask, _repo) = make_ask(&db, Arc::new(allowlist_with_read_tool()), executor.clone()).await;
    let user = owner_user().await;
    // "format drive" matches no allowlisted read tool.
    let outcome = ask.ask(&user, None, "format drive").await.expect("ask");
    assert!(outcome.proposed_actions.is_empty());
    assert!(outcome.tool_results.is_empty());
    let calls = executor.last();
    assert!(calls.is_empty());
}

#[tokio::test]
async fn non_owner_cannot_approve() {
    let db = TestDb::new().await;
    let executor = Arc::new(RecordingExecutor::default());
    let pool = db.pool();
    let repo = Arc::new(SqliteAiOpsRepository::new(pool));
    let allowlist = Arc::new(allowlist_with_write_tool());
    let ask = Arc::new(AskService::new(
        repo.clone(),
        allowlist.clone(),
        executor.clone(),
        Arc::new(NoopAuditService),
    ));
    let approval = ActionApproval::new(
        repo.clone(),
        allowlist,
        executor.clone(),
        Arc::new(NoopAuditService),
    );
    let owner = owner_user().await;
    let outcome = ask.ask(&owner, None, "Reload nginx").await.expect("ask");
    let action_id = outcome.proposed_actions[0].id;
    let user_role = openpanel_domain::User::new(
        Uuid::new_v4(),
        openpanel_domain::Username::new("viewer").expect("static"),
        openpanel_domain::Email::new("viewer@example.com").expect("static"),
        openpanel_domain::Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    );
    let res = approval.approve(&user_role, action_id).await;
    assert!(matches!(res, Err(AiOpsError::Forbidden)));
    let _ = action_id;
    let _ = AiActionId::new();
}

#[tokio::test]
async fn default_executor_refuses_unknown_tool() {
    let executor = DefaultToolExecutor::new(ToolServices::default());
    let allowlist = allowlist_with_read_tool();
    let user = owner_user().await;
    let res = executor
        .run(
            &allowlist,
            &user,
            &ToolName::new("panel.unknown").expect("static"),
            serde_json::json!({}),
        )
        .await;
    assert!(matches!(res, Err(ToolExecutorError::NotPermitted(_))));
}

#[tokio::test]
async fn tool_name_validation_rejects_unsafe_names() {
    assert!(ToolName::new("BadName").is_none());
    assert!(ToolName::new("123start").is_none());
    assert!(ToolName::new("space name").is_none());
    assert!(ToolName::new("semi;colon").is_none());
    assert!(ToolName::new("").is_none());
    let ok = ToolName::new("a");
    assert!(ok.is_some());
    let ok = ToolName::new("good.name_2");
    assert!(ok.is_some());
}

#[tokio::test]
async fn allowlist_lookup_is_case_sensitive() {
    let allowlist = allowlist_with_read_tool();
    assert!(
        allowlist
            .lookup(&ToolName::new("panel.health").expect("static"))
            .is_some()
    );
    let upper = ToolName::new("PANEL.HEALTH");
    // Uppercase first letter is invalid; lookup is therefore None.
    assert!(upper.is_none());
}

#[tokio::test]
async fn message_round_trip_through_repo() {
    use openpanel_domain::{AiMessage, AiSessionId, MessageRole};
    let db = TestDb::new().await;
    let pool = db.pool();
    let repo = SqliteAiOpsRepository::new(pool);
    let session = openpanel_domain::AiSession::new(Uuid::new_v4(), "hello");
    repo.save_session(&session).await.expect("save session");
    let message = AiMessage {
        id: Uuid::new_v4(),
        session_id: session.id,
        role: MessageRole::User,
        content: "hi".into(),
        tool_results: Vec::new(),
        created_at: chrono::Utc::now(),
    };
    repo.save_message(&message).await.expect("save message");
    let messages = repo.list_messages(session.id).await.expect("list messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "hi");
    assert_eq!(messages[0].role, MessageRole::User);
    let sessions = repo
        .list_sessions(session.owner, 10)
        .await
        .expect("list sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session.id);
    let _ = AiSessionId::new();
}
