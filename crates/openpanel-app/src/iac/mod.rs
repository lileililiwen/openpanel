//! Infrastructure-as-Code bounded context: parses an OpenAPI
//! document into an `ApiContract`, exposes the codegen scaffold
//! for the SDK and Terraform provider, and runs the drift check
//! against committed artifacts.

pub mod codegen;
pub mod openapi;
pub mod scaffold;

pub use codegen::{CodegenContract, CommittedArtifacts, DriftOutcome, GeneratedSurface};
pub use openapi::{OpenApiRef, ParsedOpenApi};
pub use scaffold::{
    render_go_stub, render_provider_stub, render_rust_stub, render_typescript_stub,
};
