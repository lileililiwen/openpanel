//! Integration tests for the ssl bounded context.
//!
//! These tests exercise the issuance state machine, error
//! classification, preflight, and a small offline ACME fixture
//! against the production `RustlsAcmeClient` re-pointed at a local
//! axum server with the Let's Encrypt wire shape.
//!
//! They live behind the `ssl` module rather than under
//! `tests/` because they need to import the in-crate items
//! (especially the `IssuanceError` classify matrix and the
//! `MockAcmeServer` test fixture).
