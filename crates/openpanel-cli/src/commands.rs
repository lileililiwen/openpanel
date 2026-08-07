use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "openpanel", about = "OpenPanel server management CLI", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the API server + agent in the foreground.
    Serve,
    /// Apply pending database migrations and exit.
    Migrate,
    /// User management commands.
    User {
        #[command(subcommand)]
        action: UserCommand,
    },
    /// Site management commands.
    Site {
        #[command(subcommand)]
        action: SiteCommand,
    },
    /// Database (MySQL) management commands.
    Database {
        #[command(subcommand)]
        action: DatabaseCommand,
    },
    /// File manager commands (operate on a site's document root).
    File {
        #[command(subcommand)]
        action: FileCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum UserCommand {
    Create {
        #[arg(long)]
        username: String,
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: String,
        #[arg(long, value_parser = clap::value_parser!(openpanel_domain::Role))]
        role: openpanel_domain::Role,
    },
    List,
    Disable {
        #[arg(long)]
        id: String,
    },
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum SiteCommand {
    Create {
        #[arg(long)]
        domain: String,
        #[arg(long)]
        owner: String,
        #[arg(long, value_delimiter = ',', default_values_t = Vec::<String>::new())]
        aliases: Vec<String>,
        #[arg(long, default_value_t = false)]
        php: bool,
        #[arg(long)]
        php_version: Option<String>,
        #[arg(long)]
        document_root: Option<String>,
    },
    List,
    Delete {
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
}

#[derive(Debug, Subcommand)]
pub enum DatabaseCommand {
    /// Provision a new MySQL database + DB user.
    Create {
        #[arg(long)]
        owner: String,
        #[arg(long)]
        suffix: String,
        #[arg(long)]
        charset: Option<String>,
    },
    List,
    Delete {
        #[arg(long)]
        id: String,
    },
    /// Rotate the DB user's password. Prints the new password to stdout.
    ChangePassword {
        #[arg(long)]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum FileCommand {
    List {
        #[arg(long)]
        site: String,
        #[arg(long, default_value_t = String::new())]
        path: String,
    },
    Read {
        #[arg(long)]
        site: String,
        #[arg(long, default_value_t = String::new())]
        path: String,
    },
    Write {
        #[arg(long)]
        site: String,
        #[arg(long, default_value_t = String::new())]
        path: String,
        #[arg(long)]
        content: String,
    },
    Mkdir {
        #[arg(long)]
        site: String,
        #[arg(long)]
        path: String,
    },
    Rm {
        #[arg(long)]
        site: String,
        #[arg(long)]
        path: String,
        #[arg(long, default_value_t = false)]
        recursive: bool,
    },
    Rename {
        #[arg(long)]
        site: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    Chmod {
        #[arg(long)]
        site: String,
        #[arg(long)]
        path: String,
        #[arg(long)]
        mode: String,
    },
}
