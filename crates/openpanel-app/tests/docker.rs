//! Docker service tests with in-memory ports.
#![allow(clippy::expect_used)]

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use openpanel_app::docker::{
    ApplyReport, BollardDockerAdapter, DockerAdapter, DockerService, ExecResult, NetworkAdapter,
    RuntimeContainerState, SqliteDockerRepository,
};
use openpanel_core::{AuditEvent, AuditService, CoreResult};
use openpanel_domain::{
    Email, Password, RepoError, Role, User, Username,
    docker::{
        ComposeStack, Container, ContainerSpec, DockerError, DockerRepository, ImageAllowlistEntry,
        ResourceLimits, RestartPolicy, StoredComposeStack,
    },
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

#[derive(Default)]
struct Repo {
    containers: Mutex<Vec<Container>>,
    allow: Mutex<Vec<ImageAllowlistEntry>>,
    stacks: Mutex<Vec<StoredComposeStack>>,
}
#[async_trait]
impl DockerRepository for Repo {
    async fn put_container(&self, value: &Container) -> Result<(), RepoError> {
        let mut rows = self
            .containers
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?;
        rows.retain(|row| row.spec.id != value.spec.id);
        rows.push(value.clone());
        Ok(())
    }

    async fn get_container(&self, id: Uuid) -> Result<Option<Container>, RepoError> {
        Ok(self
            .containers
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?
            .iter()
            .find(|row| row.spec.id == id)
            .cloned())
    }

    async fn list_containers(&self) -> Result<Vec<Container>, RepoError> {
        Ok(self
            .containers
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?
            .clone())
    }

    async fn delete_container(&self, id: Uuid) -> Result<(), RepoError> {
        self.containers
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?
            .retain(|row| row.spec.id != id);
        Ok(())
    }

    async fn allowlist(&self) -> Result<Vec<ImageAllowlistEntry>, RepoError> {
        Ok(self
            .allow
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?
            .clone())
    }

    async fn put_allowlist(&self, value: &ImageAllowlistEntry) -> Result<(), RepoError> {
        self.allow
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?
            .push(value.clone());
        Ok(())
    }

    async fn put_stack(&self, value: &StoredComposeStack) -> Result<(), RepoError> {
        let mut rows = self
            .stacks
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?;
        rows.retain(|row| row.stack.id != value.stack.id);
        rows.push(value.clone());
        Ok(())
    }

    async fn get_stack(&self, id: Uuid) -> Result<Option<StoredComposeStack>, RepoError> {
        Ok(self
            .stacks
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?
            .iter()
            .find(|row| row.stack.id == id)
            .cloned())
    }

    async fn list_stacks(&self) -> Result<Vec<StoredComposeStack>, RepoError> {
        Ok(self
            .stacks
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?
            .clone())
    }

    async fn delete_stack(&self, id: Uuid) -> Result<(), RepoError> {
        self.stacks
            .lock()
            .map_err(|e| RepoError::new(e.to_string()))?
            .retain(|row| row.stack.id != id);
        Ok(())
    }
}

#[derive(Default)]
struct Adapter {
    calls: Mutex<Vec<String>>,
    fail_create: bool,
    fail_pull: bool,
    oom: bool,
    stack_applies: Mutex<usize>,
    network_ensures: Mutex<usize>,
}
#[async_trait]
impl DockerAdapter for Adapter {
    async fn ping(&self) -> Result<(), DockerError> {
        Ok(())
    }

    async fn pull(&self, image: &str) -> Result<String, DockerError> {
        self.calls
            .lock()
            .map_err(|e| DockerError::Adapter(e.to_string()))?
            .push(format!("pull:{image}"));
        if self.fail_pull {
            return Err(DockerError::Adapter("pull failed".into()));
        }
        Ok("redis@sha256:digest".into())
    }

    async fn create(&self, _: &ContainerSpec) -> Result<String, DockerError> {
        self.calls
            .lock()
            .map_err(|e| DockerError::Adapter(e.to_string()))?
            .push("create".into());
        if self.fail_create {
            Err(DockerError::Adapter("failed".into()))
        } else {
            Ok("runtime-1".into())
        }
    }

    async fn start(&self, _: &str) -> Result<(), DockerError> {
        Ok(())
    }

    async fn stop(&self, _: &str) -> Result<(), DockerError> {
        Ok(())
    }

    async fn restart(&self, _: &str) -> Result<(), DockerError> {
        Ok(())
    }

    async fn remove(&self, _: &str, _: bool) -> Result<(), DockerError> {
        Ok(())
    }

    async fn inspect(&self, _: &str) -> Result<RuntimeContainerState, DockerError> {
        Ok(RuntimeContainerState {
            status: "running".into(),
            oom_killed: self.oom,
        })
    }

    async fn logs(&self, _: &str, _: u64) -> Result<Vec<String>, DockerError> {
        Ok(vec!["password=top-secret".into()])
    }

    async fn exec(&self, _: &str, _: &[String], user: &str) -> Result<ExecResult, DockerError> {
        if user.starts_with('0') {
            return Err(DockerError::Invalid("root".into()));
        }
        Ok(ExecResult {
            exit_code: 0,
            output: "top-secret".repeat(10_000),
            truncated: false,
        })
    }

    async fn apply_stack(&self, _: &ComposeStack) -> Result<ApplyReport, DockerError> {
        *self
            .stack_applies
            .lock()
            .map_err(|error| DockerError::Adapter(error.to_string()))? += 1;
        Ok(ApplyReport {
            created: Vec::new(),
            removed: Vec::new(),
        })
    }

    async fn remove_stack(&self, _: &ComposeStack) -> Result<Vec<String>, DockerError> {
        Ok(vec!["removed-runtime".into()])
    }
}

#[async_trait]
impl NetworkAdapter for Adapter {
    async fn ensure(&self, _: Option<Uuid>) -> Result<(), DockerError> {
        *self
            .network_ensures
            .lock()
            .map_err(|error| DockerError::Adapter(error.to_string()))? += 1;
        Ok(())
    }

    async fn remove_if_unused(&self, _: Option<Uuid>) -> Result<(), DockerError> {
        Ok(())
    }
}
#[derive(Default)]
struct Audit(Mutex<Vec<AuditEvent>>);
#[async_trait]
impl AuditService for Audit {
    async fn record(&self, event: AuditEvent) -> CoreResult<()> {
        self.0
            .lock()
            .map_err(|e| openpanel_core::CoreError::Migration(e.to_string()))?
            .push(event);
        Ok(())
    }

    async fn recent(&self, limit: i64) -> CoreResult<Vec<AuditEvent>> {
        Ok(self
            .0
            .lock()
            .map_err(|e| openpanel_core::CoreError::Migration(e.to_string()))?
            .iter()
            .rev()
            .take(limit.max(0) as usize)
            .cloned()
            .collect())
    }

    async fn query(
        &self,
        _: openpanel_core::audit::AuditQuery,
    ) -> CoreResult<openpanel_core::audit::AuditPage> {
        Ok(openpanel_core::audit::AuditPage {
            events: Vec::new(),
            next_cursor: None,
        })
    }
}

fn owner() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("owner").expect("username"),
        Email::new("owner@example.test").expect("email"),
        Password::hash("correct horse battery staple").expect("password"),
        Role::Owner,
    )
}
fn spec() -> ContainerSpec {
    ContainerSpec {
        id: Uuid::new_v4(),
        name: "redis".into(),
        image: "library/redis:7".into(),
        site_id: None,
        env: BTreeMap::from([("PASSWORD".into(), "top-secret".into())]),
        command: vec![],
        ports: vec![],
        mounts: vec![],
        capabilities: vec![],
        limits: ResourceLimits::default(),
        restart_policy: RestartPolicy::No,
        user_namespace: "1001:1001".into(),
    }
}
fn service(adapter: Arc<Adapter>) -> (DockerService, Arc<Repo>) {
    let repo = Arc::new(Repo::default());
    repo.allow.lock().expect("allow").push(ImageAllowlistEntry {
        pattern: "library/redis:*".into(),
        allow_pull: true,
        pin_digest_required: false,
    });
    (
        DockerService::new(
            repo.clone(),
            adapter.clone(),
            adapter,
            Arc::new(Audit::default()),
        ),
        repo,
    )
}

#[tokio::test]
async fn disallowed_pull_never_contacts_adapter() {
    let adapter = Arc::new(Adapter::default());
    let (svc, _) = service(adapter.clone());
    assert!(svc.pull(&owner(), "evil/image:latest").await.is_err());
    assert!(adapter.calls.lock().expect("calls").is_empty());
}
#[tokio::test]
async fn create_failure_never_persists() {
    let adapter = Arc::new(Adapter {
        fail_create: true,
        ..Default::default()
    });
    let (svc, repo) = service(adapter);
    assert!(svc.create(&owner(), spec()).await.is_err());
    assert!(repo.containers.lock().expect("rows").is_empty());
}
#[tokio::test]
async fn pull_failure_is_propagated_and_invalid_exec_is_rejected_before_adapter() {
    let adapter = Arc::new(Adapter {
        fail_pull: true,
        ..Default::default()
    });
    let (svc, _) = service(adapter.clone());
    assert!(svc.pull(&owner(), "library/redis:7").await.is_err());
    let container = svc.create(&owner(), spec()).await.expect("create");
    let before = adapter.calls.lock().expect("calls").len();
    assert!(
        svc.exec(&owner(), container.spec.id, Vec::new())
            .await
            .is_err()
    );
    assert_eq!(adapter.calls.lock().expect("calls").len(), before);
}
#[tokio::test]
async fn happy_path_redacts_and_bounds_outputs() {
    let adapter = Arc::new(Adapter::default());
    let (svc, _) = service(adapter);
    let container = svc.create(&owner(), spec()).await.expect("create");
    let logs = svc
        .logs(&owner(), container.spec.id, 10)
        .await
        .expect("logs");
    assert_eq!(logs[0], "password=[REDACTED]");
    let exec = svc
        .exec(&owner(), container.spec.id, vec!["echo".into()])
        .await
        .expect("exec");
    assert!(exec.truncated);
    assert!(!exec.output.contains("top-secret"));
}

#[tokio::test]
async fn oom_state_is_observed_and_persisted() {
    let adapter = Arc::new(Adapter {
        oom: true,
        ..Default::default()
    });
    let (svc, repo) = service(adapter);
    let container = svc.create(&owner(), spec()).await.expect("create");
    let observed = svc
        .action(&owner(), container.spec.id, "start")
        .await
        .expect("start");
    assert!(observed.oom_killed);
    assert!(repo.containers.lock().expect("rows")[0].oom_killed);
}
#[tokio::test]
async fn non_owner_is_rejected_before_adapter() {
    let adapter = Arc::new(Adapter::default());
    let (svc, _) = service(adapter.clone());
    let user = User::new(
        Uuid::new_v4(),
        Username::new("user").expect("username"),
        Email::new("user@example.test").expect("email"),
        Password::hash("correct horse battery staple").expect("password"),
        Role::User,
    );
    assert!(matches!(
        svc.create(&user, spec()).await,
        Err(DockerError::Forbidden)
    ));
    assert!(adapter.calls.lock().expect("calls").is_empty());
}

fn signed_stack() -> ComposeStack {
    signed_stack_for("cache", "library/redis:7")
}

fn signed_stack_for(name: &str, image: &str) -> ComposeStack {
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&format!("services:\n  redis:\n    image: {image}\n")).expect("yaml");
    let canonical = serde_yaml::to_string(&parsed).expect("canonical");
    let key = SigningKey::from_bytes(&[
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ]);
    ComposeStack {
        id: Uuid::new_v4(),
        name: name.into(),
        signature_b64: base64::engine::general_purpose::STANDARD
            .encode(key.sign(canonical.as_bytes()).to_bytes()),
        compose_yaml_canonical: canonical,
        site_id: None,
    }
}

#[tokio::test]
async fn real_bollard_reconciliation_is_idempotent_and_delete_cleans_runtime() {
    let docker = match bollard::Docker::connect_with_local_defaults() {
        Ok(docker) => docker,
        Err(_) => return,
    };
    let runtime = match BollardDockerAdapter::connect() {
        Ok(runtime) => Arc::new(runtime),
        Err(_) => return,
    };
    if runtime.ping().await.is_err() {
        return;
    }
    let repo = Arc::new(Repo::default());
    repo.allow.lock().expect("allow").push(ImageAllowlistEntry {
        pattern: "redis:*".into(),
        allow_pull: true,
        pin_digest_required: false,
    });
    let service = DockerService::new(repo, runtime.clone(), runtime, Arc::new(Audit::default()));
    service
        .pull(&owner(), "redis:7.0-alpine")
        .await
        .expect("pull allowlisted image");
    let name = format!("stack_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let stack = signed_stack_for(&name, "redis:7.0-alpine");
    let id = stack.id;
    service.put_stack(&owner(), stack).await.expect("put stack");
    let first = service
        .apply_stack(&owner(), id)
        .await
        .expect("first apply");
    let network = docker
        .inspect_network("openpanel-system", None)
        .await
        .expect("inspect isolated network");
    assert_eq!(network.internal, Some(true));
    let options = network.options.expect("network options");
    assert_eq!(
        options
            .get("com.docker.network.bridge.gateway_mode_ipv4")
            .map(String::as_str),
        Some("isolated")
    );
    assert_eq!(
        options
            .get("com.docker.network.bridge.enable_icc")
            .map(String::as_str),
        Some("true")
    );
    let second = service
        .apply_stack(&owner(), id)
        .await
        .expect("second apply");
    service
        .delete_stack(&owner(), id)
        .await
        .expect("delete stack");
    assert_eq!(first.created.len(), 1);
    assert!(second.created.is_empty());
    assert!(second.removed.is_empty());
}

#[tokio::test]
async fn signed_stack_is_persisted_applied_and_invalid_signature_is_rejected() {
    let adapter = Arc::new(Adapter::default());
    let (svc, repo) = service(adapter);
    let stack = signed_stack();
    let id = stack.id;
    svc.put_stack(&owner(), stack).await.expect("put stack");
    let report = svc.apply_stack(&owner(), id).await.expect("apply stack");
    assert!(report.created.is_empty());
    assert_eq!(repo.stacks.lock().expect("stacks")[0].status, "applied");
    svc.delete_stack(&owner(), id).await.expect("delete stack");
    assert!(repo.stacks.lock().expect("stacks").is_empty());
    let mut invalid = signed_stack();
    invalid.signature_b64 = "invalid".into();
    assert!(svc.put_stack(&owner(), invalid).await.is_err());
}

#[tokio::test]
async fn reconciliation_reapplies_only_applied_stacks() {
    let adapter = Arc::new(Adapter::default());
    let (svc, repo) = service(adapter.clone());
    let applied = signed_stack();
    let pending = signed_stack();
    repo.stacks.lock().expect("stacks").extend([
        StoredComposeStack {
            stack: applied,
            status: "applied".into(),
            last_applied_at: None,
        },
        StoredComposeStack {
            stack: pending,
            status: "pending".into(),
            last_applied_at: None,
        },
    ]);

    svc.reconcile_stacks().await.expect("reconcile");

    assert_eq!(*adapter.stack_applies.lock().expect("applies"), 1);
    assert_eq!(*adapter.network_ensures.lock().expect("networks"), 1);
    let rows = repo.stacks.lock().expect("stacks");
    assert!(
        rows.iter()
            .find(|row| row.status == "applied")
            .expect("applied")
            .last_applied_at
            .is_some()
    );
    assert!(
        rows.iter()
            .find(|row| row.status == "pending")
            .expect("pending")
            .last_applied_at
            .is_none()
    );
}

#[tokio::test]
async fn sqlite_repository_encrypts_environment_and_stack_documents_at_rest() {
    let db = TestDb::new().await;
    sqlx::raw_sql(openpanel_app::migrations::DOCKER_V001)
        .execute(&db.pool())
        .await
        .expect("docker migration");
    let repo = SqliteDockerRepository::new(db.pool(), [7u8; 32]);
    let container = Container {
        spec: spec(),
        runtime_id: Some("runtime".into()),
        status: "created".into(),
        oom_killed: false,
        created_at: chrono::Utc::now(),
    };
    repo.put_container(&container)
        .await
        .expect("persist container");
    let row: (String, String) =
        sqlx::query_as("SELECT spec_json,env_cipher FROM docker_containers WHERE id=?")
            .bind(container.spec.id.to_string())
            .fetch_one(&db.pool())
            .await
            .expect("raw container");
    assert!(!row.0.contains("top-secret"));
    assert!(!row.1.contains("top-secret"));
    assert_eq!(
        repo.get_container(container.spec.id)
            .await
            .expect("load")
            .expect("container")
            .spec
            .env["PASSWORD"],
        "top-secret"
    );
    let stack = StoredComposeStack {
        stack: signed_stack(),
        status: "pending".into(),
        last_applied_at: None,
    };
    repo.put_stack(&stack).await.expect("persist stack");
    let cipher: String = sqlx::query_scalar("SELECT stack_cipher FROM docker_stacks WHERE id=?")
        .bind(stack.stack.id.to_string())
        .fetch_one(&db.pool())
        .await
        .expect("raw stack");
    assert!(!cipher.contains("services"));
    assert_eq!(
        repo.get_stack(stack.stack.id)
            .await
            .expect("load stack")
            .expect("stack")
            .stack,
        stack.stack
    );
}
