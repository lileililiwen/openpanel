//! openpanel — CLI entry point.

use clap::Parser;
use openpanel_cli::{
    AuthCommand, BackupCommand, BackupDrillCommand, BackupPlanCommand, BackupRestoreCommand,
    BrandingCommand, CdnCommand, Cli, CollaboratorCommand, Command, ContainerRuntimeCommand,
    CronCommand, DatabaseCommand, DnsCommand, DockerCommand, FileCommand, FtpCommand, IacCommand,
    LogsCommand, MailCommand, MarketplaceCommand, MonitoringCommand, NotificationChannelCommand,
    NotificationCommand, NotificationSubscriptionCommand, PitrCommand, PluginCommand,
    RecoveryCodeCommand, RegistryCommand, ScanCommand, SecurityAllowlistCommand, SecurityCommand,
    SecurityRuleCommand, ServerSnapshotCommand, ServicesCommand, SiteCacheCommand,
    SiteCloneCommand, SiteCommand, SiteHttpCommand, SitePreviewCommand, SiteTemplateCommand,
    SoftwareCommand, SslCommand, StagingCommand, TerminalCommand, TokenCommand, TwoFactorCommand,
    UserCommand, WafCommand, WebappCommand, handlers,
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
            UserCommand::TwoFactor { action } => match action {
                TwoFactorCommand::Enroll { kind: _, id } => {
                    handlers::enroll_user_totp(config, id).await
                }
                TwoFactorCommand::List { id } => handlers::list_user_factors(config, id).await,
                TwoFactorCommand::Revoke { id, factor_id } => {
                    handlers::revoke_user_factor(config, id, factor_id).await
                }
                TwoFactorCommand::Recovery {
                    action: RecoveryCodeCommand::Regenerate { id },
                } => handlers::regenerate_user_recovery(config, id).await,
            },
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
            SiteCommand::Transport { site, action } => {
                handlers::site_transport(config, site, action).await
            }
            SiteCommand::Delete { id } => handlers::delete_site(config, id).await,
            SiteCommand::Enable { id } => handlers::enable_site(config, id).await,
            SiteCommand::Disable { id } => handlers::disable_site(config, id).await,
            SiteCommand::Staging { action } => match action {
                StagingCommand::Create { id, subdomain } => {
                    handlers::staging_create(config, id, subdomain).await
                }
                StagingCommand::Sync { id } => handlers::staging_sync(config, id).await,
                StagingCommand::Promote {
                    id,
                    snapshot,
                    confirmed_at,
                } => handlers::staging_promote(config, id, snapshot, confirmed_at).await,
                StagingCommand::Delete { id } => handlers::staging_delete(config, id).await,
            },
            SiteCommand::Collaborator { action } => match action {
                CollaboratorCommand::Invite {
                    site,
                    email,
                    scopes,
                } => handlers::collab_invite(config, site, email, scopes).await,
                CollaboratorCommand::List { site } => handlers::collab_list(config, site).await,
                CollaboratorCommand::Revoke { site, collaborator } => {
                    handlers::collab_revoke(config, site, collaborator).await
                }
            },
            SiteCommand::Cache { action } => match action {
                SiteCacheCommand::Show { site } => handlers::site_cache_show(config, site).await,
                SiteCacheCommand::Set {
                    site,
                    ttl,
                    static_ttl,
                    bypass,
                    cookies,
                    swr,
                } => {
                    handlers::site_cache_set(config, site, ttl, static_ttl, bypass, cookies, swr)
                        .await
                }
                SiteCacheCommand::Purge { site, paths } => {
                    handlers::site_cache_purge(config, site, paths).await
                }
            },
            SiteCommand::Clone { action } => match action {
                SiteCloneCommand::Run {
                    site,
                    target_domain,
                    target_owner,
                    keep_pii,
                } => {
                    handlers::site_clone_run(config, site, target_domain, target_owner, keep_pii)
                        .await
                }
            },
            SiteCommand::Template { action } => match action {
                SiteTemplateCommand::Export { site, name } => {
                    handlers::site_template_export(config, site, name).await
                }
                SiteTemplateCommand::List => handlers::site_template_list(config).await,
            },
            SiteCommand::Preview { action } => match action {
                SitePreviewCommand::List { site } => {
                    handlers::site_preview_list(config, site).await
                }
                SitePreviewCommand::Redeploy { site, pr } => {
                    handlers::site_preview_redeploy(config, site, pr).await
                }
                SitePreviewCommand::Destroy { site, pr } => {
                    handlers::site_preview_destroy(config, site, pr).await
                }
            },
            SiteCommand::Branding { action } => match action {
                BrandingCommand::Show => handlers::branding_show(config).await,
                BrandingCommand::Set {
                    brand_name,
                    color_fg,
                    color_bg,
                    color_accent,
                    contrast_min,
                    font_family,
                    base_size_px,
                    panel_domain,
                } => {
                    handlers::branding_set(
                        config,
                        brand_name,
                        color_fg,
                        color_bg,
                        color_accent,
                        contrast_min,
                        font_family,
                        base_size_px,
                        panel_domain,
                    )
                    .await
                }
                BrandingCommand::Clear => handlers::branding_clear(config).await,
            },
            SiteCommand::Webapp { action } => match action {
                WebappCommand::Preview { site, app, path } => {
                    handlers::webapp_preview(config, site, app, path).await
                }
                WebappCommand::List { site } => handlers::webapp_list(config, site).await,
            },
            SiteCommand::Scan { action } => match action {
                ScanCommand::Start { site } => handlers::scan_start(config, site).await,
                ScanCommand::List { site } => handlers::scan_list(config, site).await,
            },
        },
        Command::Cdn { action } => match action {
            CdnCommand::Integrations => handlers::cdn_integrations_list(config).await,
            CdnCommand::Purge { integration, paths } => {
                handlers::cdn_purge(config, integration, paths).await
            }
        },
        Command::SiteHttp { action } => match action {
            SiteHttpCommand::Show { site } => handlers::site_http_show(config, site).await,
            SiteHttpCommand::Set {
                site,
                controls_json,
            } => handlers::site_http_set(config, site, controls_json).await,
        },
        Command::Auth { action } => match action {
            AuthCommand::Sessions => handlers::auth_sessions(config).await,
            AuthCommand::Revoke { session } => handlers::auth_revoke(config, session).await,
        },
        Command::Terminal { action } => match action {
            TerminalCommand::Ticket { site } => handlers::terminal_ticket(config, site).await,
        },
        Command::Waf { action } => match action {
            WafCommand::Rules { site } => handlers::waf_rules(config, site).await,
            WafCommand::Hits { site } => handlers::waf_hits(config, site).await,
            WafCommand::Add { site, rule_json } => handlers::waf_add(config, site, rule_json).await,
            WafCommand::Remove { site, rule_id } => {
                handlers::waf_remove(config, site, rule_id).await
            }
            WafCommand::Enable { site, rule_id } => {
                handlers::waf_enabled(config, site, rule_id, true).await
            }
            WafCommand::Disable { site, rule_id } => {
                handlers::waf_enabled(config, site, rule_id, false).await
            }
            WafCommand::Test {
                site,
                rule_json,
                request_json,
            } => handlers::waf_test(config, site, rule_json, request_json).await,
        },
        Command::Docker { action } => match action {
            DockerCommand::Pull { image } => handlers::docker_pull(config, image).await,
            DockerCommand::List => handlers::docker_list(config).await,
            DockerCommand::Inspect { id } => handlers::docker_inspect(config, id).await,
            DockerCommand::Create { spec_json } => handlers::docker_create(config, spec_json).await,
            DockerCommand::Start { id } => handlers::docker_action(config, id, "start").await,
            DockerCommand::Stop { id } => handlers::docker_action(config, id, "stop").await,
            DockerCommand::Restart { id } => handlers::docker_action(config, id, "restart").await,
            DockerCommand::Logs { id, tail } => handlers::docker_logs(config, id, tail).await,
            DockerCommand::Exec { id, command } => handlers::docker_exec(config, id, command).await,
            DockerCommand::Rm { id, force } => handlers::docker_remove(config, id, force).await,
            DockerCommand::Allow {
                pattern,
                pin_digest_required,
            } => handlers::docker_allow(config, pattern, pin_digest_required).await,
        },
        Command::Ftp { action } => match action {
            FtpCommand::Create {
                site,
                username,
                password,
                read_only,
            } => handlers::ftp_create(config, site, username, password, read_only).await,
            FtpCommand::List { site } => handlers::ftp_list(config, site).await,
            FtpCommand::Disable { site, id } => {
                handlers::ftp_enabled(config, site, id, false).await
            }
            FtpCommand::Enable { site, id } => handlers::ftp_enabled(config, site, id, true).await,
            FtpCommand::Delete { site, id } => handlers::ftp_delete(config, site, id).await,
        },
        Command::Token { action } => match action {
            TokenCommand::Create {
                label,
                scopes,
                cidr_allowlist,
                expires_in_days,
            } => {
                handlers::token_create(config, label, scopes, cidr_allowlist, expires_in_days).await
            }
            TokenCommand::List => handlers::token_list(config).await,
            TokenCommand::Revoke { id } => handlers::token_revoke(config, id).await,
            TokenCommand::Rotate { id } => handlers::token_rotate(config, id).await,
        },
        Command::Notifications { action } => match action {
            NotificationCommand::Channel { action } => match action {
                NotificationChannelCommand::Add {
                    kind,
                    name,
                    endpoint,
                    port,
                    username,
                    credential,
                    from_addr,
                    allowlist,
                } => {
                    handlers::notification_channel_add(
                        config, kind, name, endpoint, port, username, credential, from_addr,
                        allowlist,
                    )
                    .await
                }
                NotificationChannelCommand::List => {
                    handlers::notification_channel_list(config).await
                }
                NotificationChannelCommand::Test { id, destination } => {
                    handlers::notification_channel_test(config, id, destination).await
                }
                NotificationChannelCommand::Rm { id } => {
                    handlers::notification_channel_rm(config, id).await
                }
            },
            NotificationCommand::Subscription { action } => match action {
                NotificationSubscriptionCommand::Add {
                    channel,
                    destination,
                    kind,
                    filter_json,
                } => {
                    handlers::notification_subscription_add(
                        config,
                        channel,
                        destination,
                        kind,
                        filter_json,
                    )
                    .await
                }
                NotificationSubscriptionCommand::List => {
                    handlers::notification_subscription_list(config).await
                }
                NotificationSubscriptionCommand::Rm { id } => {
                    handlers::notification_subscription_rm(config, id).await
                }
            },
            NotificationCommand::Health => handlers::notification_health(config).await,
        },
        Command::Database { action } => match action {
            DatabaseCommand::RemoteAccess { action } => {
                handlers::db_remote_access(config, action).await
            }
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
            DatabaseCommand::Pitr { action } => match action {
                PitrCommand::Status { id } => handlers::pitr_status(config, id).await,
                PitrCommand::Inspect { id } => handlers::pitr_inspect(config, id).await,
                PitrCommand::Restore {
                    id,
                    timestamp,
                    confirm,
                } => handlers::pitr_restore(config, id, timestamp, confirm).await,
            },
            DatabaseCommand::Staging { .. } => Err(anyhow::anyhow!(
                "use `openpanel site staging ...`; staging is per-site, not per-database"
            )),
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
            BackupCommand::Drill { action } => match action {
                BackupDrillCommand::Run { id } => handlers::backup_drill_run(config, id).await,
                BackupDrillCommand::List { id } => handlers::backup_drill_list(config, id).await,
                BackupDrillCommand::Show { id } => handlers::backup_drill_show(config, id).await,
            },
        },
        Command::SshKeys { action } => handlers::ssh_keys(config, action).await,
        Command::RuntimeEnv { action } => handlers::runtime_env(config, action).await,
        Command::ServerSnapshot { action } => match action {
            ServerSnapshotCommand::List => handlers::server_snapshot_list(config).await,
            ServerSnapshotCommand::Get { id } => handlers::server_snapshot_get(config, id).await,
            ServerSnapshotCommand::Create { retain } => {
                handlers::server_snapshot_create(config, retain).await
            }
            ServerSnapshotCommand::Schedule {
                name,
                schedule,
                timezone,
                retain,
            } => handlers::server_snapshot_schedule(config, name, schedule, timezone, retain).await,
            ServerSnapshotCommand::Preflight { id } => {
                handlers::server_snapshot_preflight(config, id).await
            }
            ServerSnapshotCommand::Restore { id, confirm } => {
                handlers::server_snapshot_restore(config, id, confirm).await
            }
        },
        Command::Logs { action } => match action {
            LogsCommand::Sources => handlers::logs_sources(config).await,
            LogsCommand::Policy { action } => handlers::logs_policy(config, action).await,
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
            MailCommand::Queue => handlers::mail_queue(config).await,
            MailCommand::Filter { mailbox, script } => {
                handlers::mail_filter(config, mailbox, script).await
            }
            MailCommand::Autoresponder { mailbox, body, off } => {
                handlers::mail_autoresponder(config, mailbox, body, off).await
            }
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
            SoftwareCommand::Diagnostics => handlers::software_diagnostics(config).await,
            SoftwareCommand::Refresh => handlers::software_refresh(config).await,
            SoftwareCommand::Search {
                query,
                category,
                tag,
                installed_only,
                update_available_only,
                page,
                page_size,
            } => {
                handlers::software_search(
                    config,
                    query,
                    category,
                    tag,
                    installed_only,
                    update_available_only,
                    page,
                    page_size,
                )
                .await
            }
            SoftwareCommand::Show { id } => handlers::software_show(config, id).await,
            SoftwareCommand::Preview { id } => handlers::software_preview(config, id).await,
            SoftwareCommand::Execute {
                digest,
                confirmation_token,
            } => handlers::software_execute(config, digest, confirmation_token).await,
            SoftwareCommand::Install { id, version: _ } => {
                handlers::software_install(config, id).await
            }
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
            SoftwareCommand::Jobs => handlers::software_jobs(config).await,
        },
        Command::Plugin { action } => match action {
            PluginCommand::Marketplace { action } => match action {
                MarketplaceCommand::Discover => handlers::marketplace_discover(config).await,
                MarketplaceCommand::Cached => handlers::marketplace_cached(config).await,
                MarketplaceCommand::Show { id } => handlers::marketplace_show(config, id).await,
            },
            PluginCommand::List => handlers::plugin_list(config).await,
            PluginCommand::Install { manifest } => handlers::plugin_install(config, manifest).await,
            PluginCommand::Enable { id } => handlers::plugin_enable(config, id).await,
            PluginCommand::Disable { id } => handlers::plugin_disable(config, id).await,
            PluginCommand::Uninstall { id } => handlers::plugin_uninstall(config, id).await,
        },
        Command::Registry { action } => match action {
            RegistryCommand::Config => handlers::registry_config(config).await,
            RegistryCommand::Namespaces => handlers::registry_namespaces(config).await,
            RegistryCommand::CreateNamespace {
                namespace,
                owner,
                quota_bytes,
            } => handlers::registry_create_namespace(config, namespace, owner, quota_bytes).await,
            RegistryCommand::Images { namespace } => {
                handlers::registry_images(config, namespace).await
            }
        },
        Command::Iac { action } => match action {
            IacCommand::Generate { openapi } => handlers::iac_generate(config, openapi).await,
            IacCommand::DriftCheck {
                openapi,
                committed_rust,
                committed_provider,
            } => {
                handlers::iac_drift_check(config, openapi, committed_rust, committed_provider).await
            }
        },
        Command::ContainerRuntime { action } => match action {
            ContainerRuntimeCommand::QuotaShow => {
                handlers::container_runtime_quota_show(config).await
            }
            ContainerRuntimeCommand::QuotaSet {
                max_concurrent,
                max_total,
                cpu,
                memory_bytes,
                egress_bytes,
            } => {
                handlers::container_runtime_quota_set(
                    config,
                    max_concurrent,
                    max_total,
                    cpu,
                    memory_bytes,
                    egress_bytes,
                )
                .await
            }
            ContainerRuntimeCommand::Metrics {
                container_id,
                limit,
            } => handlers::container_runtime_metrics(config, container_id, limit).await,
            ContainerRuntimeCommand::Pull {
                container_id,
                image,
                credential_id,
            } => handlers::container_runtime_pull(config, container_id, image, credential_id).await,
            ContainerRuntimeCommand::RaiseEgressLimit { bytes_per_month } => {
                handlers::container_runtime_raise_egress_limit(config, bytes_per_month).await
            }
            ContainerRuntimeCommand::RegistryCredentialAdd {
                registry,
                user,
                password,
            } => {
                handlers::container_runtime_registry_credential_add(
                    config, registry, user, password,
                )
                .await
            }
            ContainerRuntimeCommand::RegistryCredentialList => {
                handlers::container_runtime_registry_credential_list(config).await
            }
            ContainerRuntimeCommand::RegistryCredentialRemove { id } => {
                handlers::container_runtime_registry_credential_remove(config, id).await
            }
        },
    }
}
