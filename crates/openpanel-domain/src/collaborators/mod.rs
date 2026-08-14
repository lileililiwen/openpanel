//! Per-site collaborator bounded context.
//!
//! A collaborator is an account-scoped principal that may act on a
//! subset of the sites owned by that account, limited to a
//! permission set drawn from file / database / mail / cron. The
//! permission set is distinct from the global `identity::Role`
//! model: a collaborator never inherits the owner's role, only the
//! scope of their explicit `SiteGrant`s.

pub mod error;
pub mod grant;
pub mod permission;
pub mod repository;

pub use error::CollaboratorError;
pub use grant::{CollabStatus, Collaborator, CollaboratorId, SiteGrant};
pub use permission::{Permission, PermissionSet};
pub use repository::{CollaboratorRepository, SiteGrantRepository};