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
}

/// Subcommands for managing MySQL databases used by sites.
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
