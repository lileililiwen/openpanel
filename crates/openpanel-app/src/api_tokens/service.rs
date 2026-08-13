//! API-token lifecycle, bearer authentication, and rate limiting.

use std::{
    collections::{BTreeSet, HashMap},
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

use chrono::{DateTime, Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    ApiToken, ApiTokenError, ApiTokenMetadata, ApiTokenRepository, Cidr, Role, TokenCredential,
    TokenScope, User, UserRepository,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Per-token bucket settings.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum immediate burst.
    #[serde(default = "default_burst")]
    pub burst: u32,
    /// Refill rate.
    #[serde(default = "default_per_minute")]
    pub per_minute: u32,
}

const fn default_burst() -> u32 {
    60
}
const fn default_per_minute() -> u32 {
    60
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            burst: default_burst(),
            per_minute: default_per_minute(),
        }
    }
}

/// Token creation parameters.
#[derive(Debug, Clone)]
pub struct CreateApiToken {
    /// Principal to bind.
    pub user_id: Uuid,
    /// Display label.
    pub label: String,
    /// Explicit string scopes.
    pub scopes: Vec<String>,
    /// Explicit expiration.
    pub expires_at: DateTime<Utc>,
    /// Optional network allowlist.
    pub cidr_allowlist: Vec<String>,
}

/// Create or rotate result carrying plaintext exactly once.
#[derive(Debug, Clone, Serialize)]
pub struct CreatedApiToken {
    /// Safe persisted metadata.
    #[serde(flatten)]
    pub metadata: ApiTokenMetadata,
    /// Plaintext returned only by this operation.
    pub plaintext_token: String,
    /// Stable client warning.
    pub shown_once: bool,
}

/// Principal resolved from a bearer.
#[derive(Debug, Clone)]
pub struct BearerPrincipal {
    /// Bound user.
    pub user: User,
    /// Auditable token id.
    pub token_id: Uuid,
}

/// Lifecycle or bearer authorization error.
#[derive(Debug, Error)]
pub enum TokenAuthError {
    /// Invalid, expired, or revoked credential.
    #[error("unauthorized")]
    Unauthorized,
    /// Token exists but is expired.
    #[error("token expired")]
    Expired,
    /// Valid token lacks the route scope.
    #[error("token scope rejected")]
    ScopeRejected,
    /// Peer is outside its allowlist.
    #[error("token CIDR rejected")]
    CidrRejected,
    /// Actor lacks token-management permission.
    #[error("forbidden")]
    Forbidden,
    /// Bucket is empty.
    #[error("token rate limited")]
    RateLimited {
        /// Retry delay.
        retry_after_seconds: u64,
    },
    /// Input failed domain validation.
    #[error(transparent)]
    Invalid(#[from] ApiTokenError),
    /// Adapter error.
    #[error("token service failed: {0}")]
    Internal(String),
}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Application service for scoped tokens.
pub struct ApiTokenService {
    repo: Arc<dyn ApiTokenRepository>,
    users: Arc<dyn UserRepository>,
    audit: Arc<dyn AuditService>,
    pepper: [u8; 32],
    rate: RateLimitConfig,
    buckets: Mutex<HashMap<Uuid, Bucket>>,
}

impl ApiTokenService {
    /// Construct over domain ports.
    pub fn new(
        repo: Arc<dyn ApiTokenRepository>,
        users: Arc<dyn UserRepository>,
        audit: Arc<dyn AuditService>,
        pepper: [u8; 32],
        rate: RateLimitConfig,
    ) -> Self {
        Self {
            repo,
            users,
            audit,
            pepper,
            rate,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Create a token after RBAC and scope validation.
    pub async fn create(
        &self,
        actor: &User,
        input: CreateApiToken,
    ) -> Result<CreatedApiToken, TokenAuthError> {
        let target = self.authorize_target(actor, input.user_id).await?;
        let scopes = parse_scopes(input.scopes)?;
        authorize_scopes(actor.role(), &scopes)?;
        let now = Utc::now();
        if input.expires_at > now + Duration::days(365) {
            return Err(TokenAuthError::Invalid(ApiTokenError::InvalidExpiration));
        }
        let cidrs = input
            .cidr_allowlist
            .into_iter()
            .map(|cidr| Cidr::parse(&cidr))
            .collect::<Result<Vec<_>, _>>()?;
        let credential = TokenCredential::generate();
        let token = ApiToken::new(
            Uuid::new_v4(),
            target.id(),
            input.label,
            credential.hash(&self.pepper),
            scopes,
            cidrs,
            input.expires_at,
            now,
        )?;
        self.repo.create(&token).await.map_err(internal)?;
        self.record(
            actor,
            AuditAction::TokenCreated,
            AuditOutcome::Success,
            token.id(),
            serde_json::json!({"token_id": token.id(), "user_id": target.id()}),
        )
        .await;
        Ok(created(token, credential))
    }

    /// List safe metadata for an authorized principal.
    pub async fn list(
        &self,
        actor: &User,
        user_id: Uuid,
    ) -> Result<Vec<ApiTokenMetadata>, TokenAuthError> {
        self.authorize_target(actor, user_id).await?;
        Ok(self
            .repo
            .list(user_id)
            .await
            .map_err(internal)?
            .into_iter()
            .map(|token| token.metadata())
            .collect())
    }

    /// Revoke immediately.
    pub async fn revoke(
        &self,
        actor: &User,
        user_id: Uuid,
        id: Uuid,
    ) -> Result<(), TokenAuthError> {
        self.authorize_target(actor, user_id).await?;
        let mut token = self
            .repo
            .find(user_id, id)
            .await
            .map_err(internal)?
            .ok_or(TokenAuthError::Invalid(ApiTokenError::NotFound))?;
        token.revoke(Utc::now())?;
        self.repo.update(&token).await.map_err(internal)?;
        self.record(
            actor,
            AuditAction::TokenRevoked,
            AuditOutcome::Success,
            id,
            serde_json::json!({"token_id": id}),
        )
        .await;
        Ok(())
    }

    /// Atomically revoke and replace a token while preserving policy.
    pub async fn rotate(
        &self,
        actor: &User,
        user_id: Uuid,
        id: Uuid,
    ) -> Result<CreatedApiToken, TokenAuthError> {
        self.authorize_target(actor, user_id).await?;
        let mut old = self
            .repo
            .find(user_id, id)
            .await
            .map_err(internal)?
            .ok_or(TokenAuthError::Invalid(ApiTokenError::NotFound))?;
        if !old.is_active(Utc::now()) {
            return Err(TokenAuthError::Unauthorized);
        }
        let now = Utc::now();
        old.revoke(now)?;
        let credential = TokenCredential::generate();
        let new = ApiToken::new(
            Uuid::new_v4(),
            old.user_id(),
            old.label().to_owned(),
            credential.hash(&self.pepper),
            old.scopes().clone(),
            old.cidr_allowlist().to_vec(),
            old.expires_at(),
            now,
        )?;
        self.repo.rotate(&old, &new).await.map_err(internal)?;
        self.record(
            actor,
            AuditAction::TokenRotated,
            AuditOutcome::Success,
            new.id(),
            serde_json::json!({"token_id": new.id(), "rotated_from": old.id()}),
        )
        .await;
        Ok(created(new, credential))
    }

    /// Resolve and authorize a bearer for an exact route scope.
    pub async fn authenticate(
        &self,
        plaintext: &str,
        peer: IpAddr,
        required_scope: &TokenScope,
    ) -> Result<BearerPrincipal, TokenAuthError> {
        let credential =
            TokenCredential::parse(plaintext).map_err(|_| TokenAuthError::Unauthorized)?;
        let hash = credential.hash(&self.pepper);
        let mut token = self
            .repo
            .find_by_hash(&hash)
            .await
            .map_err(internal)?
            .filter(|found| found.hash().constant_time_eq(&hash))
            .ok_or(TokenAuthError::Unauthorized)?;
        let now = Utc::now();
        if token.revoked_at().is_some() {
            return Err(TokenAuthError::Unauthorized);
        }
        if now >= token.expires_at() {
            self.record_token(AuditAction::TokenExpired, AuditOutcome::Denied, token.id())
                .await;
            return Err(TokenAuthError::Expired);
        }
        if !token.allows_ip(peer) {
            self.record_token(
                AuditAction::TokenCidrRejected,
                AuditOutcome::Denied,
                token.id(),
            )
            .await;
            return Err(TokenAuthError::CidrRejected);
        }
        if !token.allows_scope(required_scope) {
            self.record_token(
                AuditAction::TokenScopeRejected,
                AuditOutcome::Denied,
                token.id(),
            )
            .await;
            return Err(TokenAuthError::ScopeRejected);
        }
        if let Err(error) = self.consume(token.id()) {
            self.record_token(
                AuditAction::TokenRateLimited,
                AuditOutcome::Denied,
                token.id(),
            )
            .await;
            return Err(error);
        }
        let user = self
            .users
            .find_by_id(token.user_id())
            .await
            .map_err(internal)?
            .filter(|user| !user.is_disabled())
            .ok_or(TokenAuthError::Unauthorized)?;
        token.mark_used(now);
        self.repo.update(&token).await.map_err(internal)?;
        self.record_token(AuditAction::TokenRequest, AuditOutcome::Success, token.id())
            .await;
        Ok(BearerPrincipal {
            user,
            token_id: token.id(),
        })
    }

    async fn authorize_target(&self, actor: &User, user_id: Uuid) -> Result<User, TokenAuthError> {
        let target = self
            .users
            .find_by_id(user_id)
            .await
            .map_err(internal)?
            .ok_or(TokenAuthError::Forbidden)?;
        let allowed = actor.id() == user_id
            || actor.role() == Role::Owner
            || (actor.role() == Role::Admin && target.role() != Role::Owner);
        if allowed {
            Ok(target)
        } else {
            Err(TokenAuthError::Forbidden)
        }
    }

    fn consume(&self, token_id: Uuid) -> Result<(), TokenAuthError> {
        let mut buckets = self
            .buckets
            .lock()
            .map_err(|_| TokenAuthError::Internal("rate limiter lock poisoned".into()))?;
        let now = Instant::now();
        let bucket = buckets.entry(token_id).or_insert(Bucket {
            tokens: f64::from(self.rate.burst),
            last_refill: now,
        });
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        let refill_per_second = f64::from(self.rate.per_minute) / 60.0;
        bucket.tokens =
            (bucket.tokens + elapsed * refill_per_second).min(f64::from(self.rate.burst));
        bucket.last_refill = now;
        if bucket.tokens < 1.0 {
            let retry = ((1.0 - bucket.tokens) / refill_per_second).ceil().max(1.0) as u64;
            return Err(TokenAuthError::RateLimited {
                retry_after_seconds: retry,
            });
        }
        bucket.tokens -= 1.0;
        Ok(())
    }

    async fn record(
        &self,
        actor: &User,
        action: AuditAction,
        outcome: AuditOutcome,
        target: Uuid,
        metadata: serde_json::Value,
    ) {
        self.audit
            .record(
                AuditEvent::new(actor.username().as_str(), action, outcome)
                    .target(target.to_string())
                    .metadata(metadata),
            )
            .await
            .ok();
    }

    async fn record_token(&self, action: AuditAction, outcome: AuditOutcome, token_id: Uuid) {
        self.audit
            .record(
                AuditEvent::new("api-token", action, outcome)
                    .target(token_id.to_string())
                    .metadata(serde_json::json!({"token_id": token_id})),
            )
            .await
            .ok();
    }
}

fn created(token: ApiToken, credential: TokenCredential) -> CreatedApiToken {
    CreatedApiToken {
        metadata: token.metadata(),
        plaintext_token: credential.expose().to_owned(),
        shown_once: true,
    }
}

fn parse_scopes(values: Vec<String>) -> Result<BTreeSet<TokenScope>, TokenAuthError> {
    values
        .into_iter()
        .map(|value| TokenScope::parse(&value))
        .collect::<Result<_, _>>()
        .map_err(TokenAuthError::from)
}

fn authorize_scopes(role: Role, scopes: &BTreeSet<TokenScope>) -> Result<(), TokenAuthError> {
    if role == Role::User && scopes.iter().any(|scope| scope.verb() != "read") {
        return Err(TokenAuthError::Forbidden);
    }
    Ok(())
}

fn internal(error: impl std::fmt::Display) -> TokenAuthError {
    TokenAuthError::Internal(error.to_string())
}
