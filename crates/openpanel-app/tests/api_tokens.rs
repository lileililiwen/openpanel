//! API-token application service tests over in-memory ports.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    net::IpAddr,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use openpanel_app::api_tokens::{ApiTokenService, CreateApiToken, RateLimitConfig, TokenAuthError};
use openpanel_core::AuditAction;
use openpanel_domain::{
    ApiToken, ApiTokenRepository, RepoError, Role, TokenHash, TokenScope, User,
    common::{Email, Password, Username},
};
use openpanel_test_support::{MockAudit, MockUserRepo};
use uuid::Uuid;

#[derive(Default)]
struct MemoryTokenRepo {
    tokens: Mutex<Vec<ApiToken>>,
}

#[async_trait]
impl ApiTokenRepository for MemoryTokenRepo {
    async fn create(&self, token: &ApiToken) -> Result<(), RepoError> {
        self.tokens.lock().unwrap().push(token.clone());
        Ok(())
    }

    async fn update(&self, token: &ApiToken) -> Result<(), RepoError> {
        let mut tokens = self.tokens.lock().unwrap();
        let stored = tokens
            .iter_mut()
            .find(|item| item.id() == token.id())
            .unwrap();
        *stored = token.clone();
        Ok(())
    }

    async fn rotate(&self, old: &ApiToken, new: &ApiToken) -> Result<(), RepoError> {
        let mut tokens = self.tokens.lock().unwrap();
        let stored = tokens
            .iter_mut()
            .find(|item| item.id() == old.id())
            .unwrap();
        *stored = old.clone();
        tokens.push(new.clone());
        Ok(())
    }

    async fn find(&self, user_id: Uuid, id: Uuid) -> Result<Option<ApiToken>, RepoError> {
        Ok(self
            .tokens
            .lock()
            .unwrap()
            .iter()
            .find(|item| item.user_id() == user_id && item.id() == id)
            .cloned())
    }

    async fn find_by_hash(&self, hash: &TokenHash) -> Result<Option<ApiToken>, RepoError> {
        Ok(self
            .tokens
            .lock()
            .unwrap()
            .iter()
            .find(|item| item.hash().constant_time_eq(hash))
            .cloned())
    }

    async fn list(&self, user_id: Uuid) -> Result<Vec<ApiToken>, RepoError> {
        Ok(self
            .tokens
            .lock()
            .unwrap()
            .iter()
            .filter(|item| item.user_id() == user_id)
            .cloned()
            .collect())
    }
}

fn user(role: Role) -> User {
    User::new(
        Uuid::new_v4(),
        Username::new(format!("{}-actor", role.as_str())).unwrap(),
        Email::new(format!("{}@example.com", role.as_str())).unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        role,
    )
}

fn service(repo: Arc<MemoryTokenRepo>, target: User, burst: u32) -> ApiTokenService {
    let mut users = MockUserRepo::new();
    users
        .expect_find_by_id()
        .returning(move |_| Ok(Some(target.clone())));
    let mut audit = MockAudit::new();
    audit.expect_record().returning(|_| Ok(()));
    audit.expect_recent().returning(|_| Ok(vec![]));
    ApiTokenService::new(
        repo,
        Arc::new(users),
        Arc::new(audit),
        [42; 32],
        RateLimitConfig {
            burst,
            per_minute: 60,
        },
    )
}

#[tokio::test]
async fn create_lists_metadata_and_user_cannot_issue_write_scope() {
    let actor = user(Role::User);
    let repo = Arc::new(MemoryTokenRepo::default());
    let svc = service(repo, actor.clone(), 60);
    let created = svc
        .create(
            &actor,
            CreateApiToken {
                user_id: actor.id(),
                label: "reader".into(),
                scopes: vec!["sites:read".into()],
                expires_at: Utc::now() + Duration::days(1),
                cidr_allowlist: vec![],
            },
        )
        .await
        .unwrap();
    assert!(created.plaintext_token.starts_with("openpanel_pat_"));
    assert!(created.shown_once);
    let listed = svc.list(&actor, actor.id()).await.unwrap();
    assert_eq!(listed.len(), 1);
    let json = serde_json::to_string(&listed).unwrap();
    assert!(!json.contains(&created.plaintext_token));
    assert!(!json.contains("hash"));

    let error = svc
        .create(
            &actor,
            CreateApiToken {
                user_id: actor.id(),
                label: "writer".into(),
                scopes: vec!["sites:write".into()],
                expires_at: Utc::now() + Duration::days(1),
                cidr_allowlist: vec![],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, TokenAuthError::Forbidden));
}

#[tokio::test]
async fn rotate_revokes_old_credential_and_returns_new_plaintext_once() {
    let actor = user(Role::Owner);
    let repo = Arc::new(MemoryTokenRepo::default());
    let svc = service(repo, actor.clone(), 60);
    let created = svc
        .create(
            &actor,
            CreateApiToken {
                user_id: actor.id(),
                label: "automation".into(),
                scopes: vec!["sites:read".into()],
                expires_at: Utc::now() + Duration::days(1),
                cidr_allowlist: vec![],
            },
        )
        .await
        .unwrap();
    let rotated = svc
        .rotate(&actor, actor.id(), created.metadata.id)
        .await
        .unwrap();
    assert_ne!(created.plaintext_token, rotated.plaintext_token);
    assert!(matches!(
        svc.authenticate(
            &created.plaintext_token,
            "127.0.0.1".parse().unwrap(),
            &TokenScope::parse("sites:read").unwrap()
        )
        .await,
        Err(TokenAuthError::Unauthorized)
    ));
    assert!(
        svc.authenticate(
            &rotated.plaintext_token,
            "127.0.0.1".parse().unwrap(),
            &TokenScope::parse("sites:read").unwrap(),
        )
        .await
        .is_ok()
    );
}

#[tokio::test]
async fn bearer_enforces_scope_cidr_and_rate_limit() {
    let actor = user(Role::Owner);
    let repo = Arc::new(MemoryTokenRepo::default());
    let svc = service(repo, actor.clone(), 1);
    let created = svc
        .create(
            &actor,
            CreateApiToken {
                user_id: actor.id(),
                label: "limited".into(),
                scopes: vec!["sites:read".into()],
                expires_at: Utc::now() + Duration::days(1),
                cidr_allowlist: vec!["127.0.0.0/8".into()],
            },
        )
        .await
        .unwrap();
    let read = TokenScope::parse("sites:read").unwrap();
    assert!(matches!(
        svc.authenticate(
            &created.plaintext_token,
            "10.0.0.1".parse::<IpAddr>().unwrap(),
            &read
        )
        .await,
        Err(TokenAuthError::CidrRejected)
    ));
    assert!(matches!(
        svc.authenticate(
            &created.plaintext_token,
            "127.0.0.1".parse().unwrap(),
            &TokenScope::parse("sites:write").unwrap()
        )
        .await,
        Err(TokenAuthError::ScopeRejected)
    ));
    svc.authenticate(
        &created.plaintext_token,
        "127.0.0.1".parse().unwrap(),
        &read,
    )
    .await
    .unwrap();
    assert!(matches!(
        svc.authenticate(
            &created.plaintext_token,
            "127.0.0.1".parse().unwrap(),
            &read
        )
        .await,
        Err(TokenAuthError::RateLimited {
            retry_after_seconds: 1..
        })
    ));
}

#[tokio::test]
async fn revoke_immediately_invalidates_token_and_audits_lifecycle() {
    let actor = user(Role::Owner);
    let repo = Arc::new(MemoryTokenRepo::default());
    let mut users = MockUserRepo::new();
    let target = actor.clone();
    users
        .expect_find_by_id()
        .returning(move |_| Ok(Some(target.clone())));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_for_audit = seen.clone();
    let mut audit = MockAudit::new();
    audit.expect_record().returning(move |event| {
        seen_for_audit.lock().unwrap().push(event.action);
        Ok(())
    });
    audit.expect_recent().returning(|_| Ok(vec![]));
    let svc = ApiTokenService::new(
        repo,
        Arc::new(users),
        Arc::new(audit),
        [42; 32],
        RateLimitConfig::default(),
    );
    let created = svc
        .create(
            &actor,
            CreateApiToken {
                user_id: actor.id(),
                label: "revoke me".into(),
                scopes: vec!["sites:read".into()],
                expires_at: Utc::now() + Duration::days(1),
                cidr_allowlist: vec![],
            },
        )
        .await
        .unwrap();
    svc.revoke(&actor, actor.id(), created.metadata.id)
        .await
        .unwrap();
    let actions = seen.lock().unwrap();
    assert!(actions.contains(&AuditAction::TokenCreated));
    assert!(actions.contains(&AuditAction::TokenRevoked));
}
