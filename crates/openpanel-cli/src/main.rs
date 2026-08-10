//! openpanel — CLI entry point.

use clap::Parser;
use openpanel_cli::{
    BackupCommand, BackupPlanCommand, BackupRestoreCommand, Cli, Command, CronCommand,
    DatabaseCommand, DnsCommand, FileCommand, LogsCommand, MailCommand, MonitoringCommand,
    SecurityAllowlistCommand, SecurityCommand, SecurityRuleCommand, ServicesCommand, SiteCommand,
    SoftwareCommand, SslCommand, UserCommand, handlers,
};
use openpanel_core::{Config, init_tracing};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load()?;
    init_tracing(config.log().level.as_str());

    match cli.command {
        Command::Serve => handlers::serve(config).await,
        Command::Dev => handlers::dev_up(config).await,
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
            } => {
                handlers::create_site(
                    config,
                    domain,
                    owner,
                    aliases,
                    php,
                    php_version,
                    document_root,
                )
                .await
            }
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
        Command::Ssl { action } => match action {
            SslCommand::List => handlers::ssl_list(config).await,
            SslCommand::Issue { domain, production } => {
                handlers::ssl_issue(config, domain, production).await
            }
            SslCommand::Upload {
                domain,
                cert,
                chain,
                key,
            } => handlers::ssl_upload(config, domain, cert, chain, key).await,
            SslCommand::SelfSigned { domain } => handlers::ssl_self_signed(config, domain).await,
            SslCommand::Revoke { domain } => handlers::ssl_revoke(config, domain).await,
            SslCommand::Renew { domain } => handlers::ssl_renew(config, domain).await,
        },
        Command::Monitoring { action } => match action {
            MonitoringCommand::Overview => handlers::monitoring_overview(config).await,
            MonitoringCommand::History { metric, range } => {
                handlers::monitoring_history(config, metric, range).await
            }
        },
        Command::Cron { action } => match action {
            CronCommand::Create {
                name,
                schedule,
                timezone,
                executable,
                arguments,
                working_directory,
                timeout,
            } => {
                handlers::cron_create(
                    config,
                    name,
                    schedule,
                    timezone,
                    executable,
                    arguments,
                    working_directory,
                    timeout,
                )
                .await
            }
            CronCommand::List => handlers::cron_list(config).await,
            CronCommand::Get { id } => handlers::cron_get(config, id).await,
            CronCommand::Update { id, schedule } => {
                handlers::cron_update(config, id, schedule).await
            }
            CronCommand::Disable { id } => handlers::cron_enabled(config, id, false).await,
            CronCommand::Enable { id } => handlers::cron_enabled(config, id, true).await,
            CronCommand::Run { id } => handlers::cron_run(config, id).await,
            CronCommand::Runs { job_id } => handlers::cron_runs(config, job_id).await,
            CronCommand::Delete { id } => handlers::cron_delete(config, id).await,
        },
        Command::Backup { action } => match action {
            BackupCommand::Plan { action } => match action {
                BackupPlanCommand::Create {
                    name,
                    schedule,
                    timezone,
                    panel_metadata,
                    retention,
                } => {
                    handlers::backup_plan_create(
                        config,
                        name,
                        schedule,
                        timezone,
                        panel_metadata,
                        retention,
                    )
                    .await
                }
                BackupPlanCommand::List => handlers::backup_plan_list(config).await,
                BackupPlanCommand::Get { id } => handlers::backup_plan_get(config, id).await,
                BackupPlanCommand::Update { id, retention } => {
                    handlers::backup_plan_update(config, id, retention).await
                }
                BackupPlanCommand::Enable { id } => {
                    handlers::backup_plan_enabled(config, id, true).await
                }
                BackupPlanCommand::Disable { id } => {
                    handlers::backup_plan_enabled(config, id, false).await
                }
                BackupPlanCommand::Delete { id } => handlers::backup_plan_delete(config, id).await,
            },
            BackupCommand::Run { plan_id } => handlers::backup_run(config, plan_id).await,
            BackupCommand::Runs => handlers::backup_runs(config).await,
            BackupCommand::Status { id } => handlers::backup_status(config, id).await,
            BackupCommand::Verify { id } => handlers::backup_verify(config, id).await,
            BackupCommand::Restore { action } => match action {
                BackupRestoreCommand::Preview { id } => {
                    handlers::backup_restore_preview(config, id).await
                }
                BackupRestoreCommand::Start { id } => {
                    handlers::backup_restore_start(config, id).await
                }
            },
            BackupCommand::Delete { id } => handlers::backup_delete(config, id).await,
        },
        Command::Logs { action } => match action {
            LogsCommand::Sources => handlers::logs_sources(config).await,
            LogsCommand::Tail { source, limit } => handlers::logs_tail(config, source, limit).await,
            LogsCommand::Errors { limit } => {
                handlers::logs_tail(config, "panel-error".into(), limit).await
            }
            LogsCommand::Traffic => handlers::logs_traffic(config).await,
            LogsCommand::Audit => handlers::logs_audit(config).await,
            LogsCommand::Export { source, output } => {
                handlers::logs_export(config, source, output).await
            }
        },
        Command::Security { action } => match action {
            SecurityCommand::Status => handlers::security_status(config).await,
            SecurityCommand::Rule { action } => match action {
                SecurityRuleCommand::Add {
                    protocol,
                    port,
                    source,
                    action,
                    comment,
                } => {
                    handlers::security_rule_add(config, protocol, port, source, action, comment)
                        .await
                }
                SecurityRuleCommand::List => handlers::security_rule_list(config).await,
                SecurityRuleCommand::Update { id, comment } => {
                    handlers::security_rule_update(config, id, comment).await
                }
                SecurityRuleCommand::Enable { id } => {
                    handlers::security_rule_enabled(config, id, true).await
                }
                SecurityRuleCommand::Disable { id } => {
                    handlers::security_rule_enabled(config, id, false).await
                }
                SecurityRuleCommand::Delete { id } => {
                    handlers::security_rule_delete(config, id).await
                }
            },
            SecurityCommand::Preview => handlers::security_preview(config).await,
            SecurityCommand::Apply => handlers::security_apply(config).await,
            SecurityCommand::Rollback => handlers::security_rollback(config).await,
            SecurityCommand::Blocks => handlers::security_blocks(config).await,
            SecurityCommand::Allowlist { action } => match action {
                SecurityAllowlistCommand::List => handlers::security_allowlist_list(config).await,
                SecurityAllowlistCommand::Add { network } => {
                    handlers::security_allowlist_add(config, network).await
                }
                SecurityAllowlistCommand::Delete { network } => {
                    handlers::security_allowlist_delete(config, network).await
                }
            },
            SecurityCommand::Unblock { key } => handlers::security_unblock(config, key).await,
        },
        Command::Services { action } => match action {
            ServicesCommand::List => handlers::services_list(config).await,
            ServicesCommand::Status { id } => handlers::services_status(config, id).await,
            ServicesCommand::Preview { id, action } => {
                handlers::services_preview(config, id, action).await
            }
            ServicesCommand::Start { id } => {
                handlers::services_action(config, id, "start".into(), false).await
            }
            ServicesCommand::Stop { id, confirm } => {
                handlers::services_action(config, id, "stop".into(), confirm).await
            }
            ServicesCommand::Restart { id, confirm } => {
                handlers::services_action(config, id, "restart".into(), confirm).await
            }
            ServicesCommand::Reload { id } => {
                handlers::services_action(config, id, "reload".into(), false).await
            }
            ServicesCommand::Enable { id } => {
                handlers::services_action(config, id, "enable".into(), false).await
            }
            ServicesCommand::Disable { id } => {
                handlers::services_action(config, id, "disable".into(), false).await
            }
            ServicesCommand::History { id } => handlers::services_history(config, id).await,
            ServicesCommand::Logs { id, limit } => handlers::services_logs(config, id, limit).await,
        },
        Command::Dns { action } => match action {
            DnsCommand::ProviderAdd {
                kind,
                name,
                credential,
            } => handlers::dns_provider_add(config, kind, name, credential).await,
            DnsCommand::Providers => handlers::dns_providers(config).await,
            DnsCommand::ProviderTest => handlers::dns_provider_test(config).await,
            DnsCommand::ProviderRotate { credential } => {
                handlers::dns_provider_rotate(config, credential).await
            }
            DnsCommand::ProviderDisable => handlers::dns_provider_enabled(config, false).await,
            DnsCommand::ProviderEnable => handlers::dns_provider_enabled(config, true).await,
            DnsCommand::ProviderDelete { confirm } => {
                handlers::dns_provider_delete(config, confirm).await
            }
            DnsCommand::Sync => handlers::dns_sync(config).await,
            DnsCommand::Zones => handlers::dns_zones(config).await,
            DnsCommand::RecordAdd {
                zone,
                name,
                kind,
                value,
                ttl,
            } => handlers::dns_record_add(config, zone, name, kind, value, ttl).await,
            DnsCommand::RecordUpdate {
                zone,
                record,
                value,
                ttl,
            } => handlers::dns_record_update(config, zone, record, value, ttl).await,
            DnsCommand::Records { zone } => handlers::dns_records(config, zone).await,
            DnsCommand::Check { zone } => handlers::dns_check(config, zone).await,
            DnsCommand::RecordDelete {
                zone,
                record,
                confirm,
            } => handlers::dns_record_delete(config, zone, record, confirm).await,
        },
        Command::Mail { action } => match action {
            MailCommand::Readiness => handlers::mail_readiness(config).await,
            MailCommand::DomainAdd { name } => handlers::mail_domain_add(config, name).await,
            MailCommand::Domains => handlers::mail_domains(config).await,
            MailCommand::MailboxAdd {
                domain,
                local,
                quota,
                password,
            } => handlers::mail_mailbox_add(config, domain, local, quota, password).await,
            MailCommand::Mailboxes { domain } => handlers::mail_mailboxes(config, domain).await,
            MailCommand::AliasAdd {
                domain,
                source,
                destination,
            } => handlers::mail_alias_add(config, domain, source, destination).await,
            MailCommand::Quota { address, bytes } => {
                handlers::mail_quota(config, address, bytes).await
            }
            MailCommand::Password { address, password } => {
                handlers::mail_password(config, address, password).await
            }
            MailCommand::Status => handlers::mail_status(config).await,
        },
        Command::Software { action } => match action {
            SoftwareCommand::Catalog => handlers::software_catalog(config).await,
            SoftwareCommand::Inventory => handlers::software_inventory(config).await,
            SoftwareCommand::Preview { id } => handlers::software_preview(config, id).await,
            SoftwareCommand::Execute {
                digest,
                confirmation_token,
            } => handlers::software_execute(config, digest, confirmation_token).await,
            SoftwareCommand::Install { id } => handlers::software_install(config, id).await,
            SoftwareCommand::Adopt { id } => {
                handlers::software_component_action(
                    config,
                    id,
                    openpanel_app::software_center::ComponentAction::Adopt,
                )
                .await
            }
            SoftwareCommand::Update { id } => {
                handlers::software_component_action(
                    config,
                    id,
                    openpanel_app::software_center::ComponentAction::Update,
                )
                .await
            }
            SoftwareCommand::Uninstall { id } => {
                handlers::software_component_action(
                    config,
                    id,
                    openpanel_app::software_center::ComponentAction::Remove,
                )
                .await
            }
            SoftwareCommand::Deploy {
                application,
                domain,
                php_version,
                locale,
            } => handlers::software_deploy(config, application, domain, php_version, locale).await,
            SoftwareCommand::Cancel { job } => handlers::software_cancel(config, job).await,
            SoftwareCommand::Retry { job } => handlers::software_retry(config, job).await,
            SoftwareCommand::Rollback { job } => handlers::software_rollback(config, job).await,
            SoftwareCommand::Diagnostics => handlers::software_diagnostics(config).await,
            SoftwareCommand::Jobs => handlers::software_jobs(config).await,
        },
    }
}
