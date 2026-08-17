//! Mail anti-spam and filtering application layer: SQLite
//! repository, mail filter service, sieve compiler, spam scorer,
//! mailing list service.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{MODULE_NAME, MailFilteringModule};
pub use repo::SqliteMailFilterRepository;
pub use service::{
    MailFilterService, MailingListService, ScoreOutcome, SieveCompiler, SpamScorer, route_for_score,
};
