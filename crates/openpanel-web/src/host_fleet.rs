//! Host-fleet and terminal safety building blocks.
//!
//! These are the pure, unit-tested models behind the terminal + host-fleet
//! workspace: a host view derived from `AgentRegistration`, a command-safety
//! classifier used to gate dangerous operations behind confirmation, and a
//! session-summary model with explicit expiry. The streaming terminal endpoint
//! and the `/hosts` fleet route are added in a later step; the decision logic
//! here is the source of truth so the UI and API stay consistent.

use chrono::{DateTime, Utc};
use maud::{Markup, html};
use openpanel_domain::agent::AgentRegistration;
use uuid::Uuid;

/// A render-ready host row derived from an agent registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostView {
    /// Host id (UUID form).
    pub id: Uuid,
    /// Reported hostname.
    pub hostname: String,
    /// Human status string (`Online` / `Offline` / `Pending` / `Revoked`).
    pub status: String,
    /// Whether the host is currently connected.
    pub connected: bool,
    /// Last heartbeat time, if any.
    pub last_seen: Option<DateTime<Utc>>,
}

impl HostView {
    /// Build a view from a registered agent, redacting certificate material.
    pub fn from_registration(reg: &AgentRegistration) -> Self {
        let status = reg.status();
        Self {
            id: reg.id().as_uuid(),
            hostname: reg.hostname().to_string(),
            status: agent_status_label(status).to_string(),
            connected: matches!(status, openpanel_domain::agent::AgentStatus::Online),
            last_seen: reg.last_heartbeat_at(),
        }
    }
}

/// Map an agent status to its display label.
fn agent_status_label(status: openpanel_domain::agent::AgentStatus) -> &'static str {
    match status {
        openpanel_domain::agent::AgentStatus::Pending => "Pending",
        openpanel_domain::agent::AgentStatus::Online => "Online",
        openpanel_domain::agent::AgentStatus::Offline => "Offline",
        openpanel_domain::agent::AgentStatus::Revoked => "Revoked",
    }
}

/// Safety classification of a command the operator wants to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandSafety {
    /// Safe to run without extra confirmation.
    Safe,
    /// Can stop/restart the panel or host; must be confirmed and audited.
    Dangerous(&'static str),
}

/// Classify a shell command by whether it can stop, restart, or otherwise
/// take down the panel or host. Dangerous commands are gated behind the
/// existing confirmation and audit mechanisms rather than run silently.
pub fn classify_command(command: &str) -> CommandSafety {
    let lower = command.to_ascii_lowercase();
    // Order matters: the most specific host/panel impacts first.
    if lower.contains("systemctl")
        && (lower.contains("openpanel") || lower.contains("restart") || lower.contains("stop"))
    {
        return CommandSafety::Dangerous("Controls a system service");
    }
    if lower.contains("reboot")
        || lower.contains("shutdown")
        || lower.contains("poweroff")
        || lower.contains("halt")
    {
        return CommandSafety::Dangerous("Restarts or powers off the host");
    }
    if lower.contains("rm") && lower.contains("-rf") && lower.contains(' ') && lower.contains('/') {
        return CommandSafety::Dangerous("Recursive delete at filesystem root");
    }
    CommandSafety::Safe
}

/// A terminal session summary with explicit lifecycle and expiry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// Host the session is attached to.
    pub host_id: Uuid,
    /// Authenticated actor (user id).
    pub actor: Uuid,
    /// When the session opened.
    pub opened_at: DateTime<Utc>,
    /// When the session expires (auto-terminate).
    pub expires_at: DateTime<Utc>,
    /// Connection state label.
    pub state: &'static str,
}

impl SessionSummary {
    /// Whether the session has expired at `now` (auto-terminate).
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }
}

/// Render the host fleet list. Only non-secret metadata (hostname, status,
/// last-seen) is emitted; certificate fingerprints and key material are never
/// part of the row struct, so they cannot leak into HTML.
pub fn render_host_list(hosts: &[HostView]) -> Markup {
    html! {
        section id="host-fleet" class="host-fleet" {
            h2 { "Hosts" }
            @if hosts.is_empty() {
                (crate::ui_states::EmptyState::new(
                    "No hosts registered",
                    "Register an agent to manage this host from the fleet view."
                ).render())
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Hostname" }
                            th { "Status" }
                            th { "Last seen" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for host in hosts {
                            tr {
                                td { (host.hostname) }
                                td {
                                    span class=(format!("status status-{}", host.status.to_ascii_lowercase())) {
                                        (host.status)
                                    }
                                }
                                td {
                                    @match host.last_seen {
                                        Some(ts) => (ts.to_rfc3339()),
                                        None => "—",
                                    }
                                }
                                td class="actions" {
                                    a class="btn" href=(format!("/hosts/{}/terminal", host.id)) {
                                        "Terminal"
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

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use openpanel_domain::agent::{AgentId, AgentRegistration, AgentStatus};

    use super::*;

    fn reg(status: AgentStatus, last: Option<DateTime<Utc>>) -> AgentRegistration {
        AgentRegistration::restore(
            AgentId::new(),
            "host-fp".into(),
            "web-01".into(),
            status,
            "cert-fp-secret".into(),
            last,
            Utc::now(),
            Uuid::new_v4(),
        )
    }

    #[test]
    fn host_view_derives_status_and_redacts_keys() {
        let view = HostView::from_registration(&reg(AgentStatus::Online, Some(Utc::now())));
        assert_eq!(view.hostname, "web-01");
        assert!(view.connected);
        let out = render_host_list(&[view]).into_string();
        // Certificate material must never be rendered.
        assert!(!out.contains("cert-fp-secret"), "secret leaked: {out}");
        assert!(out.contains("web-01"));
    }

    #[test]
    fn offline_host_is_not_connected() {
        let view = HostView::from_registration(&reg(AgentStatus::Offline, None));
        assert!(!view.connected);
        assert_eq!(view.status, "Offline");
    }

    #[test]
    fn dangerous_commands_are_flagged() {
        assert_eq!(
            classify_command("systemctl restart openpanel"),
            CommandSafety::Dangerous("Controls a system service")
        );
        assert_eq!(
            classify_command("sudo reboot"),
            CommandSafety::Dangerous("Restarts or powers off the host")
        );
        assert_eq!(
            classify_command("rm -rf /var/lib/data"),
            CommandSafety::Dangerous("Recursive delete at filesystem root")
        );
    }

    #[test]
    fn safe_commands_pass() {
        assert_eq!(classify_command("ls -la /var/www"), CommandSafety::Safe);
        assert_eq!(classify_command("cat /etc/hosts"), CommandSafety::Safe);
    }

    #[test]
    fn session_expiry_is_detected() {
        let now = Utc::now();
        let s = SessionSummary {
            host_id: Uuid::new_v4(),
            actor: Uuid::new_v4(),
            opened_at: now - Duration::minutes(10),
            expires_at: now - Duration::minutes(1),
            state: "active",
        };
        assert!(s.is_expired(now), "expired session");
        let live = SessionSummary {
            host_id: Uuid::new_v4(),
            actor: Uuid::new_v4(),
            opened_at: now,
            expires_at: now + Duration::minutes(30),
            state: "active",
        };
        assert!(!live.is_expired(now), "live session");
    }
}
