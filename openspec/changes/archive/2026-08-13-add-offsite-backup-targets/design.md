# Add offsite backup targets — Design

## Adapters

```rust
#[async_trait]
pub trait BackupTargetAdapter: Send + Sync {
    type Error;
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Self::Error>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, Self::Error>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>, Self::Error>;
    async fn delete(&self, key: &str) -> Result<(), Self::Error>;
    async fn test(&self) -> Result<(), Self::Error>;
}
```

Concrete adapters (`S3Adapter`, `WasabiAdapter`, `B2Adapter`,
`RsyncAdapter`) implement the trait; each is a thin layer over
existing Rust SDKs (`aws-sdk-s3`, `b2-sdk`, `ssh2`).

## Encryption

```
kek = argon2id(passphrase, salt=master_key_fingerprint, …)
key = AES-256-GCM(kek, payload)
        out = hex(nonce) ":" hex(ciphertext)
```

The KEK is generated once at panel init; the passphrase is set
under a separate confirmation prompt in the disaster recovery
flow. The KEK is wrapped with the master key in a dedicated
`backup_kek_wrappers` table so the panel can decrypt without
the passphrase when the operator is unavailable, while keeping
the payload decryptable from a cold backup with just the
passphrase.

## Endpoints

```
POST   /api/v1/backups/credentials
        body: { kind, label, payload }
        → 201 { id, label, kind }            (no secrets echoed)

GET    /api/v1/backups/credentials
DELETE /api/v1/backups/credentials/{id}      refuses if attached to a plan

POST   /api/v1/backups/plans/{id}/remote-config
        body: { credential_id, prefix, schedule? }

POST   /api/v1/backups/remote/test
        body: { credential_id, prefix }
        → 200 { reachable: bool, latency_ms }
```

## CLI

```
openpanel backups remote create  --kind s3  --label <l> \
       --access-key <ak> --secret-key <sk> --bucket <b> --region <r>
openpanel backups remote test    --credential-id <id> --prefix <p>
openpanel backups remote list
openpanel backups remote delete  --credential-id <id> --force
```

## Tests

```
1.1  Unit: KEK derive-then-decrypt round-trip; passphrase
      mismatch errors; backup payload bytes are zeroized.
1.2  Property: encrypted blob round-trips with KEK; tampering
      with nonce or ciphertext causes AES-GCM tag mismatch.
1.3  Service tests with mock adapters: write, list, get, delete,
      test; credential refused when in use.
1.4  Integration: full plan + remote-config round-trip; restore
      against offsite target.
1.5  CLI E2E: create a plan with s3 target via `openpanel backups
      remote create`; restore command pulls from the bucket.
1.6  Web: remote target wizard with CSRF and confirmation.
```
