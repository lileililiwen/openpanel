//! Feedback aggregate for the NPS-style admin interaction surface.
//!
//! A `FeedbackEntry` records one optional widget submission: a thumbs-up or
//! thumbs-down sentiment plus an optional free-text comment. The aggregate
//! enforces comment bounds and the repository trait exposes the rolling
//! 24-hour window used by the submission rate limit.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Upper bound on a feedback comment's length.
pub const MAX_FEEDBACK_COMMENT_LEN: usize = 2000;

/// Maximum submissions per account inside the 24-hour rate-limit window.
pub const FEEDBACK_RATE_LIMIT_PER_24H: u64 = 5;

/// Width of the rolling submission window, in seconds.
pub const FEEDBACK_RATE_WINDOW_SECS: i64 = 24 * 60 * 60;

/// An account must be at least this old (in days) before the widget is offered.
pub const FEEDBACK_MIN_ACCOUNT_AGE_DAYS: i64 = 3;

/// Whether a new submission would exceed the rolling-window budget.
///
/// A submission is accepted while the observed count is strictly below the
/// limit; the `limit`-th submission is rejected.
pub fn rate_limit_exceeded(count_in_window: u64, limit: u64) -> bool {
    count_in_window >= limit
}

/// User sentiment for one feedback submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sentiment {
    /// Thumbs-up: the user is satisfied.
    Up,
    /// Thumbs-down: the user is not satisfied.
    Down,
}

impl Sentiment {
    /// Parse a wire value (`"up"` / `"down"`).
    pub fn parse(value: &str) -> Result<Self, FeedbackError> {
        match value {
            "up" => Ok(Self::Up),
            "down" => Ok(Self::Down),
            _ => Err(FeedbackError::InvalidSentiment),
        }
    }

    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// One persisted feedback submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackEntry {
    id: Uuid,
    account_id: Uuid,
    sentiment: Sentiment,
    comment: Option<String>,
    created_at: DateTime<Utc>,
}

impl FeedbackEntry {
    /// Construct a validated submission. The comment (when present) is
    /// trimmed, bounded to [`MAX_FEEDBACK_COMMENT_LEN`], and must not
    /// contain control characters.
    pub fn new(
        id: Uuid,
        account_id: Uuid,
        sentiment: Sentiment,
        comment: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, FeedbackError> {
        let comment = match comment {
            Some(raw) if !raw.trim().is_empty() => Some(validate_comment(raw)?),
            Some(_) => None,
            None => None,
        };
        Ok(Self {
            id,
            account_id,
            sentiment,
            comment,
            created_at,
        })
    }

    /// Submission identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Owning account.
    pub fn account_id(&self) -> Uuid {
        self.account_id
    }

    /// Expressed sentiment.
    pub fn sentiment(&self) -> Sentiment {
        self.sentiment
    }

    /// Optional free-text comment.
    pub fn comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }

    /// Submission time.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

fn validate_comment(raw: String) -> Result<String, FeedbackError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(FeedbackError::InvalidComment("empty comment".into()));
    }
    if trimmed.len() > MAX_FEEDBACK_COMMENT_LEN {
        return Err(FeedbackError::InvalidComment(format!(
            "comment exceeds {MAX_FEEDBACK_COMMENT_LEN} characters"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(FeedbackError::InvalidComment(
            "comment contains control characters".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

/// Domain validation or persistence failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FeedbackError {
    /// The comment violates a bound or contains control characters.
    #[error("invalid feedback comment: {0}")]
    InvalidComment(String),
    /// The sentiment value is not `up` or `down`.
    #[error("invalid feedback sentiment")]
    InvalidSentiment,
    /// The account exceeded the per-window submission budget.
    #[error("feedback rate limit exceeded, try again later")]
    RateLimited,
    /// The requested submission does not exist.
    #[error("feedback entry not found")]
    NotFound,
    /// Persistence adapter failed.
    #[error("feedback persistence failed: {0}")]
    Persistence(String),
}

/// Persistence port for feedback submissions.
#[async_trait]
pub trait FeedbackRepository: Send + Sync {
    /// Persist one submission.
    async fn insert(&self, entry: &FeedbackEntry) -> Result<(), RepoError>;
    /// Count submissions for one account inside the rolling window.
    async fn count_in_window(
        &self,
        account_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<u64, RepoError>;
    /// Load the most recent submissions for one account.
    async fn find_recent(
        &self,
        account_id: Uuid,
        limit: u32,
    ) -> Result<Vec<FeedbackEntry>, RepoError>;
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn entry(sentiment: Sentiment, comment: Option<&str>) -> Result<FeedbackEntry, FeedbackError> {
        FeedbackEntry::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            sentiment,
            comment.map(str::to_owned),
            Utc::now(),
        )
    }

    #[test]
    fn sentiment_parses_wire_values() {
        assert_eq!(Sentiment::parse("up").expect("up"), Sentiment::Up);
        assert_eq!(Sentiment::parse("down").expect("down"), Sentiment::Down);
        assert_eq!(
            Sentiment::parse("neutral"),
            Err(FeedbackError::InvalidSentiment)
        );
        assert_eq!(Sentiment::Up.as_str(), "up");
        assert_eq!(Sentiment::Down.as_str(), "down");
        assert_eq!(
            serde_json::to_value(Sentiment::Down).expect("serialize"),
            serde_json::json!("down")
        );
    }

    #[test]
    fn entry_validates_comment_bounds() {
        let long = "x".repeat(MAX_FEEDBACK_COMMENT_LEN + 1);
        assert_eq!(
            entry(Sentiment::Up, Some(&long)),
            Err(FeedbackError::InvalidComment(format!(
                "comment exceeds {MAX_FEEDBACK_COMMENT_LEN} characters"
            )))
        );

        let control = "fine\tthen";
        assert_eq!(
            entry(Sentiment::Down, Some(control)),
            Err(FeedbackError::InvalidComment(
                "comment contains control characters".into()
            ))
        );
    }

    #[test]
    fn entry_trims_and_omits_blank_comments() {
        let blank = entry(Sentiment::Up, Some("   ")).expect("blank becomes none");
        assert_eq!(blank.comment(), None, "blank comment stored as none");

        let whitespace = entry(Sentiment::Up, Some("  nice panel  ")).expect("trimmed");
        assert_eq!(whitespace.comment(), Some("nice panel"));
    }

    #[test]
    fn entry_without_comment_is_valid() {
        let bare = entry(Sentiment::Down, None).expect("no comment ok");
        assert_eq!(bare.sentiment(), Sentiment::Down);
        assert_eq!(bare.comment(), None);
        assert!(bare.created_at() <= Utc::now());
        assert!(bare.created_at() > Utc::now() - Duration::seconds(60));
        assert_ne!(bare.id(), Uuid::nil());
        assert_ne!(bare.account_id(), Uuid::nil());
    }

    #[test]
    fn rate_limit_constants_are_positive() {
        const { assert!(FEEDBACK_RATE_LIMIT_PER_24H > 0) };
        assert_eq!(FEEDBACK_RATE_WINDOW_SECS, 24 * 60 * 60);
        assert_eq!(FEEDBACK_MIN_ACCOUNT_AGE_DAYS, 3);
    }

    #[test]
    fn error_variants_display() {
        assert!(
            !FeedbackError::InvalidComment("too long".into())
                .to_string()
                .is_empty()
        );
        assert!(!FeedbackError::InvalidSentiment.to_string().is_empty());
        assert!(!FeedbackError::RateLimited.to_string().is_empty());
        assert!(!FeedbackError::NotFound.to_string().is_empty());
        assert!(
            !FeedbackError::Persistence("disk".into())
                .to_string()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod prop {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn rate_limit_exceeded_is_threshold_exact(
            count in 0u64..128,
            limit in 1u64..16,
        ) {
            prop_assert_eq!(
                rate_limit_exceeded(count, limit),
                count >= limit,
                "exceeded must hold exactly at the threshold"
            );
        }

        #[test]
        fn six_submissions_with_limit_five_yield_exactly_one_rejection(
            filler in proptest::collection::vec(any::<bool>(), 6),
        ) {
            // Model the rolling-window budget: the service accepts while the
            // observed count is below the limit. Six submissions against a
            // limit of five always produce exactly one rejection, regardless
            // of which submission is rejected (count is cumulative here).
            let mut accepted = 0u64;
            let mut rejected = 0u64;
            for _ in filler {
                if rate_limit_exceeded(accepted, FEEDBACK_RATE_LIMIT_PER_24H) {
                    rejected += 1;
                } else {
                    accepted += 1;
                }
            }
            prop_assert_eq!(accepted, FEEDBACK_RATE_LIMIT_PER_24H);
            prop_assert_eq!(rejected, 1);
        }
    }
}
