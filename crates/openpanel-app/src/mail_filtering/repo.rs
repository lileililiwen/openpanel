//! SQLite adapter for the mail anti-spam and filtering bounded context.

use chrono::{DateTime, Utc};
use openpanel_domain::{
    AntiSpamPolicy, AutoResponder, AutoResponderMode, CatchAll, Forwarder, GreylistEntry,
    MailFilterRepository, MailingList, RepoError, SieveScript,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// SQLite-backed `MailFilterRepository`.
#[derive(Clone)]
pub struct SqliteMailFilterRepository {
    pool: SqlitePool,
}

impl SqliteMailFilterRepository {
    /// Construct a repository over the shared SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl MailFilterRepository for SqliteMailFilterRepository {
    async fn save_policy(&self, policy: &AntiSpamPolicy) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO antispam_policies (mailbox_id, spam_threshold, greylist_enabled, updated_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(policy.mailbox_id.to_string())
        .bind(policy.spam_threshold as i64)
        .bind(if policy.greylist_enabled { 1 } else { 0 })
        .bind(policy.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_policy(&self, mailbox_id: Uuid) -> Result<Option<AntiSpamPolicy>, RepoError> {
        let row = sqlx::query(
            "SELECT mailbox_id, spam_threshold, greylist_enabled, updated_at FROM antispam_policies WHERE mailbox_id = ?",
        )
        .bind(mailbox_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_policy).transpose()
    }

    async fn save_greylist(&self, entry: &GreylistEntry) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO greylist (sender, recipient, first_seen_at, deferred_at, whitelisted) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&entry.sender)
        .bind(&entry.recipient)
        .bind(entry.first_seen_at.to_rfc3339())
        .bind(entry.deferred_at.to_rfc3339())
        .bind(if entry.whitelisted { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn save_sieve(&self, script: &SieveScript) -> Result<(), RepoError> {
        let last_compiled_at = script.last_compiled_at.map(|t| t.to_rfc3339());
        sqlx::query(
            "INSERT OR REPLACE INTO sieve_scripts (mailbox_id, script, last_compiled_at) \
             VALUES (?, ?, ?)",
        )
        .bind(script.mailbox_id.to_string())
        .bind(&script.script)
        .bind(last_compiled_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_sieve(&self, mailbox_id: Uuid) -> Result<Option<SieveScript>, RepoError> {
        let row = sqlx::query(
            "SELECT mailbox_id, script, last_compiled_at FROM sieve_scripts WHERE mailbox_id = ?",
        )
        .bind(mailbox_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_sieve).transpose()
    }

    async fn save_autoresponder(&self, autoresponder: &AutoResponder) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO autoresponders \
             (mailbox_id, enabled, body, mode, window_start, window_end) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(autoresponder.mailbox_id.to_string())
        .bind(if autoresponder.enabled { 1 } else { 0 })
        .bind(&autoresponder.body)
        .bind(autoresponder.mode.as_str())
        .bind(autoresponder.window_start.to_rfc3339())
        .bind(autoresponder.window_end.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_autoresponder(
        &self,
        mailbox_id: Uuid,
    ) -> Result<Option<AutoResponder>, RepoError> {
        let row = sqlx::query(
            "SELECT mailbox_id, enabled, body, mode, window_start, window_end FROM autoresponders WHERE mailbox_id = ?",
        )
        .bind(mailbox_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_autoresponder).transpose()
    }

    async fn save_forwarder(&self, forwarder: &Forwarder) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO forwarders (mailbox_id, destination, keep_local) \
             VALUES (?, ?, ?)",
        )
        .bind(forwarder.mailbox_id.to_string())
        .bind(&forwarder.destination)
        .bind(if forwarder.keep_local { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_forwarders(&self, mailbox_id: Uuid) -> Result<Vec<Forwarder>, RepoError> {
        let rows = sqlx::query(
            "SELECT mailbox_id, destination, keep_local FROM forwarders WHERE mailbox_id = ?",
        )
        .bind(mailbox_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(decode_forwarder).collect()
    }

    async fn delete_forwarder(&self, mailbox_id: Uuid, destination: &str) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM forwarders WHERE mailbox_id = ? AND destination = ?")
            .bind(mailbox_id.to_string())
            .bind(destination)
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn save_catch_all(&self, catch_all: &CatchAll) -> Result<(), RepoError> {
        sqlx::query("INSERT OR REPLACE INTO catch_all (domain, destination_mailbox) VALUES (?, ?)")
            .bind(&catch_all.domain)
            .bind(catch_all.destination_mailbox.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn save_mailing_list(&self, list: &MailingList) -> Result<(), RepoError> {
        let members =
            serde_json::to_string(&list.members).map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO mailing_lists (address, members_json, created_at) \
             VALUES (?, ?, ?)",
        )
        .bind(&list.address)
        .bind(members)
        .bind(list.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn get_mailing_list(&self, address: &str) -> Result<Option<MailingList>, RepoError> {
        let row = sqlx::query(
            "SELECT address, members_json, created_at FROM mailing_lists WHERE address = ?",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(decode_mailing_list).transpose()
    }
}

fn decode_policy(row: sqlx::sqlite::SqliteRow) -> Result<AntiSpamPolicy, RepoError> {
    let mailbox_id: String = row.try_get("mailbox_id").map_err(map_sqlx)?;
    let spam_threshold: i64 = row.try_get("spam_threshold").map_err(map_sqlx)?;
    let greylist_enabled: i64 = row.try_get("greylist_enabled").map_err(map_sqlx)?;
    let updated_at: String = row.try_get("updated_at").map_err(map_sqlx)?;
    let mailbox_id = Uuid::parse_str(&mailbox_id).map_err(|e| RepoError::new(e.to_string()))?;
    let updated_at = parse_ts(&updated_at)?;
    Ok(AntiSpamPolicy {
        mailbox_id,
        spam_threshold: spam_threshold as u8,
        greylist_enabled: greylist_enabled != 0,
        updated_at,
    })
}

fn decode_sieve(row: sqlx::sqlite::SqliteRow) -> Result<SieveScript, RepoError> {
    let mailbox_id: String = row.try_get("mailbox_id").map_err(map_sqlx)?;
    let script: String = row.try_get("script").map_err(map_sqlx)?;
    let last_compiled_at: Option<String> = row.try_get("last_compiled_at").map_err(map_sqlx)?;
    let mailbox_id = Uuid::parse_str(&mailbox_id).map_err(|e| RepoError::new(e.to_string()))?;
    let last_compiled_at = last_compiled_at.as_deref().map(parse_ts).transpose()?;
    Ok(SieveScript {
        mailbox_id,
        script,
        last_compiled_at,
    })
}

fn decode_autoresponder(row: sqlx::sqlite::SqliteRow) -> Result<AutoResponder, RepoError> {
    let mailbox_id: String = row.try_get("mailbox_id").map_err(map_sqlx)?;
    let enabled: i64 = row.try_get("enabled").map_err(map_sqlx)?;
    let body: String = row.try_get("body").map_err(map_sqlx)?;
    let mode: String = row.try_get("mode").map_err(map_sqlx)?;
    let window_start: String = row.try_get("window_start").map_err(map_sqlx)?;
    let window_end: String = row.try_get("window_end").map_err(map_sqlx)?;
    let mailbox_id = Uuid::parse_str(&mailbox_id).map_err(|e| RepoError::new(e.to_string()))?;
    let mode = match mode.as_str() {
        "once" => AutoResponderMode::Once,
        "every" => AutoResponderMode::Every,
        other => return Err(RepoError::new(format!("unknown mode: {other}"))),
    };
    let window_start = parse_ts(&window_start)?;
    let window_end = parse_ts(&window_end)?;
    Ok(AutoResponder {
        mailbox_id,
        enabled: enabled != 0,
        body,
        mode,
        window_start,
        window_end,
    })
}

fn decode_forwarder(row: sqlx::sqlite::SqliteRow) -> Result<Forwarder, RepoError> {
    let mailbox_id: String = row.try_get("mailbox_id").map_err(map_sqlx)?;
    let destination: String = row.try_get("destination").map_err(map_sqlx)?;
    let keep_local: i64 = row.try_get("keep_local").map_err(map_sqlx)?;
    let mailbox_id = Uuid::parse_str(&mailbox_id).map_err(|e| RepoError::new(e.to_string()))?;
    Ok(Forwarder {
        mailbox_id,
        destination,
        keep_local: keep_local != 0,
    })
}

fn decode_mailing_list(row: sqlx::sqlite::SqliteRow) -> Result<MailingList, RepoError> {
    let address: String = row.try_get("address").map_err(map_sqlx)?;
    let members_json: String = row.try_get("members_json").map_err(map_sqlx)?;
    let created_at: String = row.try_get("created_at").map_err(map_sqlx)?;
    let members: Vec<Uuid> = serde_json::from_str(&members_json)
        .map_err(|e| RepoError::new(format!("invalid members_json: {e}")))?;
    let created_at = parse_ts(&created_at)?;
    Ok(MailingList {
        address,
        members,
        created_at,
    })
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("invalid timestamp: {e}")))
}

fn map_sqlx(e: sqlx::Error) -> RepoError {
    RepoError::new(e.to_string())
}
