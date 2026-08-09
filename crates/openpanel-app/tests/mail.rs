#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{fs, sync::Arc};

use async_trait::async_trait;
use openpanel_app::mail::{
    BackupHook, DkimKeyCustody, FilesystemMailConfigurator, MailConfigControl, MailConfigurator,
    MailRepository, MailService, MailServiceError, MemoryMailRepository, Readiness, ReadinessPort,
};
use openpanel_domain::{
    Role,
    mail::{MailAddress, MailDomainName, MailQuota},
};
use openpanel_test_support::MockAudit;
use uuid::Uuid;

mockall::mock! {Repo{}
#[async_trait]
impl MailRepository for Repo{
 async fn save_domain(&self,domain:&openpanel_app::mail::MailDomain)->Result<(),MailServiceError>;
 async fn domain(&self,id:Uuid)->Result<openpanel_app::mail::MailDomain,MailServiceError>;
 async fn save_mailbox(&self,mailbox:&openpanel_app::mail::StoredMailbox)->Result<(),MailServiceError>;
 async fn mailboxes(&self,domain_id:Uuid)->Result<Vec<openpanel_app::mail::StoredMailbox>,MailServiceError>;
}}
mockall::mock! {Config{}
#[async_trait]
impl MailConfigurator for Config{async fn validate_apply_reload(&self,domain:&openpanel_app::mail::MailDomain)->Result<(),MailServiceError>;}}
mockall::mock! {Ready{}
#[async_trait]
impl ReadinessPort for Ready{async fn check(&self,domain:&MailDomainName)->Result<Readiness,MailServiceError>;}}
mockall::mock! {Backup{}
#[async_trait]
impl BackupHook for Backup{async fn register_domain(&self,domain_id:Uuid)->Result<(),MailServiceError>;}}
mockall::mock! {Control{}
#[async_trait]
impl MailConfigControl for Control{
 async fn validate(&self,postfix:&std::path::Path,dovecot:&std::path::Path)->Result<(),MailServiceError>;
 async fn reload(&self)->Result<(),MailServiceError>;
}}

#[tokio::test]
async fn missing_mx_blocks_enable_without_config_reload() {
    let mut repo = MockRepo::new();
    let id = Uuid::new_v4();
    repo.expect_domain().once().returning(move |_| {
        Ok(openpanel_app::mail::MailDomain::test(
            id,
            "example.com",
            false,
        ))
    });
    let mut ready = MockReady::new();
    ready
        .expect_check()
        .once()
        .returning(|_| Ok(Readiness::missing_mx("MX 10 mail.example.com")));
    let mut config = MockConfig::new();
    config.expect_validate_apply_reload().never();
    let service = MailService::new(
        Arc::new(repo),
        Arc::new(config),
        Arc::new(ready),
        Arc::new(MockBackup::new()),
        Arc::new(MockAudit::stub()),
    );
    assert!(matches!(
        service
            .enable_domain(Uuid::nil(), Role::Owner, id, false)
            .await,
        Err(MailServiceError::NotReady(_))
    ));
}

#[tokio::test]
async fn mailbox_password_is_returned_once_and_never_stored_or_serialized() {
    let id = Uuid::new_v4();
    let mut repo = MockRepo::new();
    repo.expect_domain().once().returning(move |_| {
        Ok(openpanel_app::mail::MailDomain::test(
            id,
            "example.com",
            true,
        ))
    });
    repo.expect_mailboxes().once().returning(|_| Ok(vec![]));
    repo.expect_save_mailbox()
        .once()
        .withf(|mailbox| !mailbox.password_hash.contains("generated-"))
        .returning(|_| Ok(()));
    let service = MailService::new(
        Arc::new(repo),
        Arc::new(MockConfig::new()),
        Arc::new(MockReady::new()),
        Arc::new(MockBackup::new()),
        Arc::new(MockAudit::stub()),
    );
    let created = service
        .create_mailbox(
            Uuid::nil(),
            Role::Owner,
            id,
            "alice",
            MailQuota::new(1_048_576, 1024, 1_073_741_824).unwrap(),
            Some("generated-strong-password"),
        )
        .await
        .unwrap();
    assert_eq!(created.password, "generated-strong-password");
    assert!(
        !serde_json::to_string(&created.mailbox)
            .unwrap()
            .contains("password")
    );
    let _ = MailAddress::parse("alice@example.com").unwrap();
}

#[tokio::test]
async fn destructive_domain_delete_requires_fresh_scoped_token_and_reports_dependencies() {
    let repo = Arc::new(MemoryMailRepository::default());
    let mut backup = MockBackup::new();
    backup.expect_register_domain().once().returning(|_| Ok(()));
    let service = MailService::new(
        repo,
        Arc::new(MockConfig::new()),
        Arc::new(MockReady::new()),
        Arc::new(backup),
        Arc::new(MockAudit::stub()),
    );
    let actor = Uuid::new_v4();
    let domain = service
        .create_domain(actor, Role::Owner, "example.com")
        .await
        .unwrap();
    service
        .create_mailbox(
            actor,
            Role::Owner,
            domain.id,
            "alice",
            MailQuota::new(1_048_576, 1024, 1_073_741_824).unwrap(),
            Some("generated-strong-password"),
        )
        .await
        .unwrap();
    let preview = service
        .preview_delete_domain(actor, Role::Owner, domain.id)
        .await
        .unwrap();
    assert_eq!(preview.mailboxes, 1);
    assert_eq!(preview.aliases, 0);
    assert!(
        service
            .delete_domain(actor, Role::Owner, domain.id, "wrong")
            .await
            .is_err()
    );
    service
        .delete_domain(actor, Role::Owner, domain.id, &preview.confirmation_token)
        .await
        .unwrap();
    assert!(
        service
            .domains(actor, Role::Owner)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .delete_domain(actor, Role::Owner, domain.id, &preview.confirmation_token)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn users_can_manage_only_mailboxes_in_their_own_domain() {
    let repo = Arc::new(MemoryMailRepository::default());
    let mut backup = MockBackup::new();
    backup.expect_register_domain().once().returning(|_| Ok(()));
    let service = MailService::new(
        repo,
        Arc::new(MockConfig::new()),
        Arc::new(MockReady::new()),
        Arc::new(backup),
        Arc::new(MockAudit::stub()),
    );
    let owner = Uuid::new_v4();
    let domain = service
        .create_domain(owner, Role::Owner, "owned.example")
        .await
        .unwrap();
    let mailbox = service
        .create_mailbox(
            owner,
            Role::User,
            domain.id,
            "alice",
            MailQuota::new(1_048_576, 1024, 1_073_741_824).unwrap(),
            Some("generated-strong-password"),
        )
        .await
        .unwrap();
    let disabled = service
        .set_mailbox_enabled(owner, Role::User, mailbox.mailbox.address.as_str(), false)
        .await
        .unwrap();
    assert!(!disabled.enabled);
    let alias = service
        .add_alias(
            owner,
            Role::User,
            domain.id,
            "info",
            mailbox.mailbox.address.as_str(),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .aliases(owner, Role::User, domain.id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        service
            .delete_alias(owner, Role::User, alias.id, false)
            .await
            .is_err()
    );
    service
        .delete_alias(owner, Role::User, alias.id, true)
        .await
        .unwrap();
    assert!(
        service
            .delete_mailbox(owner, Role::User, mailbox.mailbox.address.as_str(), false)
            .await
            .is_err()
    );
    service
        .delete_mailbox(owner, Role::User, mailbox.mailbox.address.as_str(), true)
        .await
        .unwrap();
    assert!(
        service
            .mailboxes(owner, Role::User, domain.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .create_mailbox(
                Uuid::new_v4(),
                Role::User,
                domain.id,
                "mallory",
                MailQuota::new(1_048_576, 1024, 1_073_741_824).unwrap(),
                Some("generated-strong-password"),
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn invalid_candidate_keeps_active_includes_and_never_reloads() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("postfix-openpanel.cf"), "active-postfix").unwrap();
    fs::write(root.path().join("dovecot-openpanel.conf"), "active-dovecot").unwrap();
    let mut control = MockControl::new();
    control
        .expect_validate()
        .once()
        .returning(|_, _| Err(MailServiceError::Configuration));
    control.expect_reload().never();
    let adapter = FilesystemMailConfigurator::new(root.path(), Arc::new(control));
    assert!(
        adapter
            .validate_apply_reload(&openpanel_app::mail::MailDomain::test(
                Uuid::new_v4(),
                "example.com",
                false,
            ))
            .await
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(root.path().join("postfix-openpanel.cf")).unwrap(),
        "active-postfix"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("dovecot-openpanel.conf")).unwrap(),
        "active-dovecot"
    );
}

#[tokio::test]
async fn reload_failure_rolls_back_both_active_includes() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("postfix-openpanel.cf"), "active-postfix").unwrap();
    fs::write(root.path().join("dovecot-openpanel.conf"), "active-dovecot").unwrap();
    let mut control = MockControl::new();
    control.expect_validate().once().returning(|_, _| Ok(()));
    let mut sequence = mockall::Sequence::new();
    control
        .expect_reload()
        .once()
        .in_sequence(&mut sequence)
        .returning(|| Err(MailServiceError::Configuration));
    control
        .expect_reload()
        .once()
        .in_sequence(&mut sequence)
        .returning(|| Ok(()));
    let adapter = FilesystemMailConfigurator::new(root.path(), Arc::new(control));
    assert!(
        adapter
            .validate_apply_reload(&openpanel_app::mail::MailDomain::test(
                Uuid::new_v4(),
                "example.com",
                false,
            ))
            .await
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(root.path().join("postfix-openpanel.cf")).unwrap(),
        "active-postfix"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("dovecot-openpanel.conf")).unwrap(),
        "active-dovecot"
    );
}

#[test]
fn dkim_private_key_is_encrypted_at_rest_and_materialized_mode_0600() {
    use std::os::unix::fs::PermissionsExt;

    let custody = DkimKeyCustody::new(&[0x42; 32]).unwrap();
    let private = "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----";
    let encrypted = custody.encrypt(private).unwrap();
    assert!(!encrypted.contains("secret"));
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("mail.private");
    custody.materialize(&encrypted, &path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), private);
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
