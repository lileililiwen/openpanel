//! Mail anti-spam and filtering services: mail filter service,
//! sieve compiler, spam scorer, and mailing list service.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    AntiSpamPolicy, AutoResponder, CatchAll, Forwarder, MailFilterError, MailFilterRepository,
    MailingList, Role, SieveScript, User,
};
use uuid::Uuid;

use crate::mail_filtering::SqliteMailFilterRepository;

/// Pure sieve compiler. The v1 implementation only performs a
/// structural validation (size cap + brace / keyword scan). A real
/// compiler would shell out to `sieve-filter`.
pub struct SieveCompiler;

impl SieveCompiler {
    /// Construct a compiler.
    pub fn new() -> Self {
        Self
    }

    /// Compile `script`. Returns `Ok(())` on success and an
    /// error describing the failure otherwise.
    pub fn compile(&self, script: &SieveScript) -> Result<(), MailFilterError> {
        if script.script.len() > openpanel_domain::SIEVE_MAX_BYTES {
            return Err(MailFilterError::SieveTooLarge(
                script.script.len(),
                openpanel_domain::SIEVE_MAX_BYTES,
            ));
        }
        // The simplest of validations: an opening brace must
        // match a closing brace in the body. Anything else is
        // accepted (the real compiler would parse the AST).
        let opens = script.script.matches('{').count();
        let closes = script.script.matches('}').count();
        if opens != closes {
            return Err(MailFilterError::SieveCompile("unbalanced braces".into()));
        }
        Ok(())
    }
}

impl Default for SieveCompiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure spam scorer. The v1 ships a deterministic function of the
/// message size and a few features; production wiring swaps in a
/// real scorer (SpamAssassin / rspamd).
pub struct SpamScorer;

impl SpamScorer {
    /// Construct a scorer.
    pub fn new() -> Self {
        Self
    }

    /// Score a message; returns `Ok(0..=100)` or an error.
    pub fn score(&self, body_len: usize, has_attachment: bool, subject_uppercase_ratio: f32) -> u8 {
        // Deterministic toy score: every feature adds a small
        // amount. The score never exceeds 100.
        let mut score = 0.0_f32;
        if body_len > 100_000 {
            score += 20.0;
        }
        if has_attachment {
            score += 15.0;
        }
        score += subject_uppercase_ratio * 25.0;
        if score > 100.0 {
            score = 100.0;
        }
        score as u8
    }
}

impl Default for SpamScorer {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of a single mail score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreOutcome {
    /// Score is below the threshold; deliver to INBOX.
    Inbox,
    /// Score is at or above the threshold; deliver to Spam.
    Spam,
}

/// Decide the folder based on the score and the policy.
pub fn route_for_score(policy: &AntiSpamPolicy, score: u8) -> ScoreOutcome {
    if score >= policy.spam_threshold {
        ScoreOutcome::Spam
    } else {
        ScoreOutcome::Inbox
    }
}

/// Top-level mail filter service.
pub struct MailFilterService {
    repo: Arc<SqliteMailFilterRepository>,
    audit: Arc<dyn AuditService>,
    compiler: SieveCompiler,
}

impl MailFilterService {
    /// Construct a service.
    pub fn new(
        repo: Arc<SqliteMailFilterRepository>,
        audit: Arc<dyn AuditService>,
        compiler: SieveCompiler,
    ) -> Self {
        Self {
            repo,
            audit,
            compiler,
        }
    }

    /// Apply a per-mailbox anti-spam policy.
    pub async fn set_policy(
        &self,
        caller: &User,
        policy: AntiSpamPolicy,
    ) -> Result<AntiSpamPolicy, MailFilterError> {
        require_admin(caller)?;
        policy.validate()?;
        self.repo.save_policy(&policy).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::AntispamChanged,
                    AuditOutcome::Success,
                )
                .target(policy.mailbox_id.to_string())
                .metadata(serde_json::json!({
                    "spam_threshold": policy.spam_threshold,
                    "greylist_enabled": policy.greylist_enabled,
                })),
            )
            .await;
        Ok(policy)
    }

    /// Apply a Sieve filter. The script is compiled before
    /// being saved; a compile failure is surfaced to the caller.
    pub async fn set_sieve(
        &self,
        caller: &User,
        script: SieveScript,
    ) -> Result<SieveScript, MailFilterError> {
        require_admin(caller)?;
        self.compiler.compile(&script)?;
        let mut compiled = script;
        compiled.mark_compiled(Utc::now());
        self.repo.save_sieve(&compiled).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SieveApplied,
                    AuditOutcome::Success,
                )
                .target(compiled.mailbox_id.to_string())
                .metadata(serde_json::json!({ "bytes": compiled.script.len() })),
            )
            .await;
        Ok(compiled)
    }

    /// Load the Sieve script for a mailbox.
    pub async fn get_sieve(
        &self,
        caller: &User,
        mailbox_id: Uuid,
    ) -> Result<Option<SieveScript>, MailFilterError> {
        require_admin(caller)?;
        Ok(self.repo.get_sieve(mailbox_id).await?)
    }

    /// Load the autoresponder for a mailbox.
    pub async fn get_autoresponder(
        &self,
        caller: &User,
        mailbox_id: Uuid,
    ) -> Result<Option<AutoResponder>, MailFilterError> {
        require_admin(caller)?;
        Ok(self.repo.get_autoresponder(mailbox_id).await?)
    }

    /// Disable (but keep) the autoresponder for a mailbox.
    pub async fn disable_autoresponder(
        &self,
        caller: &User,
        mailbox_id: Uuid,
    ) -> Result<(), MailFilterError> {
        require_admin(caller)?;
        if let Some(mut responder) = self.repo.get_autoresponder(mailbox_id).await? {
            responder.enabled = false;
            self.repo.save_autoresponder(&responder).await?;
        }
        Ok(())
    }

    /// List forwarders for a mailbox.
    pub async fn list_forwarders(
        &self,
        caller: &User,
        mailbox_id: Uuid,
    ) -> Result<Vec<Forwarder>, MailFilterError> {
        require_admin(caller)?;
        Ok(self.repo.list_forwarders(mailbox_id).await?)
    }

    /// Remove one forwarder matching both source and destination.
    pub async fn remove_forwarder_for_source(
        &self,
        caller: &User,
        mailbox_id: Uuid,
        source: &str,
        destination: &str,
    ) -> Result<(), MailFilterError> {
        require_admin(caller)?;
        let forwarders = self.repo.list_forwarders(mailbox_id).await?;
        let matches = forwarders
            .iter()
            .any(|forwarder| forwarder.destination.eq_ignore_ascii_case(destination))
            && source.eq_ignore_ascii_case(destination);
        if !matches {
            return Err(MailFilterError::ForwarderLoop);
        }
        self.repo.delete_forwarder(mailbox_id, destination).await?;
        Ok(())
    }

    /// Load the catch-all for a domain.
    pub async fn get_catch_all(
        &self,
        caller: &User,
        domain: &str,
    ) -> Result<Option<CatchAll>, MailFilterError> {
        require_admin(caller)?;
        Ok(self.repo.get_catch_all(domain).await?)
    }

    /// Toggle an autoresponder.
    pub async fn set_autoresponder(
        &self,
        caller: &User,
        autoresponder: AutoResponder,
    ) -> Result<AutoResponder, MailFilterError> {
        require_admin(caller)?;
        autoresponder.validate()?;
        self.repo.save_autoresponder(&autoresponder).await?;
        Ok(autoresponder)
    }

    /// Add a forwarder. Refuses self-loops.
    pub async fn add_forwarder(
        &self,
        caller: &User,
        forwarder: Forwarder,
        source: &str,
    ) -> Result<Forwarder, MailFilterError> {
        require_admin(caller)?;
        if forwarder.detects_loop(source) {
            return Err(MailFilterError::ForwarderLoop);
        }
        self.repo.save_forwarder(&forwarder).await?;
        Ok(forwarder)
    }

    /// Remove a forwarder.
    pub async fn remove_forwarder(
        &self,
        caller: &User,
        mailbox_id: Uuid,
        destination: &str,
    ) -> Result<(), MailFilterError> {
        require_admin(caller)?;
        self.repo.delete_forwarder(mailbox_id, destination).await?;
        Ok(())
    }

    /// Persist a catch-all.
    pub async fn set_catch_all(
        &self,
        caller: &User,
        catch_all: CatchAll,
    ) -> Result<CatchAll, MailFilterError> {
        require_admin(caller)?;
        self.repo.save_catch_all(&catch_all).await?;
        Ok(catch_all)
    }
}

/// Mailing list service.
pub struct MailingListService {
    repo: Arc<SqliteMailFilterRepository>,
}

impl MailingListService {
    /// Construct a mailing list service.
    pub fn new(repo: Arc<SqliteMailFilterRepository>) -> Self {
        Self { repo }
    }

    /// Create or replace a mailing list.
    pub async fn upsert(
        &self,
        caller: &User,
        list: MailingList,
    ) -> Result<MailingList, MailFilterError> {
        require_admin(caller)?;
        self.repo.save_mailing_list(&list).await?;
        Ok(list)
    }

    /// Load a list by address.
    pub async fn get(&self, address: &str) -> Result<Option<MailingList>, MailFilterError> {
        Ok(self.repo.get_mailing_list(address).await?)
    }

    /// Load a list by address for an authorised caller.
    pub async fn get_for_caller(
        &self,
        caller: &User,
        address: &str,
    ) -> Result<Option<MailingList>, MailFilterError> {
        require_admin(caller)?;
        self.get(address).await
    }

    /// List every mailing list.
    pub async fn list_all(&self, caller: &User) -> Result<Vec<MailingList>, MailFilterError> {
        require_admin(caller)?;
        Ok(self.repo.list_mailing_lists().await?)
    }

    /// Delete a mailing list.
    pub async fn remove(&self, caller: &User, address: &str) -> Result<(), MailFilterError> {
        require_admin(caller)?;
        self.repo.delete_mailing_list(address).await?;
        Ok(())
    }
}

fn require_admin(caller: &User) -> Result<(), MailFilterError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(MailFilterError::Forbidden),
    }
}
