use clap::{Parser, Subcommand};

/// Top-level clap parser for the `openpanel` binary.
#[derive(Debug, Parser)]
#[command(name = "openpanel", about = "OpenPanel server management CLI", version)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands available to the `openpanel` CLI.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the API server + agent in the foreground.
    Serve,
    /// Zero-config dev launcher: pick a writable data dir, run
    /// migrations, bootstrap a default owner if none exists, then
    /// start the web + API server. Idempotent.
    Dev,
    /// Apply pending database migrations and exit.
    Migrate,
    /// User management commands.
    User {
        /// User subcommand to execute.
        #[command(subcommand)]
        action: UserCommand,
    },
    /// Site management commands.
    Site {
        /// Site subcommand to execute.
        #[command(subcommand)]
        action: SiteCommand,
    },
    /// Database management commands.
    Database {
        /// Database subcommand to execute.
        #[command(subcommand)]
        action: DatabaseCommand,
    },
    /// File-manager commands.
    File {
        /// File subcommand to execute.
        #[command(subcommand)]
        action: FileCommand,
    },
    /// SSL / TLS certificate management commands.
    Ssl {
        /// SSL subcommand to execute.
        #[command(subcommand)]
        action: SslCommand,
    },
    /// Host resource monitoring commands.
    Monitoring {
        /// Monitoring subcommand to execute.
        #[command(subcommand)]
        action: MonitoringCommand,
    },
    /// Scheduled command management.
    Cron {
        /// Cron subcommand to execute.
        #[command(subcommand)]
        action: CronCommand,
    },
    /// Backup plans, runs, verification, and restore.
    Backup {
        /// Backup subcommand.
        #[command(subcommand)]
        action: BackupCommand,
    },
    /// Browse registered logs and traffic insights.
    Logs {
        /// Log operation.
        #[command(subcommand)]
        action: LogsCommand,
    },
    /// Manage the isolated host firewall and login blocks.
    Security {
        /// Security operation.
        #[command(subcommand)]
        action: SecurityCommand,
    },
    /// Inspect and control registered system services.
    Services {
        /// Service operation.
        #[command(subcommand)]
        action: ServicesCommand,
    },
    /// Manage external-provider DNS zones and records.
    Dns {
        /// DNS operation.
        #[command(subcommand)]
        action: DnsCommand,
    },
    /// Manage hosted mail domains, mailboxes, aliases, and diagnostics.
    Mail {
        /// Mail operation.
        #[command(subcommand)]
        action: MailCommand,
    },
    /// Browse and install curated system software and applications.
    Software {
        /// Software Center operation.
        #[command(subcommand)]
        action: SoftwareCommand,
    },
    /// Discover, install, and manage plugins.
    Plugin {
        /// Plugin operation.
        #[command(subcommand)]
        action: PluginCommand,
    },
    /// Browse and manage the container registry.
    Registry {
        /// Registry operation.
        #[command(subcommand)]
        action: RegistryCommand,
    },
    /// Infrastructure-as-Code contract tooling.
    Iac {
        /// IaC operation.
        #[command(subcommand)]
        action: IacCommand,
    },
    /// Manage typed per-site web application firewall rules.
    Waf {
        /// WAF operation.
        #[command(subcommand)]
        action: WafCommand,
    },
    /// Manage allowlisted least-privilege containers.
    Docker {
        /// Container operation.
        #[command(subcommand)]
        action: DockerCommand,
    },
    /// Manage per-site FTP accounts.
    Ftp {
        /// FTP account operation.
        #[command(subcommand)]
        action: FtpCommand,
    },
    /// Manage scoped personal API tokens.
    Token {
        /// Token lifecycle operation.
        #[command(subcommand)]
        action: TokenCommand,
    },
    /// Manage SMTP/webhook notifications.
    Notifications {
        /// Notification operation.
        #[command(subcommand)]
        action: NotificationCommand,
    },
}

/// Notification CLI operations.
#[derive(Debug, Subcommand)]
pub enum NotificationCommand {
    /// Manage Owner channels.
    Channel {
        /// Channel operation.
        #[command(subcommand)]
        action: NotificationChannelCommand,
    },
    /// Manage owner subscriptions.
    Subscription {
        /// Subscription operation.
        #[command(subcommand)]
        action: NotificationSubscriptionCommand,
    },
    /// Print rolling channel health.
    Health,
}

/// Notification channel operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum NotificationChannelCommand {
    Add {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        endpoint: String,
        #[arg(long, default_value_t = 587)]
        port: u16,
        #[arg(long, default_value = "")]
        username: String,
        #[arg(long)]
        credential: String,
        #[arg(long, default_value = "")]
        from_addr: String,
        #[arg(long = "allow", required = true)]
        allowlist: Vec<String>,
    },
    List,
    Test {
        #[arg(long)]
        id: String,
        #[arg(long)]
        destination: String,
    },
    Rm {
        #[arg(long)]
        id: String,
    },
}

/// Notification subscription operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum NotificationSubscriptionCommand {
    Add {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        destination: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        filter_json: String,
    },
    List,
    Rm {
        #[arg(long)]
        id: String,
    },
}

/// Personal API-token lifecycle operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum TokenCommand {
    Create {
        #[arg(long)]
        label: String,
        #[arg(long = "scope", required = true)]
        scopes: Vec<String>,
        #[arg(long = "cidr")]
        cidr_allowlist: Vec<String>,
        #[arg(long, default_value_t = 90)]
        expires_in_days: i64,
    },
    List,
    Revoke {
        #[arg(long)]
        id: String,
    },
    Rotate {
        #[arg(long)]
        id: String,
    },
}

/// Per-site FTP account operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum FtpCommand {
    Create {
        #[arg(long)]
        site: String,
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
        #[arg(long)]
        read_only: bool,
    },
    List {
        #[arg(long)]
        site: String,
    },
    Disable {
        #[arg(long)]
        site: String,
        #[arg(long)]
        id: String,
    },
    Enable {
        #[arg(long)]
        site: String,
        #[arg(long)]
        id: String,
    },
    Delete {
        #[arg(long)]
        site: String,
        #[arg(long)]
        id: String,
    },
}

/// Container runtime operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum DockerCommand {
    Pull {
        #[arg(long)]
        image: String,
    },
    List,
    Inspect {
        #[arg(long)]
        id: String,
    },
    Create {
        #[arg(long)]
        spec_json: String,
    },
    Start {
        #[arg(long)]
        id: String,
    },
    Stop {
        #[arg(long)]
        id: String,
    },
    Restart {
        #[arg(long)]
        id: String,
    },
    Logs {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 100)]
        tail: u64,
    },
    Exec {
        #[arg(long)]
        id: String,
        #[arg(long, required = true)]
        command: Vec<String>,
    },
    Rm {
        #[arg(long)]
        id: String,
        #[arg(long)]
        force: bool,
    },
    Allow {
        #[arg(long)]
        pattern: String,
        #[arg(long)]
        pin_digest_required: bool,
    },
}

/// Per-site WAF operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum WafCommand {
    /// Print the complete rule set as JSON.
    Rules {
        #[arg(long)]
        site: String,
    },
    /// Print per-rule hit totals as JSON.
    Hits {
        #[arg(long)]
        site: String,
    },
    /// Add one strict JSON rule.
    Add {
        #[arg(long)]
        site: String,
        #[arg(long)]
        rule_json: String,
    },
    /// Remove a rule by id.
    Remove {
        #[arg(long)]
        site: String,
        #[arg(long)]
        rule_id: String,
    },
    /// Enable a rule by id.
    Enable {
        #[arg(long)]
        site: String,
        #[arg(long)]
        rule_id: String,
    },
    /// Disable a rule by id.
    Disable {
        #[arg(long)]
        site: String,
        #[arg(long)]
        rule_id: String,
    },
    /// Compile and simulate one rule without changing live policy.
    Test {
        #[arg(long)]
        site: String,
        #[arg(long)]
        rule_json: String,
        #[arg(long)]
        request_json: String,
    },
}

/// Curated Software Center operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum SoftwareCommand {
    Catalog,
    Inventory,
    Diagnostics,
    Refresh,
    Search {
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        installed_only: bool,
        #[arg(long)]
        update_available_only: bool,
        #[arg(long, default_value_t = 0)]
        page: usize,
        #[arg(long, default_value_t = 60)]
        page_size: usize,
    },
    Show {
        #[arg(long)]
        id: String,
    },
    Preview {
        #[arg(long)]
        id: String,
    },
    Execute {
        #[arg(long)]
        digest: String,
        #[arg(long)]
        confirmation_token: String,
    },
    Install {
        #[arg(long)]
        id: String,
        #[arg(long)]
        version: Option<String>,
    },
    Adopt {
        #[arg(long)]
        id: String,
    },
    Update {
        #[arg(long)]
        id: String,
    },
    Uninstall {
        #[arg(long)]
        id: String,
    },
    Deploy {
        #[arg(long)]
        application: String,
        #[arg(long)]
        domain: String,
        #[arg(long)]
        php_version: String,
        #[arg(long, default_value = "en_US")]
        locale: String,
    },
    Cancel {
        #[arg(long)]
        job: String,
    },
    Retry {
        #[arg(long)]
        job: String,
    },
    Rollback {
        #[arg(long)]
        job: String,
    },
    Jobs,
}

/// Hosted-mail operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum MailCommand {
    Readiness,
    DomainAdd {
        #[arg(long)]
        name: String,
    },
    Domains,
    MailboxAdd {
        #[arg(long)]
        domain: String,
        #[arg(long)]
        local: String,
        #[arg(long)]
        quota: u64,
        #[arg(long)]
        password: String,
    },
    Mailboxes {
        #[arg(long)]
        domain: String,
    },
    AliasAdd {
        #[arg(long)]
        domain: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        destination: String,
    },
    Quota {
        #[arg(long)]
        address: String,
        #[arg(long)]
        bytes: u64,
    },
    Password {
        #[arg(long)]
        address: String,
        #[arg(long)]
        password: String,
    },
    Status,
}

/// Provider-backed DNS operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum DnsCommand {
    ProviderAdd {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        credential: String,
    },
    Providers,
    ProviderTest,
    ProviderRotate {
        #[arg(long)]
        credential: String,
    },
    ProviderDisable,
    ProviderEnable,
    ProviderDelete {
        #[arg(long)]
        confirm: bool,
    },
    Sync,
    Zones,
    RecordAdd {
        #[arg(long)]
        zone: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        value: String,
        #[arg(long)]
        ttl: u32,
    },
    RecordUpdate {
        #[arg(long)]
        zone: String,
        #[arg(long)]
        record: String,
        #[arg(long)]
        value: String,
        #[arg(long)]
        ttl: u32,
    },
    Records {
        #[arg(long)]
        zone: String,
    },
    Check {
        #[arg(long)]
        zone: String,
    },
    RecordDelete {
        #[arg(long)]
        zone: String,
        #[arg(long)]
        record: String,
        #[arg(long)]
        confirm: bool,
    },
}

/// Registered system-service operations.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum ServicesCommand {
    List,
    Status {
        #[arg(long)]
        id: String,
    },
    Preview {
        #[arg(long)]
        id: String,
        #[arg(long)]
        action: String,
    },
    Start {
        #[arg(long)]
        id: String,
    },
    Stop {
        #[arg(long)]
        id: String,
        #[arg(long)]
        confirm: bool,
    },
    Restart {
        #[arg(long)]
        id: String,
        #[arg(long)]
        confirm: bool,
    },
    Reload {
        #[arg(long)]
        id: String,
    },
    Enable {
        #[arg(long)]
        id: String,
    },
    Disable {
        #[arg(long)]
        id: String,
    },
    History {
        #[arg(long)]
        id: String,
    },
    Logs {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
}

/// Host-security subcommands.
#[derive(Debug, Subcommand)]
pub enum SecurityCommand {
    /// Report nftables support and managed scope.
    Status,
    /// Manage firewall rules.
    Rule {
        /// Rule operation.
        #[command(subcommand)]
        action: SecurityRuleCommand,
    },
    /// Print the complete candidate ruleset.
    Preview,
    /// Syntax-check and apply saved rules.
    Apply,
    /// Restore last-known-good rules.
    Rollback,
    /// List login abuse blocks.
    Blocks,
    /// Manage login address allowlists.
    Allowlist {
        /// Allowlist operation.
        #[command(subcommand)]
        action: SecurityAllowlistCommand,
    },
    /// End a login abuse block.
    Unblock {
        /// Durable account: or ip: key.
        #[arg(long)]
        key: String,
    },
}

/// Login address allowlist CRUD.
#[derive(Debug, Subcommand)]
pub enum SecurityAllowlistCommand {
    /// List canonical networks.
    List,
    /// Add a network.
    Add {
        /// IPv4 or IPv6 CIDR.
        #[arg(long)]
        network: String,
    },
    /// Delete a network.
    Delete {
        /// IPv4 or IPv6 CIDR.
        #[arg(long)]
        network: String,
    },
}

/// Firewall rule CRUD.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum SecurityRuleCommand {
    /// Add an enabled rule.
    Add {
        #[arg(long)]
        protocol: String,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        source: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        comment: String,
    },
    /// List rules.
    List,
    /// Change a rule comment.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        comment: String,
    },
    /// Enable a rule.
    Enable {
        #[arg(long)]
        id: String,
    },
    /// Disable a rule.
    Disable {
        #[arg(long)]
        id: String,
    },
    /// Delete a rule.
    Delete {
        #[arg(long)]
        id: String,
    },
}

/// Log browsing and export subcommands.
#[derive(Debug, Subcommand)]
pub enum LogsCommand {
    /// List authorized registered sources.
    Sources,
    /// Tail a registered source.
    Tail {
        /// Source name or id.
        #[arg(long)]
        source: String,
        /// Maximum lines.
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Tail the panel error source.
    Errors {
        /// Maximum lines.
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Show retained traffic summaries.
    Traffic,
    /// Show recent audit events.
    Audit,
    /// Export one bounded, redacted source.
    Export {
        /// Source name or id.
        #[arg(long)]
        source: String,
        /// Destination file.
        #[arg(long)]
        output: std::path::PathBuf,
    },
}

/// Backup subcommands.
#[derive(Debug, Subcommand)]
pub enum BackupCommand {
    /// Manage recurring plans.
    Plan {
        /// Plan action.
        #[command(subcommand)]
        action: BackupPlanCommand,
    },
    /// Run a plan now.
    Run {
        /// Plan identifier.
        #[arg(long)]
        plan_id: String,
    },
    /// List runs.
    Runs,
    /// Show run status.
    Status {
        /// Run identifier.
        #[arg(long)]
        id: String,
    },
    /// Verify checksums.
    Verify {
        /// Run identifier.
        #[arg(long)]
        id: String,
    },
    /// Preview or start restore.
    Restore {
        /// Restore action.
        #[command(subcommand)]
        action: BackupRestoreCommand,
    },
    /// Delete a finalized run.
    Delete {
        /// Run identifier.
        #[arg(long)]
        id: String,
    },
}

/// Backup plan CRUD.
#[derive(Debug, Subcommand)]
pub enum BackupPlanCommand {
    /// Create a panel metadata plan.
    Create {
        /// Plan name.
        #[arg(long)]
        name: String,
        /// Cron expression.
        #[arg(long)]
        schedule: String,
        /// IANA timezone.
        #[arg(long)]
        timezone: String,
        /// Include panel metadata.
        #[arg(long)]
        panel_metadata: bool,
        /// Copies retained.
        #[arg(long)]
        retention: usize,
    },
    /// List plans.
    List,
    /// Get a plan.
    Get {
        /// Plan id.
        #[arg(long)]
        id: String,
    },
    /// Update retention.
    Update {
        /// Plan id.
        #[arg(long)]
        id: String,
        /// Copies retained.
        #[arg(long)]
        retention: usize,
    },
    /// Enable a plan.
    Enable {
        /// Plan id.
        #[arg(long)]
        id: String,
    },
    /// Disable a plan.
    Disable {
        /// Plan id.
        #[arg(long)]
        id: String,
    },
    /// Delete a plan.
    Delete {
        /// Plan id.
        #[arg(long)]
        id: String,
    },
}

/// Restore actions.
#[derive(Debug, Subcommand)]
pub enum BackupRestoreCommand {
    /// Preflight only.
    Preview {
        /// Run id.
        #[arg(long)]
        id: String,
    },
    /// Start safe restore.
    Start {
        /// Run id.
        #[arg(long)]
        id: String,
    },
}

/// Subcommands for scheduled jobs and execution history.
#[derive(Debug, Subcommand)]
#[allow(missing_docs)]
pub enum CronCommand {
    /// Create a command job.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        schedule: String,
        #[arg(long)]
        timezone: String,
        #[arg(long)]
        executable: String,
        #[arg(long = "arg")]
        arguments: Vec<String>,
        #[arg(long)]
        working_directory: String,
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    /// List jobs.
    List,
    /// Show one job.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Change a job schedule.
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        schedule: String,
    },
    /// Disable a job.
    Disable {
        #[arg(long)]
        id: String,
    },
    /// Enable a job.
    Enable {
        #[arg(long)]
        id: String,
    },
    /// Run a job immediately.
    Run {
        #[arg(long)]
        id: String,
    },
    /// List run history.
    Runs {
        #[arg(long)]
        job_id: Option<String>,
    },
    /// Delete a job.
    Delete {
        #[arg(long)]
        id: String,
    },
}

/// Subcommands for managing OpenPanel users.
#[derive(Debug, Subcommand)]
pub enum UserCommand {
    /// Create a new user account.
    Create {
        /// Login username for the new account.
        #[arg(long)]
        username: String,
        /// Email address associated with the account.
        #[arg(long)]
        email: String,
        /// Password for the new account (passed in plain text on the CLI).
        #[arg(long)]
        password: String,
        /// Role granted to the user (e.g. `admin`, `user`).
        #[arg(long, value_parser = clap::value_parser!(openpanel_domain::Role))]
        role: openpanel_domain::Role,
    },
    /// List all users.
    List,
    /// Disable a user without deleting them.
    Disable {
        /// ID of the user to disable.
        #[arg(long)]
        id: String,
    },
    /// Delete a user permanently.
    Delete {
        /// ID of the user to delete.
        #[arg(long)]
        id: String,
    },
    /// Manage a user's second-factor authentication.
    #[command(name = "2fa")]
    TwoFactor {
        /// Subcommand (enroll/list/revoke/regenerate).
        #[command(subcommand)]
        action: TwoFactorCommand,
    },
}

/// Subcommands for `openpanel user 2fa`.
#[derive(Debug, Subcommand)]
pub enum TwoFactorCommand {
    /// Enroll a TOTP factor for the user. Prints the secret, provisioning
    /// URI, and recovery codes to stdout exactly once.
    Enroll {
        /// Factor kind. TOTP enrollment is supported from the terminal;
        /// WebAuthn registration requires the browser security page.
        #[arg(value_parser = ["totp"])]
        kind: String,
        /// ID of the user to enroll.
        #[arg(long)]
        id: String,
    },
    /// List the user's enrolled factors.
    List {
        /// ID of the user whose factors to list.
        #[arg(long)]
        id: String,
    },
    /// Revoke an enrolled factor.
    Revoke {
        /// ID of the user owning the factor.
        #[arg(long)]
        id: String,
        /// Factor ID to revoke.
        #[arg(long)]
        factor_id: String,
    },
    /// Regenerate the recovery code set for a user. Prints the new
    /// codes to stdout exactly once.
    Recovery {
        /// Recovery-code operation.
        #[command(subcommand)]
        action: RecoveryCodeCommand,
    },
}

/// Recovery-code operations under `openpanel user 2fa recovery`.
#[derive(Debug, Subcommand)]
pub enum RecoveryCodeCommand {
    /// Replace all existing recovery codes and print the new set once.
    Regenerate {
        /// ID of the user whose recovery codes to regenerate.
        #[arg(long)]
        id: String,
    },
}

/// Subcommands for managing hosted sites.
#[derive(Debug, Subcommand)]
pub enum SiteCommand {
    /// Provision a new site.
    Create {
        /// Primary domain for the site (e.g. `example.com`).
        #[arg(long)]
        domain: String,
        /// Username of the site owner.
        #[arg(long)]
        owner: String,
        /// Additional domains pointing at this site, comma-separated.
        #[arg(long, value_delimiter = ',', default_values_t = Vec::<String>::new())]
        aliases: Vec<String>,
        /// Enable PHP processing for this site.
        #[arg(long, default_value_t = false)]
        php: bool,
        /// PHP version to use (e.g. `8.3`); only meaningful when `php` is set.
        #[arg(long)]
        php_version: Option<String>,
        /// Document root path relative to the site's home (defaults to `public_html`).
        #[arg(long)]
        document_root: Option<String>,
    },
    /// List all sites.
    List,
    /// Delete a site permanently.
    Delete {
        /// ID of the site to delete.
        #[arg(long)]
        id: String,
    },
    /// Enable a previously disabled site.
    Enable {
        /// ID of the site to enable.
        #[arg(long)]
        id: String,
    },
    /// Disable a site without deleting it.
    Disable {
        /// ID of the site to disable.
        #[arg(long)]
        id: String,
    },
    /// Per-site staging subcommands.
    Staging {
        /// Staging subcommand.
        #[command(subcommand)]
        action: StagingCommand,
    },
    /// Per-site collaborator subcommands.
    Collaborator {
        /// Collaborator subcommand.
        #[command(subcommand)]
        action: CollaboratorCommand,
    },
}

/// Subcommands for managing per-site collaborators.
#[derive(Debug, Subcommand)]
pub enum CollaboratorCommand {
    /// Invite a collaborator to a site with one or more scopes.
    Invite {
        /// Site id.
        #[arg(long)]
        site: String,
        /// Collaborator email.
        #[arg(long)]
        email: String,
        /// Scopes (comma-separated): file, database, mail, cron.
        #[arg(long, value_delimiter = ',', default_values_t = Vec::<String>::new())]
        scopes: Vec<String>,
    },
    /// List collaborators granted on a site.
    List {
        /// Site id.
        #[arg(long)]
        site: String,
    },
    /// Revoke a collaborator from a single site.
    Revoke {
        /// Site id.
        #[arg(long)]
        site: String,
        /// Collaborator id.
        #[arg(long)]
        collaborator: String,
    },
}
#[derive(Debug, Subcommand)]
pub enum DatabaseCommand {
    /// Provision a new MySQL database + DB user.
    Create {
        /// Username of the site owner the database is for.
        #[arg(long)]
        owner: String,
        /// Suffix appended to the database / username (final name is `openpanel_<suffix>`).
        #[arg(long)]
        suffix: String,
        /// MySQL charset to use (defaults to the server default).
        #[arg(long)]
        charset: Option<String>,
    },
    /// List all managed databases.
    List,
    /// Delete a managed database and its DB user.
    Delete {
        /// ID of the database to delete.
        #[arg(long)]
        id: String,
    },
    /// Rotate the DB user's password. Prints the new password to stdout.
    ChangePassword {
        /// ID of the database whose password should be rotated.
        #[arg(long)]
        id: String,
    },
    /// Database point-in-time recovery subcommands.
    Pitr {
        /// PITR subcommand.
        #[command(subcommand)]
        action: PitrCommand,
    },
    /// Per-site staging subcommands.
    Staging {
        /// Staging subcommand.
        #[command(subcommand)]
        action: StagingCommand,
    },
}

/// Subcommands for the `openpanel site staging` family: create
/// the staging slot, sync a snapshot, promote it to production,
/// and destroy the slot.
#[derive(Debug, Subcommand)]
pub enum StagingCommand {
    /// Create a staging slot for a site.
    Create {
        /// Site id.
        #[arg(long)]
        id: String,
        /// Optional subdomain prefix (default: `staging`).
        #[arg(long)]
        subdomain: Option<String>,
    },
    /// Take a fresh snapshot of production into staging.
    Sync {
        /// Site id.
        #[arg(long)]
        id: String,
    },
    /// Promote the current staging snapshot to production.
    Promote {
        /// Site id.
        #[arg(long)]
        id: String,
        /// Snapshot id to promote.
        #[arg(long)]
        snapshot: i64,
        /// Operator confirmation instant (RFC 3339).
        #[arg(long)]
        confirmed_at: String,
    },
    /// Destroy the staging slot.
    Delete {
        /// Site id.
        #[arg(long)]
        id: String,
    },
}

/// Subcommands for the `openpanel db pitr` family: binlog
/// streaming, point-in-time restore, and incremental file-backup
/// deltas.
#[derive(Debug, Subcommand)]
pub enum PitrCommand {
    /// Show the active binlog stream for a database (if any).
    Status {
        /// Database id.
        #[arg(long)]
        id: String,
    },
    /// Inspect the available binlog / WAL range for a database.
    Inspect {
        /// Database id.
        #[arg(long)]
        id: String,
    },
    /// Request a point-in-time restore. `--confirm` promotes the
    /// staging database to live in the same call.
    Restore {
        /// Database id.
        #[arg(long)]
        id: String,
        /// Wall-clock instant to restore to (RFC 3339).
        #[arg(long)]
        timestamp: String,
        /// Promote the staging database to live after restore.
        #[arg(long)]
        confirm: bool,
    },
}

/// Subcommands for managing TLS certificates for sites.
#[derive(Debug, Subcommand)]
pub enum SslCommand {
    /// List every managed certificate (metadata only; private keys
    /// are never printed).
    List,
    /// Issue a certificate for `domain` via ACME HTTP-01.
    Issue {
        /// Domain to issue for.
        domain: String,
        /// Target the production Let's Encrypt environment instead of
        /// the default staging endpoint.
        #[arg(long)]
        production: bool,
    },
    /// Upload a manually-managed PEM bundle (cert + chain + key) for
    /// a domain. The private key is stored encrypted at rest.
    Upload {
        /// Domain this certificate covers.
        domain: String,
        /// Path to the leaf certificate PEM file.
        #[arg(long)]
        cert: String,
        /// Path to the intermediate chain PEM file (optional).
        #[arg(long)]
        chain: Option<String>,
        /// Path to the private key PEM file.
        #[arg(long)]
        key: String,
    },
    /// Generate a self-signed certificate for `domain` (365-day
    /// validity by default).
    SelfSigned {
        /// Domain to issue the self-signed cert for.
        domain: String,
    },
    /// Revoke and delete a certificate. The row + on-disk files are
    /// removed; ACME formal revocation is not performed (v0.1).
    Revoke {
        /// Domain of the certificate to remove.
        domain: String,
    },
    /// Force-renew an ACME-issued certificate regardless of the
    /// renewal window. Manual / self-signed certs return an error.
    Renew {
        /// Domain of the certificate to renew.
        domain: String,
    },
}
/// Subcommands for host resource monitoring.
#[derive(Debug, Subcommand)]
pub enum MonitoringCommand {
    /// Print the current host snapshot (timestamp, cpu %, memory %, load).
    Overview,
    /// Print recent samples for one metric.
    History {
        /// Metric kind identifier (`Cpu`, `Memory`, `Disk`, `Network`).
        #[arg(long)]
        metric: String,
        /// Look-back window in seconds (default 3600).
        #[arg(long, default_value_t = 3600)]
        range: i64,
    },
}

/// Subcommands for browsing and editing files inside a site's document root.
#[derive(Debug, Subcommand)]
pub enum FileCommand {
    /// List entries at `path` within the site's document root.
    List {
        /// Site ID or domain identifying the site to operate on.
        #[arg(long)]
        site: String,
        /// Path relative to the document root (defaults to the root).
        #[arg(long, default_value_t = String::new())]
        path: String,
    },
    /// Print the contents of a file to stdout.
    Read {
        /// Site ID or domain identifying the site to operate on.
        #[arg(long)]
        site: String,
        /// Path of the file relative to the document root.
        #[arg(long, default_value_t = String::new())]
        path: String,
    },
    /// Write `content` to a file, creating it if necessary.
    Write {
        /// Site ID or domain identifying the site to operate on.
        #[arg(long)]
        site: String,
        /// Path of the file relative to the document root.
        #[arg(long, default_value_t = String::new())]
        path: String,
        /// Literal content to write to the file.
        #[arg(long)]
        content: String,
    },
    /// Create a directory (and parents) within the site's document root.
    Mkdir {
        /// Site ID or domain identifying the site to operate on.
        #[arg(long)]
        site: String,
        /// Path of the new directory relative to the document root.
        #[arg(long)]
        path: String,
    },
    /// Remove a file or directory.
    Rm {
        /// Site ID or domain identifying the site to operate on.
        #[arg(long)]
        site: String,
        /// Path of the entry to remove relative to the document root.
        #[arg(long)]
        path: String,
        /// Recursively remove directories and their contents.
        #[arg(long, default_value_t = false)]
        recursive: bool,
    },
    /// Rename or move a file/directory within the site's document root.
    Rename {
        /// Site ID or domain identifying the site to operate on.
        #[arg(long)]
        site: String,
        /// Source path relative to the document root.
        #[arg(long)]
        from: String,
        /// Destination path relative to the document root.
        #[arg(long)]
        to: String,
    },
    /// Change the permission bits of a file or directory.
    Chmod {
        /// Site ID or domain identifying the site to operate on.
        #[arg(long)]
        site: String,
        /// Path of the entry relative to the document root.
        #[arg(long)]
        path: String,
        /// Permission mode (e.g. `755`, `u=rw,g=r`).
        #[arg(long)]
        mode: String,
    },
}

/// Subcommands for the plugin extension framework + marketplace.
#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    /// Marketplace subcommand.
    Marketplace {
        /// Marketplace operation.
        #[command(subcommand)]
        action: MarketplaceCommand,
    },
    /// List installed plugins.
    List,
    /// Enable an installed plugin.
    Enable {
        /// Plugin id to enable.
        #[arg(long)]
        id: String,
    },
    /// Disable an installed plugin.
    Disable {
        /// Plugin id to disable.
        #[arg(long)]
        id: String,
    },
    /// Uninstall a plugin.
    Uninstall {
        /// Plugin id to remove.
        #[arg(long)]
        id: String,
    },
}

/// Subcommands for the marketplace discovery surface.
#[derive(Debug, Subcommand)]
pub enum MarketplaceCommand {
    /// Fetch a fresh catalog from the configured marketplace.
    Discover,
    /// Print the latest cached catalog as JSON.
    Cached,
    /// Print one plugin's detail from the cached catalog.
    Show {
        /// Plugin id to show.
        #[arg(long)]
        id: String,
    },
}

/// Subcommands for managing the container registry.
#[derive(Debug, Subcommand)]
pub enum RegistryCommand {
    /// Show the registry configuration.
    Config,
    /// List namespaces.
    Namespaces,
    /// Create a namespace.
    CreateNamespace {
        /// Namespace id.
        #[arg(long)]
        namespace: String,
        /// Owner user id.
        #[arg(long)]
        owner: String,
        /// Quota in bytes.
        #[arg(long)]
        quota_bytes: u64,
    },
    /// List images in a namespace.
    Images {
        /// Namespace id.
        #[arg(long)]
        namespace: String,
    },
}

/// Subcommands for the IaC contract.
#[derive(Debug, Subcommand)]
pub enum IacCommand {
    /// Print the SDK + provider surfaces generated from the
    /// built-in contract.
    Generate {
        /// Path to an OpenAPI JSON document; if omitted, the
        /// bundled sample is used.
        #[arg(long)]
        openapi: Option<String>,
    },
    /// Run the drift check against the committed artefacts.
    DriftCheck {
        /// Path to the OpenAPI JSON document.
        #[arg(long)]
        openapi: Option<String>,
        /// Path to the committed Rust SDK descriptor (JSON).
        #[arg(long)]
        committed_rust: Option<String>,
        /// Path to the committed Terraform provider descriptor
        /// (JSON).
        #[arg(long)]
        committed_provider: Option<String>,
    },
}
