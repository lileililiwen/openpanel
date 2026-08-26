//! Deliverability service integration tests.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::Arc;

use async_trait::async_trait;
use openpanel_app::DeliverabilityService;
use openpanel_domain::deliverability::{BlocklistZone, ResolverPort};
use openpanel_test_support::{MockAudit, TestDb};

struct FakeResolver {
    listed: Vec<String>,
}

#[async_trait]
impl ResolverPort for FakeResolver {
    async fn txt(&self, _name: &str) -> Result<Vec<String>, String> {
        Ok(vec![])
    }

    async fn resolve_a(&self, name: &str) -> Result<Vec<std::net::IpAddr>, String> {
        if self.listed.iter().any(|n| n == name) {
            let ip: std::net::IpAddr = "127.0.0.2".parse().expect("valid ip");
            Ok(vec![ip])
        } else {
            Ok(vec![])
        }
    }
}

const REPORT: &str = r#"<feedback><record><row><source_ip>203.0.113.7</source_ip><count>5</count><policy_evaluated><dkim>pass</dkim><spf>pass</spf></policy_evaluated></row></record></feedback>"#;

#[tokio::test]
async fn check_ip_upserts_and_ingest_prunes() {
    let db = TestDb::new().await;
    sqlx::raw_sql(openpanel_app::migrations::DELIVERABILITY_V001)
        .execute(&db.pool())
        .await
        .unwrap();
    let resolver = Arc::new(FakeResolver {
        listed: vec!["5.2.0.192.zen.spamhaus.org".to_string()],
    });
    let zones = vec![BlocklistZone::new("zen.spamhaus.org").unwrap()];
    let svc = DeliverabilityService::new(
        db.pool(),
        Arc::new(MockAudit::stub()),
        resolver.clone(),
        zones,
    );
    let owner = test_owner();

    // Listed address → true, row persisted.
    assert!(
        svc.check_ip(&owner, "192.0.2.5".parse().unwrap())
            .await
            .unwrap()
    );

    // Ingest stores one stat row.
    assert_eq!(svc.ingest_report(REPORT).await.unwrap(), 1);
    let sources = svc.sources(90).await.unwrap();
    assert_eq!(sources.len(), 1);

    // 90-day prune: an aged row disappears on next ingest.
    sqlx::query("UPDATE dmarc_source_stats SET day = ?")
        .bind(
            (chrono::Utc::now() - chrono::Duration::days(91))
                .date_naive()
                .to_string(),
        )
        .execute(&db.pool())
        .await
        .unwrap();
    svc.ingest_report(REPORT).await.unwrap();
    // The aged row was pruned; only the fresh ingest remains.
    let sources = svc.sources(90).await.unwrap();
    assert_eq!(sources.len(), 1);
}

fn test_owner() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Role, User, Username};
    User::new(
        uuid::Uuid::new_v4(),
        Username::new("owner").unwrap(),
        Email::new("owner@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    )
}
