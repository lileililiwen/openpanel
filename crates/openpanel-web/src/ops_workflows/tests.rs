//! Tests for the operations-workflow decision models and Baota-parity row views (extracted from `mod.rs` to respect the 1000-line file-length ceiling).

use super::*;

use crate::ui_states::EmptyState;

#[test]
fn empty_selection_is_rejected() {
    let r = validate_file_action(
        FileSelection {
            count: 0,
            any_directory: false,
        },
        FileAction::Delete,
        true,
    );
    assert_eq!(
        r,
        Err(ActionRejection {
            reason: "No files selected"
        })
    );
}

#[test]
fn destructive_action_requires_confirmation() {
    let rejected = validate_file_action(
        FileSelection {
            count: 3,
            any_directory: false,
        },
        FileAction::Delete,
        false,
    );
    assert_eq!(
        rejected,
        Err(ActionRejection {
            reason: "Confirmation required for destructive action"
        })
    );
}

#[test]
fn confirmed_delete_preview_is_recoverable() {
    let ok = validate_file_action(
        FileSelection {
            count: 3,
            any_directory: false,
        },
        FileAction::Delete,
        true,
    )
    .expect("confirmed");
    assert_eq!(ok.scope_count, 3);
    assert!(ok.recoverable, "delete is recoverable");
    assert!(ok.requires_confirmation);
}

#[test]
fn non_destructive_action_needs_no_confirm() {
    let ok = validate_file_action(
        FileSelection {
            count: 1,
            any_directory: false,
        },
        FileAction::Chmod,
        false,
    )
    .expect("allowed");
    assert!(!ok.requires_confirmation);
    assert!(!ok.recoverable);
}

#[test]
fn backup_blocked_when_over_capacity() {
    let v = evaluate_backup_capacity(2_000, 1_000);
    assert!(matches!(v, CapacityVerdict::Blocked { .. }));
    let out = capacity_banner(&v).into_string();
    assert!(out.contains("Not enough space"), "blocked banner: {out}");
    assert!(out.contains("2.0 KB"), "shows estimate: {out}");
    assert!(out.contains("1000 B"), "shows free: {out}");
}

#[test]
fn backup_allowed_when_fits() {
    let v = evaluate_backup_capacity(500, 1_000);
    assert!(matches!(v, CapacityVerdict::Allowed { .. }));
    assert!(capacity_banner(&v).into_string().contains("fits"));
}

#[test]
fn database_row_never_leaks_secrets() {
    let row = DatabaseRowView {
        id: Uuid::new_v4(),
        name: "appdb",
        owner: "admin",
        charset: "utf8mb4",
        status: "active",
        quota: None,
        backup_count: None,
    };
    let out = render_database_row(&row, true, "csrf-token").into_string();
    assert!(out.contains("appdb"), "name present");
    assert!(out.contains("/databases/"), "detail link");
    // A password must never be rendered in the list row.
    assert!(
        !out.contains("secret-password-xyz"),
        "no secret leaked: {out}"
    );
}

#[test]
fn task_state_banners_render() {
    assert!(TaskState::Queued.banner().into_string().contains("Queued"));
    assert!(
        TaskState::Running(42)
            .banner()
            .into_string()
            .contains("42%")
    );
    assert!(TaskState::Success.banner().into_string().contains("Done"));
    assert!(
        TaskState::Failed("boom")
            .banner()
            .into_string()
            .contains("boom")
    );
}

#[test]
fn empty_state_reused() {
    let out = EmptyState::new("Nothing", "here").render().into_string();
    assert!(out.contains("Nothing"));
}

/// Baota-parity database row actions: quota/backup-count columns,
/// password one-time panel triggers, import, permission editor,
/// Tools, phpMyAdmin deep-link, and Sync-all/Get-from-server adopt.
///
/// All decision logic here is pure and secret-free; HTTP handlers in
/// `databases.rs` gate on session + role + CSRF and delegate
/// mutations to the existing services.
#[cfg(test)]
mod row_action_tests {
    use super::*;

    fn row() -> DatabaseRowView<'static> {
        DatabaseRowView {
            id: Uuid::new_v4(),
            name: "appdb",
            owner: "admin",
            charset: "utf8mb4",
            status: "active",
            quota: Some("1024 MB"),
            backup_count: Some(3),
        }
    }

    #[test]
    fn row_actions_render_password_as_one_time_panel_triggers() {
        let out = render_database_row(&row(), true, "csrf-token").into_string();
        assert!(
            out.contains("hx-get=\"/layer/confirm?action=reveal-database-password"),
            "reveal via layer confirm: {out}"
        );
        assert!(out.contains("Rotate password"), "rotate action: {out}");
        assert!(
            !out.contains("id=\"password-panel\""),
            "no inline panel: {out}"
        );
    }

    #[test]
    fn row_lists_quota_and_backup_count_columns() {
        let out = render_database_row(&row(), true, "csrf-token").into_string();
        assert!(out.contains("1024 MB"), "quota cell: {out}");
        assert!(out.contains('3'), "backup count cell: {out}");
        let unset = DatabaseRowView {
            quota: None,
            backup_count: None,
            ..row()
        };
        let unset_out = render_database_row(&unset, true, "csrf-token").into_string();
        assert!(
            unset_out.contains('\u{2014}'),
            "unset renders em dash: {unset_out}"
        );
    }

    #[test]
    fn row_exposes_import_permission_tools_and_phpmyadmin_links() {
        let r = row();
        let out = render_database_row(&r, true, "csrf-token").into_string();
        let id = r.id;
        for needle in [
            format!("/databases/{id}/import").as_str(),
            format!("/databases/{id}/permission").as_str(),
            format!("/databases/{id}/tools").as_str(),
            format!("/databases/{id}/admin").as_str(),
        ] {
            assert!(out.contains(needle), "missing {needle}: {out}");
        }
        assert!(out.contains("phpMyAdmin"), "phpmyadmin label: {out}");
    }

    #[test]
    fn permission_view_maps_modes_and_refuses_everyone_without_opt_in() {
        let id = Uuid::new_v4();
        let out = permission_view(&id, "csrf-token").into_string();
        assert!(out.contains("Local"), "local mode: {out}");
        assert!(out.contains("Everyone"), "everyone mode: {out}");
        assert!(out.contains("Specified"), "specified-ip mode: {out}");
        assert!(out.contains("allow_any_source"), "opt-in checkbox: {out}");
        assert!(out.contains("name=\"_csrf\""), "csrf field: {out}");

        let refused =
            decide_permission(id, DbPermissionMode::Everyone, vec![], false).expect_err("refused");
        assert!(
            refused.contains("global_access_locked"),
            "refusal names the gate: {refused}"
        );
        let allowed =
            decide_permission(id, DbPermissionMode::Everyone, vec![], true).expect("opt-in allows");
        assert!(allowed.allow_cidrs.contains(&"0.0.0.0/0".to_string()));
        let local =
            decide_permission(id, DbPermissionMode::Local, vec![], false).expect("local always ok");
        assert!(!local.enabled);
    }

    #[test]
    fn import_view_rejects_non_sql_non_zip_with_form_error() {
        let id = Uuid::new_v4();
        let err = validate_import_filename("backup.exe").expect_err("rejected");
        assert!(
            err.contains(".sql") && err.contains(".zip"),
            "guidance: {err}"
        );
        assert!(validate_import_filename("dump.sql").is_ok());
        assert!(validate_import_filename("DUMP.ZIP").is_ok());
        let out = import_view(&id, "csrf-token", Some(&err)).into_string();
        assert!(out.contains("form-error"), "inline error: {out}");
        assert!(out.contains("role=\"alert\""), "alert role: {out}");
    }

    #[test]
    fn tools_view_renders_repair_optimize_convert_with_confirm_gates() {
        let id = Uuid::new_v4();
        let out = tools_view(&id, "csrf-token").into_string();
        for action in ["repair", "optimize", "convert"] {
            assert!(out.contains(action), "missing {action}: {out}");
            assert!(
                out.contains("action=run-database-tool"),
                "layer-confirm gate: {out}"
            );
        }
        assert!(validate_tool_action("repair").is_ok());
        assert!(validate_tool_action("vacuum").is_err());
    }

    #[test]
    fn adopt_view_lists_only_server_databases_missing_from_panel() {
        let panel = ["appdb", "logs"];
        let server = ["appdb", "logs", "legacy_shop", "staging_copy"];
        let missing = adopt_difference(&panel, &server);
        assert_eq!(missing, vec!["legacy_shop", "staging_copy"]);
        let id_csrf_out = adopt_view(&panel, &server, "csrf-token").into_string();
        assert!(
            id_csrf_out.contains("legacy_shop"),
            "adoptable listed: {id_csrf_out}"
        );
        assert!(
            id_csrf_out.contains("staging_copy"),
            "adoptable listed: {id_csrf_out}"
        );
        assert!(
            !id_csrf_out.contains(">appdb<"),
            "panel db not adoptable: {id_csrf_out}"
        );
    }

    #[test]
    fn phpmyadmin_deep_link_points_at_admin_panel() {
        let id = Uuid::new_v4();
        let link = phpmyadmin_url("admin_shop");
        assert!(link.contains("admin_shop"), "db scoped: {link}");
        assert!(!link.contains(' '), "no spaces: {link}");
        let out = phpmyadmin_panel(&id, "admin_shop", "csrf-token").into_string();
        assert!(out.contains(&link), "panel carries the link: {out}");
        assert!(out.contains("single-use"), "sso contract noted: {out}");
    }

    mod prop {
        use super::*;

        #[test]
        fn prop_db_permission_never_open_by_default() {
            // Sweep over modes/cidrs/flags: without the explicit
            // owner opt-in the decision never yields a 0.0.0.0/0 ACL.
            let id = Uuid::new_v4();
            for (mode, cidrs) in [
                (DbPermissionMode::Local, vec![]),
                (DbPermissionMode::Everyone, vec![]),
                (
                    DbPermissionMode::SpecifiedIp,
                    vec!["10.0.0.0/8".to_string()],
                ),
                (DbPermissionMode::SpecifiedIp, vec!["0.0.0.0/0".to_string()]),
                (DbPermissionMode::SpecifiedIp, vec![]),
            ] {
                let decision = decide_permission(id, mode, cidrs, false);
                match decision {
                    Ok(decided) => assert!(
                        !decided.allow_cidrs.iter().any(|c| c == "0.0.0.0/0"),
                        "open ACL without opt-in: {decided:?}"
                    ),
                    Err(reason) => assert!(
                        reason.contains("global_access_locked"),
                        "refusal names the gate: {reason}"
                    ),
                }
            }
        }
    }
}

/// Baota-parity file affordances: clipboard (copy/cut/paste), compress/
/// extract, recycle/restore, history-restore, and the capacity-gated
/// archive header. All previews validate through `validate_file_action`
/// so scope, recoverability, and confirmation stay consistent.
#[cfg(test)]
mod parity_tests {
    use super::*;

    #[test]
    fn archive_preview_returns_scope_and_progress_banner() {
        let selection = FileSelection {
            count: 2,
            any_directory: false,
        };
        let preview = archive_preview(selection, FileAction::Compress, 500, 1_000, false)
            .expect("compress fits");
        assert_eq!(preview.scope_count, 2, "scope: {preview:?}");
        let running = TaskState::Running(42).banner().into_string();
        assert!(running.contains("42%"), "percent: {running}");
        assert!(
            running.contains("role=\"status\""),
            "status role: {running}"
        );
        let extract = archive_preview(selection, FileAction::Extract, 500, 1_000, false)
            .expect("extract fits");
        assert_eq!(extract.scope_count, 2, "extract scope: {extract:?}");
    }

    #[test]
    fn archive_preview_blocks_over_capacity_with_message() {
        let selection = FileSelection {
            count: 2,
            any_directory: false,
        };
        let err = archive_preview(selection, FileAction::Compress, 2_000, 1_000, false)
            .expect_err("over capacity blocked");
        assert!(
            err.reason.contains("capacity") || err.reason.contains("exceeds"),
            "capacity reason: {}",
            err.reason
        );
        let verdict = evaluate_backup_capacity(2_000, 1_000);
        let banner = capacity_banner(&verdict).into_string();
        assert!(banner.contains("Not enough space"), "blocked: {banner}");
        assert!(banner.contains("need"), "need vs have: {banner}");
    }

    #[test]
    fn clipboard_preview_empty_rejected_and_paste_needs_confirm() {
        let empty = clipboard_preview(0, FileAction::Paste, false).expect_err("empty rejected");
        assert_eq!(
            empty.reason, "No files selected",
            "reason: {}",
            empty.reason
        );
        let unconfirmed =
            clipboard_preview(2, FileAction::Paste, false).expect_err("paste needs confirm");
        assert!(
            unconfirmed.reason.contains("Confirmation"),
            "confirm reason: {}",
            unconfirmed.reason
        );
        let ok = clipboard_preview(2, FileAction::Copy, false).expect("copy allowed");
        assert_eq!(ok.scope_count, 2, "scope: {ok:?}");
        assert!(!ok.requires_confirmation, "copy needs no confirm: {ok:?}");
    }

    #[test]
    fn recycle_preview_requires_confirm_and_is_recoverable() {
        let unconfirmed = recycle_preview(1, false).expect_err("confirm required");
        assert!(
            unconfirmed.reason.contains("Confirmation"),
            "reason: {}",
            unconfirmed.reason
        );
        let ok = recycle_preview(1, true).expect("confirmed");
        assert!(ok.recoverable, "delete recoverable: {ok:?}");
        assert!(ok.requires_confirmation, "delete needs confirm: {ok:?}");
        let history = history_restore_preview(1, true).expect("history restore");
        assert_eq!(history.scope_count, 1, "scope: {history:?}");
    }
}
