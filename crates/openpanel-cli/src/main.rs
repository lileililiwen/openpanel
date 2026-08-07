//! openpanel — CLI entry point.

use clap::Parser;
use openpanel_cli::handlers;
use openpanel_cli::{Cli, Command, DatabaseCommand, FileCommand, SiteCommand, UserCommand};
use openpanel_core::{init_tracing, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load()?;
    init_tracing(config.log().level.as_str());

    match cli.command {
        Command::Serve => handlers::serve(config).await,
        Command::Migrate => handlers::migrate(config).await,
        Command::User { action } => match action {
            UserCommand::Create {
                username,
                email,
                password,
                role,
            } => handlers::create_user(config, username, email, password, role).await,
            UserCommand::List => handlers::list_users(config).await,
            UserCommand::Disable { id } => handlers::disable_user(config, id).await,
            UserCommand::Delete { id } => handlers::delete_user(config, id).await,
        },
        Command::Site { action } => match action {
            SiteCommand::Create {
                domain,
                owner,
                aliases,
                php,
                php_version,
                document_root,
            } => handlers::create_site(config, domain, owner, aliases, php, php_version, document_root).await,
            SiteCommand::List => handlers::list_sites(config).await,
            SiteCommand::Delete { id } => handlers::delete_site(config, id).await,
            SiteCommand::Enable { id } => handlers::enable_site(config, id).await,
            SiteCommand::Disable { id } => handlers::disable_site(config, id).await,
        },
        Command::Database { action } => match action {
            DatabaseCommand::Create {
                owner,
                suffix,
                charset,
            } => handlers::create_database(config, owner, suffix, charset).await,
            DatabaseCommand::List => handlers::list_databases(config).await,
            DatabaseCommand::Delete { id } => handlers::delete_database(config, id).await,
            DatabaseCommand::ChangePassword { id } => {
                handlers::change_database_password(config, id).await
            }
        },
        Command::File { action } => match action {
            FileCommand::List { site, path } => handlers::file_list(config, site, path).await,
            FileCommand::Read { site, path } => handlers::file_read(config, site, path).await,
            FileCommand::Write {
                site,
                path,
                content,
            } => handlers::file_write(config, site, path, content).await,
            FileCommand::Mkdir { site, path } => handlers::file_mkdir(config, site, path).await,
            FileCommand::Rm {
                site,
                path,
                recursive,
            } => handlers::file_rm(config, site, path, recursive).await,
            FileCommand::Rename { site, from, to } => {
                handlers::file_rename(config, site, from, to).await
            }
            FileCommand::Chmod { site, path, mode } => {
                handlers::file_chmod(config, site, path, mode).await
            }
        },
    }
}