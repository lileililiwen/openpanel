## ADDED Requirements

### Requirement: Non-Nagging Feedback Widget

The shell SHALL render a single feedback widget (NPS-style: thumbs-up /
thumbs-down, optional free-text) once per browser, gated on **both**
conditions:

1. The authenticated account's age in days (`account.created_at` →
   now) is `>= 3`.
2. The browser has not previously dismissed or submitted the widget —
   tracked via `localStorage.openpanel_feedback_seen !== "true"`.

The widget SHALL render in the bottom-right of the shell, SHALL be
keyboard-accessible, SHALL NOT block the rest of the UI, and SHALL be
dismissable via an explicit close button (which sets
`localStorage.openpanel_feedback_seen = "true"`).

#### Scenario: New account does not see widget

- **WHEN** the authenticated account is younger than 3 days
- **THEN** the feedback widget is not rendered in the shell

#### Scenario: Returning browser does not see widget

- **WHEN** the browser already has `localStorage.openpanel_feedback_seen = "true"`
- **THEN** the feedback widget is not rendered, regardless of account age

#### Scenario: Mature account on fresh browser sees widget

- **WHEN** the account is >= 3 days old AND `localStorage.openpanel_feedback_seen`
  is unset
- **THEN** the shell renders the widget in the bottom-right corner

### Requirement: Feedback Submission Persists

`POST /feedback` SHALL accept `{ sentiment: "up" | "down", comment?: string }`,
validate that the caller is authenticated, and persist the response to a
new `feedback` table. The handler MUST rate-limit to 5 submissions per
account per 24h and MUST emit a `layer-toast` (`kind=success`,
`msg="Thanks for the feedback"`) on success.

#### Scenario: Successful submission

- **WHEN** an authenticated user submits `POST /feedback` with `sentiment="up"`
- **THEN** the response is `200`, a row is inserted in the `feedback` table,
  the client sets `localStorage.openpanel_feedback_seen = "true"`, the
  widget is hidden, and a success toast is shown

#### Scenario: Rate-limited submission

- **WHEN** the same account has already submitted 5 times in the last 24h
- **THEN** the response is `429 Too Many Requests`, no row is inserted,
  and the widget shows an inline error message

### Requirement: Feedback Aggregate is Domain-I/O-Free

The feedback aggregate SHALL live in
`crates/openpanel-domain/src/feedback.rs`, SHALL define
`FeedbackEntry`, `Sentiment`, `FeedbackError`, and a
`FeedbackRepository` trait. The repository implementation SHALL live in
`crates/openpanel-app/src/feedback.rs` and SHALL depend only on
`openpanel-domain` + `openpanel-core`.

#### Scenario: Domain has no I/O imports

- **WHEN** `cargo build -p openpanel-domain` runs
- **THEN** it MUST NOT link to `sqlx`, `axum`, or `tokio`; the new
  feedback module MUST compile without those dependencies