//! SQLite-backed adapter for `CertificateRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    RepoError,
    ssl::{
        certificate::{Certificate, KeyType},
        repository::CertificateRepository,
        source::CertificateSource,
    },
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// SQLite-backed adapter for the domain `CertificateRepository` trait.
#[derive(Clone)]
pub struct SqliteCertificateRepository {
    pool: Pool<Sqlite>,
}

impl SqliteCertificateRepository {
    /// Build a repository over the given SQLite connection pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn parse_source(s: &str) -> CertificateSource {
    match s {
        "acme" => CertificateSource::Acme,
        "manual" => CertificateSource::Manual,
        "self_signed" => CertificateSource::SelfSigned,
        _ => CertificateSource::Manual,
    }
}

fn parse_key_type(s: &str) -> KeyType {
    match s {
        "ecdsa-p256" => KeyType::EcdsaP256,
        "ecdsa-p384" => KeyType::EcdsaP384,
        "rsa-2048" => KeyType::Rsa2048,
        "rsa-4096" => KeyType::Rsa4096,
        _ => KeyType::EcdsaP256,
    }
}

#[async_trait]
impl CertificateRepository for SqliteCertificateRepository {
    async fn insert(&self, cert: &Certificate) -> Result<(), RepoError> {
        sqlx::query(
            r#"
            INSERT INTO certificates
                (id, domain, source, issuer, valid_from, valid_to, key_type,
                 cert_pem, chain_pem, key_ciphertext, force_https,
                 acme_endpoint, created_at, renewed_at, last_error)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(cert.id.to_string())
        .bind(&cert.domain)
        .bind(cert.source.as_str())
        .bind(&cert.issuer)
        .bind(cert.valid_from.to_rfc3339())
        .bind(cert.valid_to.to_rfc3339())
        .bind(cert.key_type.as_str())
        .bind(&cert.cert_pem)
        .bind(&cert.chain_pem)
        .bind(&cert.key_pem)
        .bind(cert.force_https)
        .bind(cert.acme_endpoint.as_deref())
        .bind(cert.created_at.to_rfc3339())
        .bind(cert.renewed_at.map(|d| d.to_rfc3339()))
        .bind(cert.last_error.as_deref())
        .execute(&self.pool)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE") {
                RepoError::new("unique violation: domain already exists")
            } else {
                RepoError::new(msg)
            }
        })?;
        Ok(())
    }

    async fn update(&self, cert: &Certificate) -> Result<(), RepoError> {
        let res = sqlx::query(
            r#"
            UPDATE certificates SET
                source = ?, issuer = ?, valid_from = ?, valid_to = ?,
                key_type = ?, cert_pem = ?, chain_pem = ?,
                key_ciphertext = ?, force_https = ?,
                acme_endpoint = ?, renewed_at = ?, last_error = ?
            WHERE id = ?
            "#,
        )
        .bind(cert.source.as_str())
        .bind(&cert.issuer)
        .bind(cert.valid_from.to_rfc3339())
        .bind(cert.valid_to.to_rfc3339())
        .bind(cert.key_type.as_str())
        .bind(&cert.cert_pem)
        .bind(&cert.chain_pem)
        .bind(&cert.key_pem)
        .bind(cert.force_https)
        .bind(cert.acme_endpoint.as_deref())
        .bind(cert.renewed_at.map(|d| d.to_rfc3339()))
        .bind(cert.last_error.as_deref())
        .bind(cert.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(RepoError::new(format!(
                "certificate not found: {}",
                cert.id
            )));
        }
        Ok(())
    }

    async fn find_by_domain(&self, domain: &str) -> Result<Option<Certificate>, RepoError> {
        let row = sqlx::query("SELECT * FROM certificates WHERE domain = ?")
            .bind(domain)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_cert).transpose()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Certificate>, RepoError> {
        let row = sqlx::query("SELECT * FROM certificates WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_cert).transpose()
    }

    async fn list(&self) -> Result<Vec<Certificate>, RepoError> {
        let rows = sqlx::query("SELECT * FROM certificates ORDER BY domain")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_cert).collect()
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM certificates WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }
}

fn row_to_cert(row: sqlx::sqlite::SqliteRow) -> Result<Certificate, RepoError> {
    let id_str: String = row
        .try_get("id")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let id = Uuid::parse_str(&id_str).map_err(|e| RepoError::new(e.to_string()))?;
    let domain: String = row
        .try_get("domain")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let source_str: String = row
        .try_get("source")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let issuer: String = row
        .try_get("issuer")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let valid_from_str: String = row
        .try_get("valid_from")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let valid_to_str: String = row
        .try_get("valid_to")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let valid_from = DateTime::parse_from_rfc3339(&valid_from_str)
        .map_err(|e| RepoError::new(e.to_string()))?
        .with_timezone(&Utc);
    let valid_to = DateTime::parse_from_rfc3339(&valid_to_str)
        .map_err(|e| RepoError::new(e.to_string()))?
        .with_timezone(&Utc);
    let key_type_str: String = row
        .try_get("key_type")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let cert_pem: String = row
        .try_get("cert_pem")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let chain_pem: String = row
        .try_get("chain_pem")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let key_pem: Vec<u8> = row
        .try_get("key_ciphertext")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let force_https: i64 = row
        .try_get("force_https")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let acme_endpoint: Option<String> = row
        .try_get("acme_endpoint")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let created_at = DateTime::parse_from_rfc3339(
        &row.try_get::<String, _>("created_at")
            .map_err(|e| RepoError::new(e.to_string()))?,
    )
    .map_err(|e| RepoError::new(e.to_string()))?
    .with_timezone(&Utc);
    let renewed_at = row
        .try_get::<Option<String>, _>("renewed_at")
        .map_err(|e| RepoError::new(e.to_string()))?
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| RepoError::new(e.to_string()))
        })
        .transpose()?;
    let last_error: Option<String> = row
        .try_get("last_error")
        .map_err(|e| RepoError::new(e.to_string()))?;
    let last_attempt_at = row
        .try_get::<Option<String>, _>("last_attempt_at")
        .map_err(|e| RepoError::new(e.to_string()))?
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| RepoError::new(e.to_string()))
        })
        .transpose()?;

    Ok(Certificate {
        id,
        domain,
        source: parse_source(&source_str),
        issuer,
        valid_from,
        valid_to,
        key_type: parse_key_type(&key_type_str),
        cert_pem,
        chain_pem,
        key_pem,
        force_https: force_https != 0,
        acme_endpoint,
        created_at,
        renewed_at,
        last_error,
        last_attempt_at,
    })
}

// Silence unused-helper warning when only used in trait method bodies
#[allow(dead_code)]
fn _unused_helper() {}
