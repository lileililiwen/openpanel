//! Unit and property tests for terminal tickets and sessions.

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{
    CloseReason, OsTicketRandomer, TICKET_TTL_SECS, TerminalError, TerminalSession, TerminalTicket,
    TicketRandomer,
};

struct FixedRandomer(u8);

impl TicketRandomer for FixedRandomer {
    fn fill(&self, dest: &mut [u8]) {
        for byte in dest.iter_mut() {
            *byte = self.0;
        }
    }
}

#[test]
fn test_ticket_mint_sets_expiry_and_hex_token() {
    let now = Utc::now();
    let ticket = TerminalTicket::mint(&OsTicketRandomer, Uuid::new_v4(), Uuid::new_v4(), now, 30);
    assert_eq!(ticket.token.0.len(), 64);
    assert!(
        ticket
            .token
            .0
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert_eq!((ticket.expires_at - now).num_seconds(), 30);
    assert!(!ticket.consumed);
}

#[test]
fn test_consume_is_single_use() {
    let now = Utc::now();
    let mut ticket =
        TerminalTicket::mint(&FixedRandomer(1), Uuid::new_v4(), Uuid::new_v4(), now, 30);
    let first = ticket.consume(now).cloned().expect("first consume");
    assert_eq!(first.0, "01".repeat(32));
    assert_eq!(ticket.consume(now).unwrap_err(), TerminalError::Used);
}

#[test]
fn test_expired_ticket_rejected() {
    let now = Utc::now();
    let mut ticket =
        TerminalTicket::mint(&FixedRandomer(2), Uuid::new_v4(), Uuid::new_v4(), now, 30);
    let later = now + Duration::seconds(TICKET_TTL_SECS + 1);
    assert_eq!(ticket.consume(later).unwrap_err(), TerminalError::Expired);
    // Expiry does not mark the ticket consumed; state stays inspectable.
    assert!(!ticket.consumed);
}

#[test]
fn test_tokens_are_unique_across_mints() {
    let now = Utc::now();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..1000 {
        let ticket =
            TerminalTicket::mint(&OsTicketRandomer, Uuid::new_v4(), Uuid::new_v4(), now, 30);
        assert!(seen.insert(ticket.token.0), "token collision");
    }
}

#[test]
fn test_session_close_records_duration_and_is_idempotent() {
    let opened_at = Utc::now();
    let mut session = TerminalSession::open(Uuid::new_v4(), Uuid::new_v4(), opened_at);
    session.close(CloseReason::IdleTimeout, opened_at + Duration::seconds(42));
    assert_eq!(session.duration_secs(), Some(42));
    assert_eq!(session.close_reason, Some(CloseReason::IdleTimeout));

    // Second close keeps the first outcome.
    session.close(CloseReason::Overflow, opened_at + Duration::seconds(100));
    assert_eq!(session.duration_secs(), Some(42));
}
