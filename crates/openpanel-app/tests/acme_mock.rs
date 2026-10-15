//! Mock ACME integration test.
//!
//! Exercises the bridge the live `RustlsAcmeClient` uses for HTTP-01:
//! an `AcmeHttpServer` with a resolver-style delegated token lookup
//! (the same hook `RustlsOrderDriver::drive` attaches), served over
//! the loopback port nginx forwards `/.well-known/acme-challenge/`
//! to. The challenge must be answerable while an issuance is in
//! flight and stop being answerable once issuance detaches the hook.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use openpanel_app::ssl::challenge_server::{AcmeHttpServer, TokenLookup};

/// Simulates the rustls-acme resolver: one active challenge at a
/// time, keyed by token.
fn resolver_like_lookup() -> (TokenLookup, Arc<Mutex<HashMap<String, String>>>) {
    let data: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let shared = data.clone();
    let lookup: TokenLookup =
        Arc::new(move |token: &str| shared.lock().expect("resolver fixture").get(token).cloned());
    (lookup, data)
}

#[tokio::test]
async fn resolver_style_lookup_answers_http01_over_the_served_port() {
    let server = AcmeHttpServer::new();
    let (lookup, resolver_data) = resolver_like_lookup();
    server.attach_token_lookup(lookup);
    let addr = openpanel_app::ssl::challenge_server::serve(server.clone(), 0)
        .await
        .expect("challenge server binds");

    // Mid-issuance: the order machine registers challenge data.
    resolver_data
        .lock()
        .expect("resolver fixture")
        .insert("mock-token".to_owned(), "mock-key-auth".to_owned());

    let resp = reqwest::get(format!(
        "http://{addr}/.well-known/acme-challenge/mock-token"
    ))
    .await
    .expect("challenge GET");
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.expect("body"), "mock-key-auth");

    // A token the resolver never held is still a 404.
    let miss = reqwest::get(format!(
        "http://{addr}/.well-known/acme-challenge/other-token"
    ))
    .await
    .expect("challenge GET (miss)");
    assert_eq!(miss.status(), 404);
}

#[tokio::test]
async fn detached_lookup_stops_answering_after_issuance() {
    let server = AcmeHttpServer::new();
    let (lookup, resolver_data) = resolver_like_lookup();
    server.attach_token_lookup(lookup);
    let addr = openpanel_app::ssl::challenge_server::serve(server.clone(), 0)
        .await
        .expect("challenge server binds");
    resolver_data
        .lock()
        .expect("resolver fixture")
        .insert("mock-token".to_owned(), "mock-key-auth".to_owned());

    server.detach_token_lookup();
    let resp = reqwest::get(format!(
        "http://{addr}/.well-known/acme-challenge/mock-token"
    ))
    .await
    .expect("challenge GET after detach");
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn manual_registration_takes_precedence_over_the_hook() {
    let server = AcmeHttpServer::new();
    server.register("manual.example.com", "shared-token", "manual-key-auth");
    let (lookup, _data) = resolver_like_lookup();
    server.attach_token_lookup(lookup);
    let addr = openpanel_app::ssl::challenge_server::serve(server.clone(), 0)
        .await
        .expect("challenge server binds");

    let resp = reqwest::get(format!(
        "http://{addr}/.well-known/acme-challenge/shared-token"
    ))
    .await
    .expect("challenge GET");
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.expect("body"), "manual-key-auth");
}
