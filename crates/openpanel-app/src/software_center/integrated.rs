//! Transactional application deployment over explicit OpenPanel resource ports.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use openpanel_domain::{Email, Password, Role, User, Username};
use rand::{Rng, distributions::Alphanumeric};
use uuid::Uuid;

use super::{
    ApplicationCapabilities, ApplicationDeployer, ApplicationDeploymentInput, ApplicationResource,
    ArtifactDownloader, ProvisionedApplication, SafeArtifactInstaller, SoftwareCenterError,
    pinned_artifact,
};
use crate::{DatabasesService, SitesService};

/// Site reservation returned by the site-management port.
#[derive(Debug, Clone)]
pub struct CreatedApplicationSite {
    /// Site aggregate identifier.
    pub id: Uuid,
    /// Canonical document root allocated by the site service.
    pub document_root: String,
}

/// Least-privilege database returned by the database-management port.
#[derive(Debug, Clone)]
pub struct CreatedApplicationDatabase {
    /// Database aggregate identifier.
    pub id: Uuid,
    /// Distro database name.
    pub name: String,
    /// Application database user.
    pub username: String,
    /// Local database host.
    pub host: String,
    /// One-time plaintext used only while writing the application configuration.
    pub password: String,
}

/// Public resource boundary used by the deployment transaction.
#[async_trait]
pub trait ApplicationDeploymentResources: Send + Sync {
    /// Optional integration capabilities supported by this resource composition.
    fn capabilities(&self) -> ApplicationCapabilities {
        ApplicationCapabilities::default()
    }

    /// Reserve the domain and create its PHP-enabled site.
    async fn create_site(
        &self,
        actor: Uuid,
        input: &ApplicationDeploymentInput,
    ) -> Result<CreatedApplicationSite, SoftwareCenterError>;

    /// Create a least-privilege application database and user.
    async fn create_database(
        &self,
        actor: Uuid,
        input: &ApplicationDeploymentInput,
    ) -> Result<CreatedApplicationDatabase, SoftwareCenterError>;

    /// Download, verify, extract, configure, and initialize the pinned application.
    async fn install_application(
        &self,
        input: &ApplicationDeploymentInput,
        site: &CreatedApplicationSite,
        database: &CreatedApplicationDatabase,
        admin_username: &str,
        admin_password: &str,
    ) -> Result<(), SoftwareCenterError>;

    /// Create an optional DNS integration and return its rollback identifier.
    async fn create_dns(
        &self,
        input: &ApplicationDeploymentInput,
    ) -> Result<String, SoftwareCenterError>;

    /// Create an optional TLS integration.
    async fn create_tls(
        &self,
        input: &ApplicationDeploymentInput,
    ) -> Result<(), SoftwareCenterError>;

    /// Create an optional backup registration and return its aggregate identifier.
    async fn create_backup(
        &self,
        actor: Uuid,
        input: &ApplicationDeploymentInput,
        site: &CreatedApplicationSite,
    ) -> Result<Uuid, SoftwareCenterError>;

    /// Validate the initialized application without returning response content.
    async fn validate_application(
        &self,
        input: &ApplicationDeploymentInput,
        site: &CreatedApplicationSite,
    ) -> Result<(), SoftwareCenterError>;

    /// Remove exactly one resource described by a job receipt.
    async fn remove(&self, resource: &ApplicationResource) -> Result<(), SoftwareCenterError>;
}

/// Application adapter that owns ordering and reverse-order compensation.
pub struct IntegratedApplicationDeployer {
    resources: Arc<dyn ApplicationDeploymentResources>,
}

/// Production resource adapter backed by the existing site/database services and pinned artifacts.
pub struct OpenPanelApplicationResources {
    sites: Arc<SitesService>,
    databases: Arc<DatabasesService>,
    downloader: ArtifactDownloader,
    installer: SafeArtifactInstaller,
}

impl OpenPanelApplicationResources {
    /// Compose production site, database, artifact, and PHP installer integrations.
    pub fn new(
        sites: Arc<SitesService>,
        databases: Arc<DatabasesService>,
    ) -> Result<Self, SoftwareCenterError> {
        Ok(Self {
            sites,
            databases,
            downloader: ArtifactDownloader::new()?,
            installer: SafeArtifactInstaller::default(),
        })
    }
}

impl IntegratedApplicationDeployer {
    /// Construct the transaction over explicit site/database/artifact/integration ports.
    pub fn new(resources: Arc<dyn ApplicationDeploymentResources>) -> Self {
        Self { resources }
    }

    async fn unwind(&self, resources: &[ApplicationResource]) -> Result<(), SoftwareCenterError> {
        let mut failed = false;
        for resource in resources.iter().rev() {
            if self.resources.remove(resource).await.is_err() {
                failed = true;
            }
        }
        if failed {
            Err(SoftwareCenterError::Package("operation failed".into()))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl ApplicationDeployer for IntegratedApplicationDeployer {
    fn capabilities(&self) -> ApplicationCapabilities {
        self.resources.capabilities()
    }

    async fn provision(
        &self,
        actor: Uuid,
        input: &ApplicationDeploymentInput,
    ) -> Result<ProvisionedApplication, SoftwareCenterError> {
        let admin_username = "openpanel-admin".to_owned();
        let admin_password: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        let mut receipts = Vec::new();

        let site = match self.resources.create_site(actor, input).await {
            Ok(site) => site,
            Err(error) => return Err(error),
        };
        receipts.push(ApplicationResource::Site {
            id: site.id,
            document_root: site.document_root.clone(),
        });

        let database = match self.resources.create_database(actor, input).await {
            Ok(database) => database,
            Err(error) => {
                let _ = self.unwind(&receipts).await;
                return Err(error);
            }
        };
        receipts.push(ApplicationResource::Database { id: database.id });

        if let Err(error) = self
            .resources
            .install_application(input, &site, &database, &admin_username, &admin_password)
            .await
        {
            let _ = self.unwind(&receipts).await;
            return Err(error);
        }
        receipts.push(ApplicationResource::Artifact {
            application: input.application.clone(),
            domain: input.domain.clone(),
            php_version: input.php_version.clone(),
            document_root: site.document_root.clone(),
        });

        if input.enable_dns {
            match self.resources.create_dns(input).await {
                Ok(id) => receipts.push(ApplicationResource::Dns { id }),
                Err(error) => {
                    let _ = self.unwind(&receipts).await;
                    return Err(error);
                }
            }
        }
        if input.enable_tls {
            if let Err(error) = self.resources.create_tls(input).await {
                let _ = self.unwind(&receipts).await;
                return Err(error);
            }
            receipts.push(ApplicationResource::Tls {
                domain: input.domain.clone(),
            });
        }
        if input.enable_backups {
            match self.resources.create_backup(actor, input, &site).await {
                Ok(id) => receipts.push(ApplicationResource::Backup { id }),
                Err(error) => {
                    let _ = self.unwind(&receipts).await;
                    return Err(error);
                }
            }
        }

        let labels = receipts.iter().map(resource_label).collect();
        Ok(ProvisionedApplication {
            id: Uuid::new_v4(),
            resources: labels,
            resource_receipts: receipts,
            admin_username,
            admin_password,
        })
    }

    async fn validate(
        &self,
        deployment: &ProvisionedApplication,
    ) -> Result<(), SoftwareCenterError> {
        let site = deployment
            .resource_receipts
            .iter()
            .find_map(|resource| match resource {
                ApplicationResource::Site { id, document_root } => Some(CreatedApplicationSite {
                    id: *id,
                    document_root: document_root.clone(),
                }),
                _ => None,
            })
            .ok_or(SoftwareCenterError::Validation)?;
        let (application, domain, php_version) = deployment
            .resource_receipts
            .iter()
            .find_map(|resource| match resource {
                ApplicationResource::Artifact {
                    application,
                    domain,
                    php_version,
                    ..
                } => Some((application.as_str(), domain.as_str(), php_version.as_str())),
                _ => None,
            })
            .ok_or(SoftwareCenterError::Validation)?;
        let input = ApplicationDeploymentInput::test(application, domain, php_version);
        self.resources.validate_application(&input, &site).await
    }

    async fn rollback(
        &self,
        deployment: &ProvisionedApplication,
    ) -> Result<(), SoftwareCenterError> {
        self.unwind(&deployment.resource_receipts).await
    }
}

fn resource_label(resource: &ApplicationResource) -> String {
    match resource {
        ApplicationResource::Site { id, .. } => format!("site:{id}"),
        ApplicationResource::Database { id } => format!("database:{id}"),
        ApplicationResource::Artifact {
            application,
            document_root,
            ..
        } => format!("artifact:{application}:{document_root}"),
        ApplicationResource::Dns { id } => format!("dns:{id}"),
        ApplicationResource::Tls { domain } => format!("tls:{domain}"),
        ApplicationResource::Backup { id } => format!("backup:{id}"),
    }
}

#[async_trait]
impl ApplicationDeploymentResources for OpenPanelApplicationResources {
    async fn create_site(
        &self,
        actor: Uuid,
        input: &ApplicationDeploymentInput,
    ) -> Result<CreatedApplicationSite, SoftwareCenterError> {
        let caller = service_actor(actor)?;
        let site = self
            .sites
            .create_site(
                &caller,
                actor,
                &input.domain,
                Vec::new(),
                true,
                Some(input.php_version.clone()),
                None,
            )
            .await
            .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
        Ok(CreatedApplicationSite {
            id: site.id(),
            document_root: site.document_root().to_owned(),
        })
    }

    async fn create_database(
        &self,
        actor: Uuid,
        input: &ApplicationDeploymentInput,
    ) -> Result<CreatedApplicationDatabase, SoftwareCenterError> {
        let caller = service_actor(actor)?;
        let suffix = database_suffix(&input.application, &input.domain);
        let (database, password) = self
            .databases
            .create_database(&caller, actor, "software", &suffix, Some("utf8mb4".into()))
            .await
            .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
        Ok(CreatedApplicationDatabase {
            id: database.id(),
            name: database.name().to_owned(),
            username: database.db_user().to_owned(),
            host: database.db_host().to_owned(),
            password,
        })
    }

    async fn install_application(
        &self,
        input: &ApplicationDeploymentInput,
        site: &CreatedApplicationSite,
        database: &CreatedApplicationDatabase,
        admin_username: &str,
        admin_password: &str,
    ) -> Result<(), SoftwareCenterError> {
        let artifact = pinned_artifact(&input.application)?;
        let bytes = self.downloader.download(&artifact).await?;
        let root = Path::new(&site.document_root);
        prepare_document_root(root)?;
        self.installer
            .extract_verified(&bytes, &artifact.digest, artifact.archive_root, root)?;
        let result = match input.application.as_str() {
            "wordpress" => {
                install_wordpress(root, input, database, admin_username, admin_password).await
            }
            "drupal" => install_drupal(root, input, database, admin_password).await,
            _ => Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            )),
        };
        if result.is_err() {
            let _ = remove_managed_document_root(root);
        }
        result
    }

    async fn create_dns(
        &self,
        _input: &ApplicationDeploymentInput,
    ) -> Result<String, SoftwareCenterError> {
        Err(SoftwareCenterError::Unsupported)
    }

    async fn create_tls(
        &self,
        _input: &ApplicationDeploymentInput,
    ) -> Result<(), SoftwareCenterError> {
        Err(SoftwareCenterError::Unsupported)
    }

    async fn create_backup(
        &self,
        _actor: Uuid,
        _input: &ApplicationDeploymentInput,
        _site: &CreatedApplicationSite,
    ) -> Result<Uuid, SoftwareCenterError> {
        Err(SoftwareCenterError::Unsupported)
    }

    async fn validate_application(
        &self,
        input: &ApplicationDeploymentInput,
        site: &CreatedApplicationSite,
    ) -> Result<(), SoftwareCenterError> {
        let root = Path::new(&site.document_root);
        let script = match input.application.as_str() {
            "wordpress" => wordpress_health_script(),
            "drupal" => drupal_health_script(),
            _ => {
                return Err(SoftwareCenterError::Invalid(
                    "invalid software request".into(),
                ));
            }
        };
        run_php_script(root, &input.php_version, &script).await
    }

    async fn remove(&self, resource: &ApplicationResource) -> Result<(), SoftwareCenterError> {
        match resource {
            ApplicationResource::Artifact { document_root, .. } => {
                remove_managed_document_root(Path::new(document_root))
            }
            ApplicationResource::Database { id } => self
                .databases
                .delete_database(&service_actor(Uuid::nil())?, *id)
                .await
                .map_err(|_| SoftwareCenterError::Package("operation failed".into())),
            ApplicationResource::Site { id, document_root } => {
                self.sites
                    .delete_site(&service_actor(Uuid::nil())?, *id)
                    .await
                    .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
                remove_empty_site_parent(Path::new(document_root));
                Ok(())
            }
            ApplicationResource::Dns { .. }
            | ApplicationResource::Tls { .. }
            | ApplicationResource::Backup { .. } => Err(SoftwareCenterError::Unsupported),
        }
    }
}

fn service_actor(id: Uuid) -> Result<User, SoftwareCenterError> {
    let username = Username::new("software")
        .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
    let email = Email::new("software@openpanel.invalid")
        .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))?;
    Ok(User::new(
        id,
        username,
        email,
        Password::from_hash("software-center-service-account"),
        Role::Owner,
    ))
}

fn database_suffix(application: &str, domain: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(domain.as_bytes()));
    let prefix = if application == "wordpress" {
        "wp"
    } else {
        "dr"
    };
    format!("{prefix}_{}", &digest[..12])
}

fn prepare_document_root(root: &Path) -> Result<(), SoftwareCenterError> {
    validate_managed_document_root(root)?;
    let placeholder = root.join("index.html");
    if placeholder.exists() {
        fs::remove_file(&placeholder)
            .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
    }
    if fs::read_dir(root)
        .map_err(|error| SoftwareCenterError::Package(format!("read {}: {error}", root.display())))?
        .next()
        .is_some()
    {
        return Err(SoftwareCenterError::Conflict);
    }
    Ok(())
}

fn validate_managed_document_root(root: &Path) -> Result<(), SoftwareCenterError> {
    if !root.is_absolute()
        || root.file_name().and_then(|value| value.to_str()) != Some("public_html")
        || !root.starts_with("/var/www")
        || root
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        || fs::symlink_metadata(root)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(true)
    {
        return Err(SoftwareCenterError::Invalid(
            "invalid software request".into(),
        ));
    }
    Ok(())
}

fn remove_managed_document_root(root: &Path) -> Result<(), SoftwareCenterError> {
    validate_managed_document_root(root)?;
    fs::remove_dir_all(root).map_err(|_| SoftwareCenterError::Package("operation failed".into()))
}

fn remove_empty_site_parent(root: &Path) {
    if let Some(parent) = root.parent() {
        let _ = fs::remove_dir(parent);
    }
}

async fn install_wordpress(
    root: &Path,
    input: &ApplicationDeploymentInput,
    database: &CreatedApplicationDatabase,
    admin_username: &str,
    admin_password: &str,
) -> Result<(), SoftwareCenterError> {
    let config = wordpress_config(database)?;
    write_secret_file(&root.join("wp-config.php"), &config)?;
    let script = format!(
        "<?php\ndefine('WP_INSTALLING', true);\n$_SERVER['HTTP_HOST']={};\n$_SERVER['REQUEST_URI']='/';\nrequire __DIR__.'/wp-load.php';\nrequire_once ABSPATH.'wp-admin/includes/upgrade.php';\n$result=wp_install({}, {}, 'admin@openpanel.invalid', true, '', {}, {});\nif (is_wp_error($result)) {{ exit(1); }}\n",
        php_string(&input.domain)?,
        php_string(&input.domain)?,
        php_string(admin_username)?,
        php_string(admin_password)?,
        php_string(&input.locale)?,
    );
    run_php_script(root, &input.php_version, &script).await
}

fn wordpress_config(database: &CreatedApplicationDatabase) -> Result<String, SoftwareCenterError> {
    let salts = (0..8).map(|_| random_secret(64)).collect::<Vec<_>>();
    let names = [
        "AUTH_KEY",
        "SECURE_AUTH_KEY",
        "LOGGED_IN_KEY",
        "NONCE_KEY",
        "AUTH_SALT",
        "SECURE_AUTH_SALT",
        "LOGGED_IN_SALT",
        "NONCE_SALT",
    ];
    let mut config = format!(
        "<?php\ndefine('DB_NAME', {});\ndefine('DB_USER', {});\ndefine('DB_PASSWORD', {});\ndefine('DB_HOST', {});\ndefine('DB_CHARSET', 'utf8mb4');\n$table_prefix='wp_';\n",
        php_string(&database.name)?,
        php_string(&database.username)?,
        php_string(&database.password)?,
        php_string(&database.host)?,
    );
    for (name, value) in names.into_iter().zip(salts) {
        config.push_str(&format!("define('{name}', {});\n", php_string(&value)?));
    }
    config.push_str("define('DISALLOW_FILE_EDIT', true);\nif (!defined('ABSPATH')) define('ABSPATH', __DIR__.'/');\nrequire_once ABSPATH.'wp-settings.php';\n");
    Ok(config)
}

async fn install_drupal(
    root: &Path,
    input: &ApplicationDeploymentInput,
    database: &CreatedApplicationDatabase,
    admin_password: &str,
) -> Result<(), SoftwareCenterError> {
    let default_dir = root.join("sites/default");
    fs::create_dir_all(default_dir.join("files"))
        .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
    fs::copy(
        default_dir.join("default.settings.php"),
        default_dir.join("settings.php"),
    )
    .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
    let language = input.locale.split(['_', '-']).next().unwrap_or("en");
    let driver = "Drupal\\mysql\\Driver\\Database\\mysql";
    let script = format!(
        "<?php\n$autoloader=require __DIR__.'/autoload.php';\nrequire_once __DIR__.'/core/includes/install.core.inc';\n$driver={};\n$settings=['interactive'=>false,'site_path'=>'sites/default','parameters'=>['profile'=>'standard','langcode'=>{}],'forms'=>['install_settings_form'=>['driver'=>$driver,$driver=>['database'=>{},'username'=>{},'password'=>{},'host'=>{},'port'=>'3306','prefix'=>'']],'install_configure_form'=>['site_name'=>{},'site_mail'=>'admin@openpanel.invalid','account'=>['name'=>'openpanel-admin','mail'=>'admin@openpanel.invalid','pass'=>['pass1'=>{},'pass2'=>{}]],'enable_update_status_module'=>true,'enable_update_status_emails'=>null]]];\ninstall_drupal($autoloader,$settings);\n",
        php_string(driver)?,
        php_string(language)?,
        php_string(&database.name)?,
        php_string(&database.username)?,
        php_string(&database.password)?,
        php_string(&database.host)?,
        php_string(&input.domain)?,
        php_string(admin_password)?,
        php_string(admin_password)?,
    );
    run_php_script(root, &input.php_version, &script).await?;
    secure_existing_file(&default_dir.join("settings.php"))
}

fn wordpress_health_script() -> String {
    "<?php\nrequire __DIR__.'/wp-load.php';\nexit(is_blog_installed() ? 0 : 1);\n".into()
}

fn drupal_health_script() -> String {
    "<?php\n$autoloader=require __DIR__.'/autoload.php';\n$request=Symfony\\Component\\HttpFoundation\\Request::create('/');\n$kernel=Drupal\\Core\\DrupalKernel::createFromRequest($request,$autoloader,'prod');\n$kernel->boot();\nexit($kernel->getContainer()->get('database')->schema()->tableExists('users_field_data') ? 0 : 1);\n".into()
}

async fn run_php_script(
    root: &Path,
    php_version: &str,
    script: &str,
) -> Result<(), SoftwareCenterError> {
    let program = match php_version {
        "8.3" => "/usr/bin/php8.3",
        "8.4" => "/usr/bin/php8.4",
        _ => {
            return Err(SoftwareCenterError::Invalid(
                "invalid software request".into(),
            ));
        }
    };
    let script_path = root.join(format!(".openpanel-install-{}.php", Uuid::new_v4()));
    write_secret_file(&script_path, script)?;
    let output = tokio::time::timeout(
        Duration::from_secs(300),
        tokio::process::Command::new(program)
            .arg(&script_path)
            .current_dir(root)
            .env_clear()
            .env("LANG", "C.UTF-8")
            .kill_on_drop(true)
            .output(),
    )
    .await;
    let _ = fs::remove_file(&script_path);
    match output {
        Ok(Ok(result)) if result.status.success() => Ok(()),
        _ => Err(SoftwareCenterError::Validation),
    }
}

fn write_secret_file(path: &Path, contents: &str) -> Result<(), SoftwareCenterError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
    file.write_all(contents.as_bytes())
        .map_err(|_| SoftwareCenterError::Package("operation failed".into()))?;
    file.flush()
        .map_err(|_| SoftwareCenterError::Package("operation failed".into()))
}

fn secure_existing_file(path: &Path) -> Result<(), SoftwareCenterError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| SoftwareCenterError::Package("operation failed".into()))
}

fn php_string(value: &str) -> Result<String, SoftwareCenterError> {
    serde_json::to_string(value)
        .map_err(|_| SoftwareCenterError::Invalid("invalid software request".into()))
}

fn random_secret(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}
