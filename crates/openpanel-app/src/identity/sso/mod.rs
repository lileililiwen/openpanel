//! SSO bounded context: SQLite repository, OIDC adapter,
//! orchestration service, and composition module.

pub mod module;
mod oidc;
mod repo;
mod service;

pub use module::{MODULE_NAME, SsoModule};
pub use oidc::OpenidConnectAdapter;
pub use openpanel_domain::identity::sso::{
    ExternalIdentity, OidcClaims, OidcPort, SsoConnection, SsoError, SsoLoginState,
};
pub use repo::SqliteSsoRepository;
pub use service::{CallbackOutcome, SsoService};
