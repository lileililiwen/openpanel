# Add container runtime — Design

## Quota

```rust
pub struct ContainerQuota {
    pub owner_id: UserId,
    pub max_concurrent: u32,           // default 8
    pub max_total: u32,                // default 50
    pub cpu_pct_max: u8,               // 0..=100; default 50
    pub memory_bytes_max: u64,         // default 1GiB per container
    pub egress_bytes_per_month: u64,   // default 10GiB
}
```

```
can_create_container(owner):
  if current >= quota.max_total: false
  if current_concurrent >= quota.max_concurrent: false
  return true
```

Enforcement is in the docker management service; the quota
cap is read before every container create.

## Registry

```rust
pub struct RegistryCredential {
    pub id: RegistryCredentialId,
    pub owner_id: UserId,
    pub registry: String,               // "registry.example.com"
    pub username: String,
    pub password_ciphertext: Vec<u8>,   // encrypted under master key
    pub created_at, last_used_at, audit_meta,
}
```

The panel refuses weak passwords (≥ 16 chars with mixed
classes); plaintext passwords are returned exactly once.

## Egress

Egress accounting is implemented as a separate observer that
reads `NetworkEgressAccount{owner_id, container_id?, period,
bytes}` rows at month-rollover. Threshold crossings emit
`BandwidthThresholdCrossed` events that the bandwidth
accounting change already consumes.

## Endpoints

```
GET    /api/v1/container-quota                owner or principal
PUT    /api/v1/container-quota                Owners only; subject to plan
GET    /api/v1/containers/{id}/metrics        CPU/RAM/network/error counters
POST   /api/v1/registry/credentials           body: { registry, username, password }
GET    /api/v1/registry/credentials           list
DELETE /api/v1/registry/credentials/{id}
POST   /api/v1/containers/{id}/pull           body: { image_ref, registry_credential_id? }
PUT    /api/v1/containers/{id}/egress/limit   body: { bytes_per_month }
```

## CLI

```
openpanel container quota show
openpanel container quota set   --cpu 70 --memory 2GiB --egress 50GiB
openpanel container metrics     <container_id>
openpanel registry credentials add     --registry <r> --user <u>
openpanel registry credentials list
openpanel registry credentials remove  <id>
openpanel container pull        <container_id> --image <ref>
```

## Tests

```
1.1  Unit: quota math; egress accounting rotation;
      credential cipher round-trip.
1.2  Property: quota monotonic; plain passwords are zeroized
      after decryption; no plaintext in any audit.
1.3  Service tests with mock docker and mock adapter.
1.4  Integration: live docker fixture with quota enforced;
      pull against a test registry.
1.5  CLI E2E.
1.6  Web: /container/quota editor + /registry/credentials
      (CSRF).
```
