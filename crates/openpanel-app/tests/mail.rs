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
        Arc::new(openpanel_app::mail_filtering::queue::NullQueueAdapter),
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
        Arc::new(openpanel_app::mail_filtering::queue::NullQueueAdapter),
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
        Arc::new(openpanel_app::mail_filtering::queue::NullQueueAdapter),
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
        Arc::new(openpanel_app::mail_filtering::queue::NullQueueAdapter),
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

mod surface_leak_prop {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::sync::{Arc, Mutex};

    use openpanel_app::mail_filtering::{
        MailFilterService, SieveCompiler, SqliteMailFilterRepository,
    };
    use openpanel_core::AuditEvent;
    use openpanel_domain::{
        Email, Password, Role, SIEVE_MAX_BYTES, User, Username,
        mail_filtering::{AutoResponder, AutoResponderMode, SieveScript},
    };
    use openpanel_test_support::{MockAudit, TestDb};
    use proptest::prelude::*;
    use uuid::Uuid;

    type CapturedEvents = Arc<Mutex<Vec<AuditEvent>>>;

    async fn service_with_capturing_audit() -> (MailFilterService, CapturedEvents) {
        let db = TestDb::new().await;
        sqlx::raw_sql(openpanel_app::migrations::MAIL_FILTERING_V001)
            .execute(&db.pool())
            .await
            .unwrap();
        let events: CapturedEvents = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let mut audit = MockAudit::new();
        audit.expect_record().returning(move |event| {
            sink.lock().unwrap().push(event);
            Ok(())
        });
        let service = MailFilterService::new(
            Arc::new(SqliteMailFilterRepository::new(db.pool())),
            Arc::new(audit),
            SieveCompiler::new(),
        );
        (service, events)
    }

    fn owner() -> User {
        User::new(
            Uuid::new_v4(),
            Username::new("owner").unwrap(),
            Email::new("owner@example.test").unwrap(),
            Password::hash("correct horse battery staple").unwrap(),
            Role::Owner,
        )
    }

    /// Everything the audit log persisted for these calls, flattened
    /// to one searchable string.
    fn audit_transcript(events: &[AuditEvent]) -> String {
        events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn prop_surface_and_audit_never_leak() {
        // Fast argon2/bcrypt costs for the fixture owner.
        openpanel_domain::Password::set_test_costs(8, 1);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (service, events) = runtime.block_on(service_with_capturing_audit());
        let service = Arc::new(service);
        let owner = owner();

        let strategy = (
            "[A-Z0-9]{16,32}",
            "[a-zA-Z0-9 .,!?]{8,512}",
            "[a-zA-Z0-9]{8,64}",
        );
        proptest::test_runner::TestRunner::new(ProptestConfig::with_cases(100))
            .run(&strategy, |(marker, body, filler)| {
                // 1. Oversize Sieve scripts are rejected at the gate:
                // never constructed, never stored, never echoed.
                let oversize = format!("{{ {marker} {} }}", "x".repeat(SIEVE_MAX_BYTES + 1));
                assert!(SieveScript::new(Uuid::new_v4(), oversize).is_err());

                // 2. A valid script is stored; the audit event carries
                // only a byte count — no script fragment.
                let script_text =
                    format!("require [\"fileinto\"]; # {marker}\n{filler} {{ keep :all; }}");
                let script = SieveScript::new(Uuid::new_v4(), script_text.clone()).unwrap();
                let saved = runtime.block_on(service.set_sieve(&owner, script)).unwrap();
                assert_eq!(saved.script, script_text);
                let transcript = audit_transcript(&events.lock().unwrap());
                prop_assert!(!transcript.contains(&marker));
                prop_assert!(!transcript.contains(&filler));

                // 3. An autoresponder body is returned to its caller
                // but never reaches the audit transcript.
                let responder = AutoResponder {
                    mailbox_id: Uuid::new_v4(),
                    enabled: true,
                    body: body.clone(),
                    mode: AutoResponderMode::Once,
                    window_start: chrono::Utc::now(),
                    window_end: chrono::Utc::now() + chrono::Duration::days(1),
                };
                let echoed = runtime
                    .block_on(service.set_autoresponder(&owner, responder))
                    .unwrap();
                assert_eq!(echoed.body, body);
                let transcript = audit_transcript(&events.lock().unwrap());
                prop_assert!(!transcript.contains(&body));
                Ok(())
            })
            .unwrap();
    }
}
