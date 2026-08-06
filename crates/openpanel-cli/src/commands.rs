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
}

#[derive(Debug, Subcommand)]
pub enum UserCommand {
    /// Create a new user.
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
    /// List users (id, username, email, role).
    List,
    /// Disable a user.
    Disable {
        #[arg(long)]
        id: String,
    },
    /// Delete a user.
    Delete {
        #[arg(long)]
        id: String,
    },
}