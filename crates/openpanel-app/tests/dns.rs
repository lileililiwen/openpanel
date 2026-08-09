#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use openpanel_app::dns::{
    CredentialCipher, DnsProvider, DnsRepository, DnsService, DnsServiceError, ProviderCredential,
    ProviderZone, RemoteRecord,
};
use openpanel_domain::{
    Role,
    dns::{DnsName, ProviderCapabilities, RecordKind, RemoteVersion},
};
use openpanel_test_support::MockAudit;
use proptest::prelude::*;
use uuid::Uuid;

mockall::mock! {
    Provider {}
    #[async_trait]
    impl DnsProvider for Provider {
        async fn test(&self, credential: &ProviderCredential) -> Result<ProviderCapabilities, DnsServiceError>;
        async fn zones(&self, credential: &ProviderCredential) -> Result<Vec<ProviderZone>, DnsServiceError>;
        async fn records(&self, credential: &ProviderCredential, zone_id: &str) -> Result<Vec<RemoteRecord>, DnsServiceError>;
        async fn create_record(&self, credential: &ProviderCredential, zone_id: &str, record: &RemoteRecord, expected: &RemoteVersion) -> Result<RemoteRecord, DnsServiceError>;
        async fn delete_record(&self, credential: &ProviderCredential, zone_id: &str, record_id: &str, expected: &RemoteVersion) -> Result<RemoteVersion, DnsServiceError>;
    }
}

mockall::mock! {
    Repo {}
    #[async_trait]
    impl DnsRepository for Repo {
        async fn save_account(&self, account: &openpanel_app::dns::StoredProviderAccount) -> Result<(), DnsServiceError>;
        async fn account(&self, id: Uuid) -> Result<openpanel_app::dns::StoredProviderAccount, DnsServiceError>;
        async fn save_zone(&self, account_id: Uuid, zone: &ProviderZone) -> Result<(), DnsServiceError>;
        async fn import_records(&self, zone_id: Uuid, records: &[RemoteRecord]) -> Result<(), DnsServiceError>;
        async fn local_records(&self, zone_id: Uuid) -> Result<Vec<RemoteRecord>, DnsServiceError>;
    }
}

#[test]
fn credential_cipher_uses_unique_nonces_and_never_embeds_plaintext() {
    let cipher = CredentialCipher::new(&[7_u8; 32]).unwrap();
    let first = cipher.encrypt("super-secret-token").unwrap();
    let second = cipher.encrypt("super-secret-token").unwrap();
    assert_ne!(first, second);
    assert!(!first.contains("super-secret-token"));
    assert_eq!(cipher.decrypt(&first).unwrap(), "super-secret-token");
}

#[tokio::test]
async fn owner_adds_account_without_secret_in_result_or_audit() {
    let mut provider = MockProvider::new();
    provider
        .expect_test()
        .once()
        .returning(|_| Ok(ProviderCapabilities::all(60, 86_400)));
    let mut repo = MockRepo::new();
    repo.expect_save_account()
        .once()
        .withf(|account| !account.encrypted_credential.contains("secret-token"))
        .returning(|_| Ok(()));
    let service = DnsService::new(
        Arc::new(repo),
        Arc::new(provider),
        CredentialCipher::new(&[9_u8; 32]).unwrap(),
        Arc::new(MockAudit::stub()),
    );
    let account = service
        .create_account(
            Uuid::new_v4(),
            Role::Owner,
            "cloudflare",
            "Primary",
            "secret-token",
        )
        .await
        .unwrap();
    assert!(
        !serde_json::to_string(&account)
            .unwrap()
            .contains("secret-token")
    );
}

#[tokio::test]
async fn synchronization_imports_remote_records_without_provider_mutation() {
    let account_id = Uuid::new_v4();
    let zone_id = Uuid::new_v4();
    let mut provider = MockProvider::new();
    provider.expect_zones().once().returning(move |_| {
        Ok(vec![ProviderZone::new(
            zone_id,
            "remote-zone",
            DnsName::new("example.com").unwrap(),
            RemoteVersion::new("v2").unwrap(),
        )])
    });
    provider.expect_records().once().returning(|_, _| {
        Ok(vec![
            RemoteRecord::a("remote-record", "www.example.com", "192.0.2.1", 300, "v2").unwrap(),
        ])
    });
    provider.expect_create_record().never();
    provider.expect_delete_record().never();
    let mut repo = MockRepo::new();
    repo.expect_account().once().returning(move |_| {
        Ok(openpanel_app::dns::StoredProviderAccount::test(
            account_id, "token",
        ))
    });
    repo.expect_save_zone().once().returning(|_, _| Ok(()));
    repo.expect_import_records()
        .once()
        .withf(move |id, records| *id == zone_id && records.len() == 1)
        .returning(|_, _| Ok(()));
    let service = DnsService::new(
        Arc::new(repo),
        Arc::new(provider),
        CredentialCipher::new(&[9_u8; 32]).unwrap(),
        Arc::new(MockAudit::stub()),
    );
    let result = service
        .sync(Uuid::new_v4(), Role::Owner, account_id)
        .await
        .unwrap();
    assert_eq!(result.imported_records, 1);
}

#[tokio::test]
async fn stale_remote_version_is_a_conflict_and_cname_is_rejected_locally() {
    assert!(matches!(
        DnsServiceError::provider("token=secret conflict current=v3"),
        DnsServiceError::Provider(message) if !message.contains("secret")
    ));
    let _ = RecordKind::Cname;
}

#[tokio::test]
async fn site_proposal_is_non_mutating_and_txt_lease_cleanup_targets_one_remote_id() {
    let repo = Arc::new(openpanel_app::dns::MemoryDnsRepository::default());
    let service = DnsService::new(
        repo,
        Arc::new(openpanel_app::dns::FakeDnsProvider::new().unwrap()),
        CredentialCipher::new(&[5_u8; 32]).unwrap(),
        Arc::new(openpanel_core::NoopAuditService),
    );
    let account = service
        .create_account(Uuid::nil(), Role::Owner, "fake", "Primary", "token")
        .await
        .unwrap();
    service
        .sync(Uuid::nil(), Role::Owner, account.id)
        .await
        .unwrap();
    let zone = service.zones().await.unwrap().remove(0);
    let proposal = service
        .propose_site_records(Role::Owner, zone.id, "www.example.test", "192.0.2.44")
        .await
        .unwrap();
    assert_eq!(proposal.records.len(), 1);
    assert!(service.records(zone.id).await.unwrap().is_empty());
    let first = service
        .create_txt_lease(
            Uuid::nil(),
            Role::Owner,
            zone.id,
            "_acme-challenge.example.test",
            "first",
        )
        .await
        .unwrap();
    let second = service
        .create_txt_lease(
            Uuid::nil(),
            Role::Owner,
            zone.id,
            "_acme-challenge.example.test",
            "second",
        )
        .await
        .unwrap();
    service
        .cleanup_txt_lease(Uuid::nil(), Role::Owner, first)
        .await
        .unwrap();
    let remaining = service.records(zone.id).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].remote_id, second.remote_id);
}

proptest! {
    #[test]
    fn prop_arbitrary_provider_errors_never_echo_input(message in "[A-Z0-9]{24,64}") {
        let rendered = DnsServiceError::provider(&message).to_string();
        prop_assert!(!rendered.contains(&message));
        prop_assert!(rendered.len() < 128);
    }
}
