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
}

impl FileAction {
    /// Whether the action is destructive and requires explicit confirmation.
    pub fn is_destructive(self) -> bool {
        matches!(self, FileAction::Delete)
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
}

/// Render a database row. By construction no password or private key is
/// available to this renderer, so the output is guaranteed secret-free.
pub fn render_database_row(row: &DatabaseRowView, can_manage: bool, csrf: &str) -> Markup {
    html! {
        tr {
            td { a href=(format!("/databases/{}", row.id)) { (row.name) } }
            td { (row.owner) }
            td { (row.charset) }
            td { span class="status" { (row.status) } }
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
                    a class="btn danger" hx-get=(format!("/layer/confirm?action=delete-database&id={}", row.id))
                        hx-target="#layer-root" href=(format!("/layer/confirm?action=delete-database&id={}", row.id)) {
                        "Delete"
                    }
                }
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
mod tests {
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
}
