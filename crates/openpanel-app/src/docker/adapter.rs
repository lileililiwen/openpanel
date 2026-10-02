//! Bollard adapter exposing only vetted daemon operations.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use bollard::{
    Docker,
    auth::DockerCredentials,
    body_full,
    container::LogOutput,
    exec::{CreateExecOptions, StartExecResults},
    models::{
        ContainerCreateBody, ContainerSummaryStateEnum, HostConfig, NetworkCreateRequest,
        PortBinding as DockerPortBinding, RestartPolicy as DockerRestartPolicy,
        RestartPolicyNameEnum,
    },
    query_parameters::{
        BuildImageOptionsBuilder, CreateContainerOptionsBuilder, CreateImageOptionsBuilder,
        ListContainersOptionsBuilder, LogsOptionsBuilder, PushImageOptionsBuilder,
        RemoveContainerOptionsBuilder,
    },
};
use futures_util::StreamExt;
use openpanel_domain::docker::{
    ComposeStack, ContainerSpec, DockerError, ResourceLimits, RestartPolicy,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{ApplyReport, DockerAdapter, ExecResult, NetworkAdapter, RuntimeContainerState};

/// Runtime adapter backed by Docker Engine HTTP over its configured socket.
pub struct BollardDockerAdapter {
    docker: Docker,
    registry_credentials: Option<DockerCredentials>,
}

impl BollardDockerAdapter {
    /// Connect using local Docker/Podman socket discovery.
    pub fn connect() -> Result<Self, DockerError> {
        Docker::connect_with_local_defaults()
            .map(|docker| Self {
                docker,
                registry_credentials: None,
            })
            .map_err(adapter)
    }

    /// Construct around an already configured Bollard client.
    pub fn new(docker: Docker) -> Self {
        Self {
            docker,
            registry_credentials: None,
        }
    }

    /// Connect and configure registry push credentials from delivery
    /// settings. Credentials are optional: without them the adapter
    /// pushes anonymously, which is what a local registry expects.
    pub fn connect_with_registry_credentials(
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self, DockerError> {
        let adapter = Self::connect()?;
        match (username, password) {
            (Some(username), Some(password)) => {
                Ok(adapter.with_registry_credentials(DockerCredentials {
                    username: Some(username),
                    password: Some(password),
                    ..Default::default()
                }))
            }
            _ => Ok(adapter),
        }
    }

    /// Configure registry credentials resolved from provider delivery settings.
    /// They are never included in diagnostics or debug output.
    pub fn with_registry_credentials(mut self, credentials: DockerCredentials) -> Self {
        self.registry_credentials = Some(credentials);
        self
    }
}

#[async_trait]
impl DockerAdapter for BollardDockerAdapter {
    async fn ping(&self) -> Result<(), DockerError> {
        self.docker.ping().await.map(|_| ()).map_err(adapter)
    }

    async fn pull(&self, image: &str) -> Result<String, DockerError> {
        if let Ok(inspected) = self.docker.inspect_image(image).await
            && let Some(digest) = inspected
                .repo_digests
                .and_then(|values| values.into_iter().next())
        {
            return Ok(digest);
        }
        let options = CreateImageOptionsBuilder::default()
            .from_image(image)
            .build();
        let mut stream = self.docker.create_image(Some(options), None, None);
        while let Some(item) = stream.next().await {
            item.map_err(adapter)?;
        }
        self.docker
            .inspect_image(image)
            .await
            .map_err(adapter)?
            .repo_digests
            .and_then(|digests| digests.into_iter().next())
            .ok_or_else(|| DockerError::Adapter("pulled image has no repository digest".into()))
    }

    async fn image_exists(&self, image: &str) -> Result<bool, DockerError> {
        match self.docker.inspect_image(image).await {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(false),
            Err(_) => Err(DockerError::Adapter("image inspection failed".into())),
        }
    }

    async fn build_image(
        &self,
        context: Vec<u8>,
        image: &str,
        build_command: &str,
    ) -> Result<(), DockerError> {
        let args = HashMap::from([(
            "OPENPANEL_BUILD_COMMAND".to_string(),
            build_command.to_string(),
        )]);
        let options = BuildImageOptionsBuilder::default()
            .dockerfile("Dockerfile")
            .t(image)
            .buildargs(&args)
            .rm(true)
            .build();
        let mut stream = self
            .docker
            .build_image(options, None, Some(body_full(context.into())));
        let mut saw_success = false;
        while let Some(item) = stream.next().await {
            let info = item.map_err(|_| DockerError::Adapter("image build failed".into()))?;
            if info.error_detail.is_some() {
                return Err(DockerError::Adapter("image build failed".into()));
            }
            if info.aux.is_some() {
                saw_success = true;
            }
        }
        if !saw_success {
            // Docker's classic builder may not emit `aux`; verify the tag exists.
            self.docker
                .inspect_image(image)
                .await
                .map_err(|_| DockerError::Adapter("image build produced no image".into()))?;
        }
        Ok(())
    }

    async fn push_image(&self, image: &str) -> Result<String, DockerError> {
        let (repository, tag) = image
            .rsplit_once(':')
            .ok_or_else(|| DockerError::Invalid("push image must be tagged".into()))?;
        let options = PushImageOptionsBuilder::default().tag(tag).build();
        let mut stream =
            self.docker
                .push_image(repository, Some(options), self.registry_credentials.clone());
        let mut digest = None;
        while let Some(item) = stream.next().await {
            let info = item.map_err(push_error)?;
            if let Some(error) = info.error_detail {
                let message = error.message.unwrap_or_default();
                return Err(classify_push_message(&message));
            }
            if let Some(status) = info.status
                && let Some(value) = status.split("digest: ").nth(1)
            {
                let candidate = value.split_whitespace().next().unwrap_or_default();
                if valid_sha256(candidate) {
                    digest = Some(candidate.to_string());
                }
            }
        }
        digest.ok_or_else(|| DockerError::Adapter("registry did not confirm a digest".into()))
    }

    async fn create(&self, spec: &ContainerSpec) -> Result<String, DockerError> {
        self.create_labeled(spec, &BTreeMap::new()).await
    }

    async fn create_labeled(
        &self,
        spec: &ContainerSpec,
        labels: &BTreeMap<String, String>,
    ) -> Result<String, DockerError> {
        let network = spec
            .site_id
            .map(|site| format!("openpanel-{site}"))
            .unwrap_or_else(|| "openpanel-system".into());
        let port_bindings = spec.ports.iter().fold(HashMap::new(), |mut map, port| {
            map.insert(
                format!("{}/tcp", port.container),
                Some(vec![DockerPortBinding {
                    host_ip: Some("127.0.0.1".into()),
                    host_port: Some(port.host.to_string()),
                }]),
            );
            map
        });
        let restart_policy = match spec.restart_policy {
            RestartPolicy::No => DockerRestartPolicy {
                name: Some(RestartPolicyNameEnum::NO),
                maximum_retry_count: None,
            },
            RestartPolicy::OnFailure { max_retries } => DockerRestartPolicy {
                name: Some(RestartPolicyNameEnum::ON_FAILURE),
                maximum_retry_count: max_retries.map(|value| value as i64),
            },
            RestartPolicy::Always => DockerRestartPolicy {
                name: Some(RestartPolicyNameEnum::ALWAYS),
                maximum_retry_count: None,
            },
            RestartPolicy::UnlessStopped => DockerRestartPolicy {
                name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
                maximum_retry_count: None,
            },
        };
        let host_config = HostConfig {
            cpu_shares: Some(spec.limits.cpu_shares as i64),
            memory: Some((spec.limits.mem_mb * 1024 * 1024) as i64),
            memory_swap: Some((spec.limits.mem_mb * 1024 * 1024) as i64),
            pids_limit: Some(spec.limits.pids_max),
            network_mode: Some(network),
            port_bindings: Some(port_bindings),
            binds: Some(
                spec.mounts
                    .iter()
                    .map(|mount| {
                        format!(
                            "{}:{}{}",
                            mount.source.display(),
                            mount.target.display(),
                            if mount.read_only { ":ro" } else { "" }
                        )
                    })
                    .collect(),
            ),
            cap_drop: Some(vec!["ALL".into()]),
            cap_add: Some(spec.capabilities.clone()),
            restart_policy: Some(restart_policy),
            security_opt: Some(vec!["no-new-privileges:true".into()]),
            ..Default::default()
        };
        let body = ContainerCreateBody {
            image: Some(spec.image.clone()),
            user: Some(spec.user_namespace.clone()),
            env: Some(
                spec.env
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect(),
            ),
            cmd: (!spec.command.is_empty()).then(|| spec.command.clone()),
            exposed_ports: Some(
                spec.ports
                    .iter()
                    .map(|port| format!("{}/tcp", port.container))
                    .collect(),
            ),
            host_config: Some(host_config),
            labels: Some(
                HashMap::from([("openpanel.container_id".into(), spec.id.to_string())])
                    .into_iter()
                    .chain(
                        labels
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone())),
                    )
                    .collect(),
            ),
            ..Default::default()
        };
        self.docker
            .create_container(
                Some(
                    CreateContainerOptionsBuilder::default()
                        .name(&format!("openpanel-{}", spec.name))
                        .build(),
                ),
                body,
            )
            .await
            .map(|result| result.id)
            .map_err(adapter)
    }

    async fn start(&self, id: &str) -> Result<(), DockerError> {
        self.docker.start_container(id, None).await.map_err(adapter)
    }

    async fn stop(&self, id: &str) -> Result<(), DockerError> {
        self.docker.stop_container(id, None).await.map_err(adapter)
    }

    async fn restart(&self, id: &str) -> Result<(), DockerError> {
        self.docker
            .restart_container(id, None)
            .await
            .map_err(adapter)
    }

    async fn remove(&self, id: &str, force: bool) -> Result<(), DockerError> {
        self.docker
            .remove_container(
                id,
                Some(
                    RemoveContainerOptionsBuilder::default()
                        .force(force)
                        .build(),
                ),
            )
            .await
            .map_err(adapter)
    }

    async fn inspect(&self, id: &str) -> Result<RuntimeContainerState, DockerError> {
        let state = self
            .docker
            .inspect_container(id, None)
            .await
            .map_err(adapter)?
            .state;
        Ok(RuntimeContainerState {
            status: state
                .as_ref()
                .and_then(|value| value.status)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".into()),
            oom_killed: state
                .as_ref()
                .and_then(|value| value.oom_killed)
                .unwrap_or(false),
            // Only an image-declared HEALTHCHECK produces a verdict;
            // `None` tells the caller to fall back to the status.
            health: state
                .as_ref()
                .and_then(|value| value.health.as_ref())
                .and_then(|health| health.status.as_ref())
                .map(|status| status.to_string()),
        })
    }

    async fn logs(&self, id: &str, tail: u64) -> Result<Vec<String>, DockerError> {
        let options = LogsOptionsBuilder::default()
            .stdout(true)
            .stderr(true)
            .tail(&tail.to_string())
            .build();
        let mut stream = self.docker.logs(id, Some(options));
        let mut lines = Vec::new();
        while let Some(item) = stream.next().await {
            let output = item.map_err(adapter)?;
            lines.push(match output {
                LogOutput::StdOut { message }
                | LogOutput::StdErr { message }
                | LogOutput::Console { message }
                | LogOutput::StdIn { message } => String::from_utf8_lossy(&message).into_owned(),
            });
            if lines.len() >= tail as usize {
                break;
            }
        }
        Ok(lines)
    }

    async fn exec(
        &self,
        id: &str,
        command: &[String],
        user: &str,
    ) -> Result<ExecResult, DockerError> {
        let created = self
            .docker
            .create_exec(
                id,
                CreateExecOptions {
                    cmd: Some(command.to_vec()),
                    user: Some(user.to_owned()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(adapter)?;
        let mut output = String::new();
        if let StartExecResults::Attached {
            output: mut stream, ..
        } = self
            .docker
            .start_exec(&created.id, None)
            .await
            .map_err(adapter)?
        {
            while let Some(item) = stream.next().await {
                output.push_str(&item.map_err(adapter)?.to_string());
                if output.len() > 65_536 {
                    break;
                }
            }
        }
        let inspected = self
            .docker
            .inspect_exec(&created.id)
            .await
            .map_err(adapter)?;
        Ok(ExecResult {
            exit_code: inspected.exit_code.unwrap_or(-1),
            truncated: output.len() > 65_536,
            output,
        })
    }

    async fn apply_stack(&self, stack: &ComposeStack) -> Result<ApplyReport, DockerError> {
        let document: AdapterComposeDocument = serde_yaml::from_str(&stack.compose_yaml_canonical)
            .map_err(|_| DockerError::Invalid("unsupported compose document".into()))?;
        let mut created = Vec::new();
        let mut removed = Vec::new();
        let prefix = format!("/openpanel-{}_", stack.name);
        let mut existing = self
            .docker
            .list_containers(Some(
                ListContainersOptionsBuilder::default().all(true).build(),
            ))
            .await
            .map_err(adapter)?
            .into_iter()
            .filter_map(|container| {
                let name = container
                    .names
                    .as_ref()?
                    .iter()
                    .find(|name| name.starts_with(&prefix))?;
                Some((name.clone(), container))
            })
            .collect::<HashMap<_, _>>();
        for (service, desired) in document.services {
            if service.is_empty()
                || service.len() > 63
                || !service
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            {
                return Err(DockerError::Invalid("invalid compose service name".into()));
            }
            let mut digest = Sha256::new();
            digest.update(stack.id.as_bytes());
            digest.update(service.as_bytes());
            let bytes: [u8; 16] = digest.finalize()[..16]
                .try_into()
                .map_err(|_| DockerError::Invalid("invalid stack service id".into()))?;
            let id = Uuid::from_bytes(bytes);
            let name = format!("{}_{}", stack.name, service);
            let runtime_name = format!("openpanel-{name}");
            if let Some(current) = existing.remove(&format!("/{runtime_name}")) {
                let runtime_id = current.id.unwrap_or_else(|| runtime_name.clone());
                if current.image.as_deref() == Some(desired.image.as_str()) {
                    if current.state != Some(ContainerSummaryStateEnum::RUNNING) {
                        self.start(&runtime_id).await?;
                    }
                    continue;
                }
                self.remove(&runtime_id, true).await?;
                removed.push(runtime_id);
            }
            let spec = ContainerSpec {
                id,
                name,
                image: desired.image,
                site_id: stack.site_id,
                env: std::collections::BTreeMap::new(),
                command: Vec::new(),
                ports: Vec::new(),
                mounts: Vec::new(),
                capabilities: Vec::new(),
                limits: ResourceLimits::default(),
                restart_policy: RestartPolicy::UnlessStopped,
                user_namespace: "65534:65534".into(),
            };
            let runtime = self.create(&spec).await?;
            self.start(&runtime).await?;
            created.push(runtime);
        }
        for (_, container) in existing {
            if let Some(runtime_id) = container.id {
                self.remove(&runtime_id, true).await?;
                removed.push(runtime_id);
            }
        }
        Ok(ApplyReport { created, removed })
    }

    async fn remove_stack(&self, stack: &ComposeStack) -> Result<Vec<String>, DockerError> {
        let prefix = format!("/openpanel-{}_", stack.name);
        let containers = self
            .docker
            .list_containers(Some(
                ListContainersOptionsBuilder::default().all(true).build(),
            ))
            .await
            .map_err(adapter)?;
        let mut removed = Vec::new();
        for container in containers {
            let owned = container
                .names
                .as_ref()
                .is_some_and(|names| names.iter().any(|name| name.starts_with(&prefix)));
            if owned && let Some(runtime_id) = container.id {
                self.remove(&runtime_id, true).await?;
                removed.push(runtime_id);
            }
        }
        Ok(removed)
    }
}

#[async_trait]
impl NetworkAdapter for BollardDockerAdapter {
    async fn ensure(&self, site_id: Option<Uuid>) -> Result<(), DockerError> {
        let name = network_name(site_id);
        let request = NetworkCreateRequest {
            name,
            driver: Some("bridge".into()),
            internal: Some(true),
            options: Some(HashMap::from([
                ("com.docker.network.bridge.enable_icc".into(), "true".into()),
                (
                    "com.docker.network.bridge.gateway_mode_ipv4".into(),
                    "isolated".into(),
                ),
                (
                    "com.docker.network.bridge.gateway_mode_ipv6".into(),
                    "isolated".into(),
                ),
            ])),
            ..Default::default()
        };
        if let Err(error) = self.docker.create_network(request).await
            && !error.to_string().contains("already exists")
        {
            return Err(adapter(error));
        }
        Ok(())
    }

    async fn remove_if_unused(&self, site_id: Option<Uuid>) -> Result<(), DockerError> {
        let name = network_name(site_id);
        match self.docker.remove_network(&name).await {
            Ok(()) => Ok(()),
            Err(error) if error.to_string().contains("active endpoints") => Ok(()),
            Err(error) if error.to_string().contains("not found") => Ok(()),
            Err(error) => Err(adapter(error)),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdapterComposeDocument {
    services: std::collections::BTreeMap<String, AdapterComposeService>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdapterComposeService {
    image: String,
}

fn network_name(site_id: Option<Uuid>) -> String {
    site_id
        .map(|site| format!("openpanel-{site}"))
        .unwrap_or_else(|| "openpanel-system".into())
}

fn adapter(error: impl std::fmt::Display) -> DockerError {
    DockerError::Adapter(error.to_string().chars().take(1024).collect())
}

fn push_error(error: bollard::errors::Error) -> DockerError {
    match error {
        bollard::errors::Error::DockerResponseServerError {
            status_code: 401 | 403,
            ..
        } => DockerError::Forbidden,
        other => classify_push_message(&other.to_string()),
    }
}

fn classify_push_message(message: &str) -> DockerError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unauthorized")
        || lower.contains("authentication required")
        || lower.contains("denied")
        || lower.contains("forbidden")
    {
        DockerError::Forbidden
    } else {
        DockerError::Adapter("registry push failed".into())
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}
