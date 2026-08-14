//! IaC (Infrastructure as Code) bounded context.
//!
//! Describes the contract for a typed client SDK and a Terraform
//! provider generated from the OpenAPI spec. The bounded context
//! stays focused on the *contract* — what resources / operations
//! the SDK and provider expose, and the sync guarantee that keeps
//! them aligned with `openapi.rs`. The actual code generation lives
//! in `openpanel-app` (`iac` module).

pub mod contract;
pub mod error;
pub mod provider;
pub mod sdk;

pub use contract::{ApiContract, Operation, OperationId, ResourceEndpoint};
pub use error::IacError;
pub use provider::{ProviderResource, ResourceKind, TerraformProvider};
pub use sdk::{Language, SdkPackage, SdkSurface};