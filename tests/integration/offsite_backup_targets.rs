//! Offsite backup targets integration tests: encrypted credential
//! lifecycle and remote target attachment against the real
//! SQLite-backed service.

use openpanel_app::{derive_kek, encrypt_payload, master_key_fingerprint, unwrap_kek};
use openpanel_domain::{
    BackupTargetAdapter, CredentialKind, OffsiteBackupError, RemoteTargetConfig,
};
use uuid::Uuid;

use crate::common::*;

/// In-memory adapter for tests; exercises the trait through the
/// real service.
struct MemoryAdapter {
    objects: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

#[async_trait::async_trait]
impl BackupTargetAdapter for MemoryAdapter {
    type Error = OffsiteBackupError;

    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        self.objects
            .lock()
            .unwrap()
            .insert(key.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, Self::Error> {
        self.objects
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or(OffsiteBackupError::CredentialNotFound)
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, Self::Error> {
        let mut keys = self
            .objects
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        Ok(keys)
    }

    async fn delete(&self, key: &str) -> Result<(), Self::Error> {
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }

    async fn test(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[tokio::test]
async fn credential_lifecycle_and_remote_config_round_trip() {
    let server = TestServer::new().await;
    let service = server.offsite_backup_targets();

    // Create a credential: the secret is encrypted at rest.
    let credential = service
        .create_credential("admin", CredentialKind::S3, "prod-bucket", "AKIA:secret")
        .await
        .expect("create");
    assert!(!credential.secret_enc().contains("AKIA:secret"));

    // The KEK round-trips the plaintext.
    let decrypted = service
        .decrypt_credential(credential.id())
        .await
        .expect("decrypt");
    assert_eq!(decrypted.payload(), "AKIA:secret");

    // Listing never echoes secrets.
    let listed = service.list_credentials().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].secret_enc().contains("AKIA:secret"));

    // Attach a remote target to a plan, then delete is refused.
    let plan_id = Uuid::new_v4();
    let config = RemoteTargetConfig::new(
        plan_id,
        credential.id(),
        "host1/backups",
        Some("0 3 * * *".to_string()),
    )
    .expect("config");
    service
        .attach_remote_target("admin", &config)
        .await
        .expect("attach");
    let err = service
        .delete_credential("admin", credential.id())
        .await
        .expect_err("in use");
    assert_eq!(err, OffsiteBackupError::CredentialInUse(credential.id()));

    // Upload / download / list through the adapter.
    let adapter = MemoryAdapter {
        objects: std::sync::Mutex::new(std::collections::HashMap::new()),
    };
    service
        .upload(&adapter, "host1/site.tar.gz", b"payload")
        .await
        .expect("upload");
    let keys = service
        .list_objects(&adapter, "host1/")
        .await
        .expect("list objects");
    assert_eq!(keys, vec!["host1/site.tar.gz".to_string()]);
    let bytes = service
        .download(&adapter, "host1/site.tar.gz")
        .await
        .expect("download");
    assert_eq!(bytes, b"payload");

    // Reachability probe reports success.
    let probe = service
        .test_remote("admin", credential.id(), &adapter)
        .await
        .expect("probe");
    assert!(probe.reachable());
}

#[tokio::test]
async fn kek_derivation_and_wrapping_match_offsite_semantics() {
    // A cold restore re-derives the KEK from the passphrase + the
    // master-key fingerprint, without the master key itself.
    let master_key = [0x42u8; 32];
    let fingerprint = master_key_fingerprint(&master_key);
    let passphrase = b"correct horse battery staple";
    let kek = derive_kek(passphrase, &fingerprint).expect("derive");

    // Payload encrypted under the KEK decrypts with only the KEK.
    let stored = encrypt_payload(&kek, "AKIA:secret").expect("encrypt");
    let pt = openpanel_app::decrypt_payload(&kek, &stored).expect("decrypt");
    assert_eq!(pt, "AKIA:secret");

    // The KEK can be wrapped for at-rest panel use and unwrapped.
    let wrapped = openpanel_app::wrap_kek(&master_key, &kek).expect("wrap");
    let unwrapped = unwrap_kek(&master_key, &wrapped).expect("unwrap");
    assert_eq!(unwrapped, kek);
}
