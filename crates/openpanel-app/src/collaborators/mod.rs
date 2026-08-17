//! Per-site collaborator bounded context: invite, resolve, and revoke
//! collaborators scoped to specific sites.

pub mod module;
pub mod repo;
pub mod service;
#[cfg(test)]
mod service_tests;

pub use module::{CollaboratorsModule, MODULE_NAME};
pub use repo::{SqliteCollaboratorRepository, SqliteSiteGrantRepository};
pub use service::{
    CollaboratorService, GrantResolver, InviteCollaboratorError, InviteRequest, UpdateRequest,
};
