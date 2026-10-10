//! Operational workflow building blocks shared by the files, databases, and
//! backups surfaces: recoverable file-action validation, capacity-aware backup
//! decisions, secret-safe database rows, and reusable task-state banners.
//!
//! These are pure, unit-tested models. The wizard-style HTTP endpoints that
//! consume them are added incrementally in later steps; the decision logic
//! here is the source of truth so UI and API stay consistent.

use maud::{Markup, html};
use uuid::Uuid;

use crate::layout::csrf_field;
use crate::ui_states::{ErrorState, LoadingState};
/// Lifecycle state of a long-running task (import/export/backup/restore).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    /// Queued, awaiting worker pickup.
    Queued,
    /// Actively running with a 0–100 progress percentage.
    Running(u8),
    /// Completed successfully.
    Success,
    /// Failed with a safe (secret-free) message.
    Failed(&'static str),
}

impl TaskState {
    /// Render a consistent, accessible task-state banner.
    pub fn banner(self) -> Markup {
        match self {
            TaskState::Queued => LoadingState::new("Queued…").render(),
            TaskState::Running(pct) => LoadingState::new(&format!("Running… {pct}%")).render(),
            TaskState::Success => html! {
                p class="banner banner--ok" role="status" { "Done" }
            },
            TaskState::Failed(msg) => ErrorState::new("Operation failed", msg, "").render(),
        }
    }
}

/// A selected set of file entries for a bulk action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSelection {
    /// Number of selected entries.
    pub count: usize,
    /// Whether any selected entry is a directory.
    pub any_directory: bool,
}

/// A file operation that can be applied to a selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    /// Move to recycle bin (recoverable).
    Delete,
    /// Rename in place.
    Rename,
    /// Change POSIX mode.
    Chmod,
    /// Move/copy to another path.
    Move,
    /// Archive (tar) for download.
    Archive,
    /// Clipboard copy (non-destructive).
    Copy,
    /// Clipboard cut (destructive: source removed on paste).
    Cut,
    /// Clipboard paste (destructive: may overwrite at destination).
    Paste,
    /// Compress a selection into an archive.
    Compress,
    /// Extract an archive inside the chroot.
    Extract,
}

impl FileAction {
    /// Whether the action is destructive and requires explicit confirmation.
    pub fn is_destructive(self) -> bool {
        matches!(
            self,
            FileAction::Delete | FileAction::Cut | FileAction::Paste
        )
    }

    /// Whether the action is recoverable where the backend supports a recycle
    /// bin (used to set user expectations before they confirm).
    pub fn is_recoverable(self) -> bool {
        matches!(self, FileAction::Delete)
    }

    /// Human label for the action.
    pub fn label(self) -> &'static str {
        match self {
            FileAction::Delete => "Delete",
            FileAction::Rename => "Rename",
            FileAction::Chmod => "Change permissions",
            FileAction::Move => "Move",
            FileAction::Archive => "Archive",
            FileAction::Copy => "Copy",
            FileAction::Cut => "Cut",
            FileAction::Paste => "Paste",
            FileAction::Compress => "Compress",
            FileAction::Extract => "Extract",
        }
    }
}

/// Preview of an action about to be applied to a selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPreview {
    /// The action being previewed.
    pub action: FileAction,
    /// Number of entries the action will touch.
    pub scope_count: usize,
    /// Whether the action is recoverable (recycle bin).
    pub recoverable: bool,
    /// Whether the caller must confirm before execution.
    pub requires_confirmation: bool,
}

/// Why an action was rejected before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRejection {
    /// Safe, user-facing reason.
    pub reason: &'static str,
}

/// Validate a bulk file action against the selection and confirmation state.
///
/// An empty selection is rejected; destructive actions without confirmation
/// are rejected so the UI can show the confirmation layer rather than a
/// silent, irreversible mutation.
pub fn validate_file_action(
    selection: FileSelection,
    action: FileAction,
    confirmed: bool,
) -> Result<ActionPreview, ActionRejection> {
    if selection.count == 0 {
        return Err(ActionRejection {
            reason: "No files selected",
        });
    }
    if action.is_destructive() && !confirmed {
        return Err(ActionRejection {
            reason: "Confirmation required for destructive action",
        });
    }
    Ok(ActionPreview {
        action,
        scope_count: selection.count,
        recoverable: action.is_recoverable(),
        requires_confirmation: action.is_destructive(),
    })
}

/// Validate a clipboard paste/copy/cut against the clipboard size.
///
/// An empty clipboard is rejected with the same safe reason as an empty
/// selection; destructive clipboard actions (cut/paste) without
/// confirmation are rejected so the UI shows the confirmation layer.
pub fn clipboard_preview(
    clipboard_count: usize,
    action: FileAction,
    confirmed: bool,
) -> Result<ActionPreview, ActionRejection> {
    if clipboard_count == 0 {
        return Err(ActionRejection {
            reason: "No files selected",
        });
    }
    validate_file_action(
        FileSelection {
            count: clipboard_count,
            any_directory: false,
        },
        action,
        confirmed,
    )
}

/// Validate a compress/extract archive action: the capacity verdict gates
/// execution before any mutation, then the selection preview applies.
///
/// Over-capacity requests are rejected with the actionable capacity
/// reason instead of failing mid-flight.
pub fn archive_preview(
    selection: FileSelection,
    action: FileAction,
    estimated_bytes: u64,
    free_bytes: u64,
    confirmed: bool,
) -> Result<ActionPreview, ActionRejection> {
    if let CapacityVerdict::Blocked { reason, .. } =
        evaluate_backup_capacity(estimated_bytes, free_bytes)
    {
        return Err(ActionRejection { reason });
    }
    validate_file_action(selection, action, confirmed)
}

/// Validate a recycle-bin delete: destructive and recoverable, so
/// confirmation is required and the preview carries recoverability.
pub fn recycle_preview(count: usize, confirmed: bool) -> Result<ActionPreview, ActionRejection> {
    validate_file_action(
        FileSelection {
            count,
            any_directory: false,
        },
        FileAction::Delete,
        confirmed,
    )
}

/// Validate a history-version restore. Restores reuse the rename
/// preview shape (scoped, non-recoverable) so the confirmation and
/// audit expectations stay consistent.
pub fn history_restore_preview(
    count: usize,
    confirmed: bool,
) -> Result<ActionPreview, ActionRejection> {
    validate_file_action(
        FileSelection {
            count,
            any_directory: false,
        },
        FileAction::Rename,
        confirmed,
    )
}

/// Outcome of a backup capacity check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapacityVerdict {
    /// Enough free space for the estimated backup.
    Allowed {
        /// Free bytes on the destination.
        free_bytes: u64,
        /// Estimated backup size in bytes.
        estimated_bytes: u64,
    },
    /// Not enough space; execution must be blocked with guidance.
    Blocked {
        /// Actionable, secret-free message.
        reason: &'static str,
        /// Estimated backup size in bytes.
        estimated_bytes: u64,
        /// Free bytes on the destination.
        free_bytes: u64,
    },
}

/// Decide whether a backup may proceed given its estimated size and the free
/// space on the destination. A backup that would exceed available capacity is
/// blocked with an actionable message instead of failing mid-flight.
pub fn evaluate_backup_capacity(estimated_bytes: u64, free_bytes: u64) -> CapacityVerdict {
    if estimated_bytes > free_bytes {
        CapacityVerdict::Blocked {
            reason: "Estimated backup exceeds available capacity",
            estimated_bytes,
            free_bytes,
        }
    } else {
        CapacityVerdict::Allowed {
            estimated_bytes,
            free_bytes,
        }
    }
}

/// Render a capacity verdict as an actionable preview/warning block.
pub fn capacity_banner(verdict: &CapacityVerdict) -> Markup {
    match verdict {
        CapacityVerdict::Allowed {
            free_bytes,
            estimated_bytes,
        } => html! {
            p class="banner banner--ok" role="status" {
                "Estimated " (format_bytes(*estimated_bytes))
                " fits in " (format_bytes(*free_bytes)) " free."
            }
        },
        CapacityVerdict::Blocked {
            reason,
            estimated_bytes,
            free_bytes,
        } => ErrorState::new(
            "Not enough space",
            &format!(
                "{reason}: need {}, have {}",
                format_bytes(*estimated_bytes),
                format_bytes(*free_bytes)
            ),
            "",
        )
        .render(),
    }
}

/// A secret-safe view of a database row. Passwords and private credentials are
/// never part of this struct, so rendering it cannot leak secrets.
///
/// `quota` is the operator-configured size limit (e.g. `"1024 MB"`);
/// `backup_count` is the number of retained backups. Both are read
/// projections: `None` renders as an em dash until a quota/backend
/// projection ships, and the quota-set form echoes per-request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseRowView<'a> {
    /// Database id (for detail links).
    pub id: Uuid,
    /// Database name.
    pub name: &'a str,
    /// Owning account.
    pub owner: &'a str,
    /// Character set.
    pub charset: &'a str,
    /// Status string.
    pub status: &'a str,
    /// Configured quota (`None` = unset).
    pub quota: Option<&'a str>,
    /// Retained backup count (`None` = unknown).
    pub backup_count: Option<u64>,
}

/// Render a database row. By construction no password or private key is
/// available to this renderer, so the output is guaranteed secret-free.
///
/// Row actions: password rotate/reveal (one-time panel only), plus
/// Baota-parity links for import, permission (access), Tools, and the
/// phpMyAdmin deep-link. Destructive actions route through the
/// layer-confirm gate.
pub fn render_database_row(row: &DatabaseRowView, can_manage: bool, csrf: &str) -> Markup {
    html! {
        tr {
            td { a href=(format!("/databases/{}", row.id)) { (row.name) } }
            td { (row.owner) }
            td { (row.charset) }
            td { span class="status" { (row.status) } }
            td { (row.quota.unwrap_or("\u{2014}")) }
            td {
                @match row.backup_count {
                    Some(count) => (count),
                    None => "\u{2014}",
                }
            }
            @if can_manage {
                td class="actions" {
                    form class="inline" hx-post=(format!("/databases/{}/password", row.id)) hx-target="#password-panel" hx-swap="outerHTML" {
                        (csrf_field(csrf))
                        button type="submit" { "Rotate password" }
                    }
                    a class="btn danger" hx-get=(format!("/layer/confirm?action=reveal-database-password&id={}", row.id))
                        hx-target="#layer-root" href=(format!("/layer/confirm?action=reveal-database-password&id={}", row.id)) {
                        "Reveal"
                    }
                    a class="btn btn-ghost" href=(format!("/databases/{}/import", row.id)) { "Import" }
                    a class="btn btn-ghost" href=(format!("/databases/{}/permission", row.id)) { "Access" }
                    a class="btn btn-ghost" href=(format!("/databases/{}/tools", row.id)) { "Tools" }
                    a class="btn btn-ghost" href=(format!("/databases/{}/admin", row.id)) { "phpMyAdmin" }
                    a class="btn danger" hx-get=(format!("/layer/confirm?action=delete-database&id={}", row.id))
                        hx-target="#layer-root" href=(format!("/layer/confirm?action=delete-database&id={}", row.id)) {
                        "Delete"
                    }
                }
            }
        }
    }
}

/// Permission mode for the database permission editor. Maps 1:1 onto
/// the existing RemoteAccess semantics: `Local` disables remote
/// access, `Everyone` requests the `0.0.0.0/0` wildcard (refused
/// without the owner opt-in), `SpecifiedIp` applies an explicit CIDR
/// allowlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbPermissionMode {
    /// Localhost only (remote access disabled).
    Local,
    /// The `0.0.0.0/0` wildcard (requires the owner opt-in).
    Everyone,
    /// An explicit CIDR allowlist.
    SpecifiedIp,
}

impl DbPermissionMode {
    /// Parse the editor's mode value.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "everyone" => Some(Self::Everyone),
            "specified" => Some(Self::SpecifiedIp),
            _ => None,
        }
    }

    /// Stable editor value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Everyone => "everyone",
            Self::SpecifiedIp => "specified",
        }
    }
}

/// Validated permission decision, mirroring the stored RemoteAccess shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionDecision {
    /// Database the decision applies to.
    pub database_id: Uuid,
    /// Whether remote access is enabled.
    pub enabled: bool,
    /// Allowed source CIDRs (empty when disabled).
    pub allow_cidrs: Vec<String>,
    /// Explicit wildcard opt-in.
    pub wildcard_opt_in: bool,
}

/// Validate a permission edit through the existing RemoteAccess
/// enforcement (`RemoteAccess::from_request`). Open-by-default and
/// wildcard-without-opt-in edits are refused with the
/// `global_access_locked` gate text, matching the REST surface.
pub fn decide_permission(
    database_id: Uuid,
    mode: DbPermissionMode,
    cidrs: Vec<String>,
    wildcard_opt_in: bool,
) -> Result<PermissionDecision, String> {
    let (enabled, allow_cidrs) = match mode {
        DbPermissionMode::Local => (false, Vec::new()),
        DbPermissionMode::Everyone => (true, vec!["0.0.0.0/0".to_string()]),
        DbPermissionMode::SpecifiedIp => (true, cidrs),
    };
    openpanel_domain::RemoteAccess::from_request(
        database_id,
        enabled,
        allow_cidrs.clone(),
        wildcard_opt_in,
    )
    .map(|_| PermissionDecision {
        database_id,
        enabled,
        allow_cidrs,
        wildcard_opt_in,
    })
    .map_err(|error| match error {
        openpanel_domain::DbPrivilegeError::WildcardAcl => {
            "global_access_locked: Everyone requires the explicit allow_any_source owner opt-in"
                .to_string()
        }
        openpanel_domain::DbPrivilegeError::EmptyAcl => {
            "global_access_locked: enabling remote access requires at least one CIDR".to_string()
        }
        other => format!("invalid permission: {other:?}"),
    })
}

/// Validate an import filename. Only `.sql` / `.zip` payloads are
/// accepted; anything else is rejected before anything is imported.
pub fn validate_import_filename(name: &str) -> Result<&str, String> {
    let lower = name.to_ascii_lowercase();
    if !name.is_empty() && (lower.ends_with(".sql") || lower.ends_with(".zip")) {
        Ok(name)
    } else {
        Err("import rejected: only .sql and .zip files are accepted".to_string())
    }
}

/// Validate a Tools action. Only repair / optimize / convert exist.
pub fn validate_tool_action(action: &str) -> Result<&str, String> {
    match action {
        "repair" | "optimize" | "convert" => Ok(action),
        _ => Err(format!("unknown tool action: {action}")),
    }
}

/// Server-side databases missing from the panel (the adopt set).
pub fn adopt_difference<'a>(panel: &[&'a str], server: &[&'a str]) -> Vec<&'a str> {
    server
        .iter()
        .copied()
        .filter(|name| !panel.contains(name))
        .collect()
}

/// phpMyAdmin deep-link for a database. The base path is an
/// operator-configurable convention; launching Authenticates through
/// the existing admin-tool single-use SSO mint (replay returns 401),
/// so the link itself carries no credential material.
pub fn phpmyadmin_url(db_name: &str) -> String {
    format!("/phpmyadmin?db={}", crate::layer::urlencode(db_name))
}

/// Render the permission editor: Local / Everyone / Specified IP
/// radios, a CIDR field, and the `allow_any_source` owner opt-in.
pub fn permission_view(db_id: &Uuid, csrf: &str) -> Markup {
    html! {
        section id="db-permission" {
            h2 { "Remote access" }
            form class="form" method="post" action=(format!("/databases/{db_id}/permission")) novalidate {
                (csrf_field(csrf))
                label { "Local (localhost only)"
                    input type="radio" name="mode" value="local" checked;
                }
                label { "Everyone (0.0.0.0/0)"
                    input type="radio" name="mode" value="everyone";
                }
                label { "Specified IP (CIDR allowlist)"
                    input type="radio" name="mode" value="specified";
                }
                label { "Allowed CIDRs (comma separated)"
                    input type="text" name="cidrs" placeholder="203.0.113.0/24";
                }
                label class="checkbox" {
                    input type="checkbox" name="allow_any_source" value="true";
                    " Allow access from anywhere (owner opt-in)"
                }
                button type="submit" { "Save access" }
            }
        }
    }
}

/// Render the `.sql` / `.zip` import upload form, with an optional
/// inline rejection.
pub fn import_view(db_id: &Uuid, csrf: &str, error: Option<&str>) -> Markup {
    html! {
        section id="db-import" {
            h2 { "Import" }
            @if let Some(message) = error {
                p class="form-error" role="alert" { (message) }
            }
            form class="form" method="post" action=(format!("/databases/{db_id}/import")) novalidate {
                (csrf_field(csrf))
                label { "Dump file (.sql or .zip)"
                    input type="text" name="filename" placeholder="backup.sql";
                }
                button type="submit" { "Import dump" }
            }
        }
    }
}

/// Render the Tools panel: repair / optimize / convert, each routed
/// through the layer-confirm gate before dispatch.
pub fn tools_view(db_id: &Uuid, csrf: &str) -> Markup {
    let _ = csrf;
    html! {
        section id="db-tools" {
            h2 { "Tools" }
            ul {
                @for action in ["repair", "optimize", "convert"] {
                    li {
                        a class="btn btn-ghost"
                            hx-get=(format!("/layer/confirm?action=run-database-tool&id={db_id}&tool={action}"))
                            hx-target="#layer-root"
                            href=(format!("/layer/confirm?action=run-database-tool&id={db_id}&tool={action}")) {
                            (action)
                        }
                    }
                }
            }
        }
    }
}

/// Render the Sync-all / Get-from-server adopt flow: server-side
/// databases missing from the panel, each with an adopt action that
/// registers panel metadata only (never imports credentials).
pub fn adopt_view(panel: &[&str], server: &[&str], csrf: &str) -> Markup {
    let missing = adopt_difference(panel, server);
    let server_joined = server.join(",");
    html! {
        section id="db-adopt" {
            h2 { "Sync from server" }
            p {
                (panel.len()) " of " (server.len())
                " server databases already in the panel."
            }
            @if missing.is_empty() {
                (crate::ui_states::EmptyState::new(
                    "Everything is in sync",
                    "Every server database is already registered in the panel."
                ).render())
            } @else {
                table class="table" {
                    thead { tr { th { "Database" } th { "Action" } } }
                    tbody {
                        @for name in &missing {
                            tr {
                                td { strong { (name) } }
                                td class="actions" {
                                    form class="form form-row" method="post" action="/databases/adopt" novalidate {
                                        (csrf_field(csrf))
                                        input type="hidden" name="server" value=(server_joined);
                                        input type="hidden" name="name" value=(name);
                                        button type="submit" { "Adopt" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render the phpMyAdmin launch panel: the SSO deep-link plus the
/// single-use contract (the admin-tool mint issues one token, replay
/// returns 401, every launch audits `AdminToolLaunched`).
pub fn phpmyadmin_panel(db_id: &Uuid, db_name: &str, csrf: &str) -> Markup {
    let _ = (db_id, csrf);
    let link = phpmyadmin_url(db_name);
    html! {
        section id="db-admin" {
            h2 { "phpMyAdmin" }
            p {
                a class="btn" href=(link) { "Open phpMyAdmin" }
            }
            p class="muted" {
                "Sign-in is single-use: the admin-tool mint issues one token per launch."
            }
        }
    }
}

/// Human-readable byte size, e.g. `1.5 MB`.
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests;
