//! Docker-backed immutable image resolution for application sources.

use std::{
    future::Future,
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
};

use chrono::Utc;
use openpanel_domain::{
    ApplicationDeliveryError, ApplicationSource, ApplicationSourceKind, BuildAdapter,
    ReleaseDigest, application_delivery::validate_source_ref, docker::DockerError,
};
use uuid::Uuid;

use crate::docker::DockerAdapter;

/// Docker build adapter configured with one stable remote registry namespace.
pub struct DockerBuildAdapter {
    docker: Arc<dyn DockerAdapter>,
    namespace: String,
    max_context_bytes: usize,
}

impl DockerBuildAdapter {
    /// Validate a registry namespace and build-context byte ceiling.
    pub fn new(
        docker: Arc<dyn DockerAdapter>,
        namespace: String,
        max_context_bytes: usize,
    ) -> Result<Self, ApplicationDeliveryError> {
        if namespace.trim().is_empty()
            || namespace.contains('@')
            || namespace.chars().any(char::is_whitespace)
            || max_context_bytes == 0
        {
            return Err(ApplicationDeliveryError::Invalid(
                "delivery registry namespace or context limit is invalid".into(),
            ));
        }
        Ok(Self {
            docker,
            namespace: namespace.trim_end_matches('/').into(),
            max_context_bytes,
        })
    }

    /// Stable repository path shared with the runtime adapter.
    pub fn repository(&self, application_id: Uuid) -> String {
        format!("{}/{}", self.namespace, application_id)
    }
}

impl BuildAdapter for DockerBuildAdapter {
    fn adapter_id(&self) -> &'static str {
        "oci-docker-build-v1"
    }

    fn resolve_digest(
        &self,
        application_id: Uuid,
        source: &ApplicationSource,
        build_command: &str,
    ) -> Result<ReleaseDigest, ApplicationDeliveryError> {
        validate_source_ref(source.kind(), source.source_ref())?;
        let docker = self.docker.clone();
        run_docker(async move { docker.ping().await })
            .map_err(|_| ApplicationDeliveryError::RuntimeUnavailable)?;
        match source.kind() {
            ApplicationSourceKind::Oci => {
                let repository = source.url().strip_prefix("oci://").ok_or_else(|| {
                    ApplicationDeliveryError::Invalid("OCI source URL is invalid".into())
                })?;
                let digest = ReleaseDigest::new(source.source_ref(), Utc::now())?;
                let image = format!("{repository}@{}", digest.value());
                let docker = self.docker.clone();
                let inspect_image = image.clone();
                let exists = run_docker(async move { docker.image_exists(&inspect_image).await })
                    .map_err(|_| ApplicationDeliveryError::RuntimeUnavailable)?;
                if !exists {
                    let docker = self.docker.clone();
                    let pulled = run_docker(async move { docker.pull(&image).await })
                        .map_err(|_| ApplicationDeliveryError::RuntimeUnavailable)?;
                    if !pulled.ends_with(digest.value()) {
                        return Err(ApplicationDeliveryError::Invalid(
                            "OCI registry returned a different digest".into(),
                        ));
                    }
                }
                Ok(digest)
            }
            ApplicationSourceKind::Git | ApplicationSourceKind::Compose => {
                let context = git_context(source, self.max_context_bytes)?;
                let repository = self.repository(application_id);
                let tag = format!("{repository}:{}", source.source_ref());
                let docker = self.docker.clone();
                let build_command = build_command.to_owned();
                let build_tag = tag.clone();
                run_docker(async move {
                    docker
                        .build_image(context, &build_tag, &build_command)
                        .await
                })
                .map_err(|_| ApplicationDeliveryError::RuntimeUnavailable)?;
                let docker = self.docker.clone();
                let pushed =
                    run_docker(async move { docker.push_image(&tag).await }).map_err(|error| {
                        match error {
                            DockerError::Forbidden => {
                                ApplicationDeliveryError::RegistryUnauthorized
                            }
                            _ => ApplicationDeliveryError::RuntimeUnavailable,
                        }
                    })?;
                ReleaseDigest::new(pushed, Utc::now())
            }
        }
    }
}

/// Run Docker I/O on a worker thread so synchronous domain ports never block
/// a nested Tokio runtime. The outer caller may already be on a Tokio worker.
pub(super) fn run_docker<T, F>(future: F) -> Result<T, DockerError>
where
    T: Send,
    F: Future<Output = Result<T, DockerError>> + Send,
{
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| DockerError::Adapter("docker worker unavailable".into()))?;
                runtime.block_on(future)
            })
            .join()
            .map_err(|_| DockerError::Adapter("docker worker failed".into()))?
    })
}

fn git_context(
    source: &ApplicationSource,
    max_bytes: usize,
) -> Result<Vec<u8>, ApplicationDeliveryError> {
    let source_url = source.url();
    if !(source.url().starts_with("https://")
        || source.url().starts_with("ssh://")
        || source.url().starts_with("git@")
        || (cfg!(test) && source.url().starts_with("file://")))
    {
        return Err(ApplicationDeliveryError::Invalid(
            "Git source URL is invalid".into(),
        ));
    }
    if source_url.starts_with("https://")
        && source_url[8..]
            .split('/')
            .next()
            .is_some_and(|authority| authority.contains('@'))
    {
        return Err(ApplicationDeliveryError::Invalid(
            "Git source URL credentials rejected".into(),
        ));
    }
    let dir = tempfile::tempdir()
        .map_err(|_| ApplicationDeliveryError::Invalid("build context unavailable".into()))?;
    let path = dir.path();
    git(path, &["init", "--quiet"])?;
    git(path, &["remote", "add", "origin", source.url()])?;
    git(
        path,
        &[
            "fetch",
            "--quiet",
            "--depth",
            "1",
            "origin",
            source.source_ref(),
        ],
    )?;
    git(path, &["checkout", "--quiet", "--detach", "FETCH_HEAD"])?;
    let head = git_output(path, &["rev-parse", "HEAD"])?;
    if !head.trim().starts_with(source.source_ref()) {
        return Err(ApplicationDeliveryError::Invalid(
            "Git source ref did not resolve exactly".into(),
        ));
    }
    if !path.join("Dockerfile").is_file() {
        return Err(ApplicationDeliveryError::Invalid(
            "Git source has no Dockerfile".into(),
        ));
    }
    let mut tar = tar::Builder::new(Vec::new());
    let mut total = 0usize;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry
            .map_err(|_| ApplicationDeliveryError::Invalid("build context unreadable".into()))?;
        let relative = entry
            .path()
            .strip_prefix(path)
            .map_err(|_| ApplicationDeliveryError::Invalid("build context invalid".into()))?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .next()
                .is_some_and(|c| c.as_os_str() == ".git")
        {
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(ApplicationDeliveryError::Invalid(
                "build context symlink rejected".into(),
            ));
        }
        if entry.file_type().is_file() {
            let length = usize::try_from(
                entry
                    .metadata()
                    .map_err(|_| {
                        ApplicationDeliveryError::Invalid("build context unreadable".into())
                    })?
                    .len(),
            )
            .map_err(|_| ApplicationDeliveryError::Invalid("build context too large".into()))?;
            total = total.saturating_add(length).saturating_add(1024);
            if total > max_bytes {
                return Err(ApplicationDeliveryError::Invalid(
                    "build context too large".into(),
                ));
            }
            tar.append_path_with_name(entry.path(), relative)
                .map_err(|_| {
                    ApplicationDeliveryError::Invalid("build context unreadable".into())
                })?;
        }
    }
    tar.into_inner()
        .map_err(|_| ApplicationDeliveryError::Invalid("build context unreadable".into()))
}

fn git(path: &Path, args: &[&str]) -> Result<(), ApplicationDeliveryError> {
    let status = git_command(path, args)
        .status()
        .map_err(|_| ApplicationDeliveryError::Invalid("Git fetch unavailable".into()))?;
    if status.success() {
        Ok(())
    } else {
        Err(ApplicationDeliveryError::Invalid("Git fetch failed".into()))
    }
}

fn git_output(path: &Path, args: &[&str]) -> Result<String, ApplicationDeliveryError> {
    let output = git_command(path, args)
        .output()
        .map_err(|_| ApplicationDeliveryError::Invalid("Git fetch unavailable".into()))?;
    if !output.status.success() {
        return Err(ApplicationDeliveryError::Invalid("Git fetch failed".into()));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| ApplicationDeliveryError::Invalid("Git commit invalid".into()))
}

fn git_command(path: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(path)
        .args(["-c", "protocol.ext.allow=never", "-c", "credential.helper="])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_SSH_COMMAND")
        .env_remove("GIT_CONFIG_COUNT")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}
