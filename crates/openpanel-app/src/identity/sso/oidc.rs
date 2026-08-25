//! OIDC protocol adapter implemented directly over `reqwest` +
//! `serde_json` (discovery document, authorization URL, code
//! exchange). The ID token is fetched server-to-server from the token
//! endpoint over TLS using the confidential-client secret; `iss` and
//! `nonce` claims are verified locally. JWKS signature verification is
//! defence-in-depth tracked as follow-up work.

use base64::Engine;
use openpanel_domain::identity::sso::{
    OidcClaims, OidcPort, SsoConnection, SsoError, SsoLoginState,
};
use sha2::{Digest, Sha256};

/// Production [`OidcPort`].
pub struct OpenidConnectAdapter {
    /// Registered redirect URI for the panel callback.
    pub redirect_uri: String,
    /// Client secret resolver: decrypts the stored ciphertext.
    pub secret_resolver: ArcSecretResolver,
}

/// Resolves the stored ciphertext into the plaintext client secret.
pub type ArcSecretResolver = std::sync::Arc<dyn Fn(&str) -> Result<String, SsoError> + Send + Sync>;

impl OpenidConnectAdapter {
    /// Construct the adapter with a secret-decrypting closure.
    pub fn new(redirect_uri: impl Into<String>, secret_resolver: ArcSecretResolver) -> Self {
        Self {
            redirect_uri: redirect_uri.into(),
            secret_resolver,
        }
    }
}

#[derive(Debug, Clone)]
struct Endpoints {
    authorization: String,
    token: String,
}

async fn discover(connection: &SsoConnection) -> Result<Endpoints, SsoError> {
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        connection.issuer_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .get(discovery_url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| SsoError::Discovery)?;
    if !response.status().is_success() {
        return Err(SsoError::Discovery);
    }
    let json: serde_json::Value = response.json().await.map_err(|_| SsoError::Discovery)?;
    let get = |key: &str| -> Result<String, SsoError> {
        json.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or(SsoError::Discovery)
    };
    Ok(Endpoints {
        authorization: get("authorization_endpoint")?,
        token: get("token_endpoint")?,
    })
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(serde::Deserialize)]
struct IdTokenClaims {
    iss: String,
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    nonce: Option<String>,
}

#[async_trait::async_trait]
impl OidcPort for OpenidConnectAdapter {
    async fn authorize_url(
        &self,
        connection: &SsoConnection,
        state: &SsoLoginState,
    ) -> Result<String, SsoError> {
        let endpoints = discover(connection).await?;
        let url = reqwest::Url::parse_with_params(
            &endpoints.authorization,
            &[
                ("response_type", "code"),
                ("client_id", connection.client_id.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("scope", "openid email"),
                ("state", state.state.as_str()),
                ("nonce", state.nonce.as_str()),
                (
                    "code_challenge",
                    pkce_challenge(&state.pkce_verifier).as_str(),
                ),
                ("code_challenge_method", "S256"),
            ],
        )
        .map_err(|_| SsoError::InvalidConfig("authorization endpoint URL invalid".into()))?;
        Ok(url.to_string())
    }

    async fn exchange(
        &self,
        connection: &SsoConnection,
        code: &str,
        pkce_verifier: &str,
        expected_nonce: &str,
    ) -> Result<OidcClaims, SsoError> {
        let endpoints = discover(connection).await?;
        let secret = (self.secret_resolver)(&connection.client_secret_cipher)?;
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("client_id", connection.client_id.as_str()),
            ("client_secret", secret.as_str()),
            ("code_verifier", pkce_verifier),
        ];
        let response = reqwest::Client::new()
            .post(&endpoints.token)
            .form(&params)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|_| SsoError::Discovery)?;
        if !response.status().is_success() {
            return Err(SsoError::Discovery);
        }
        let token: TokenResponse = response.json().await.map_err(|_| SsoError::Discovery)?;

        // Decode the ID-token payload (middle segment).
        let payload_b64 = token
            .id_token
            .split('.')
            .nth(1)
            .ok_or(SsoError::NonceMismatch)?;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| SsoError::NonceMismatch)?;
        let claims: IdTokenClaims =
            serde_json::from_slice(&payload).map_err(|_| SsoError::NonceMismatch)?;

        // Local claim checks: issuer must match the configured
        // connection and the nonce must bind to our outstanding login.
        if !claims
            .iss
            .trim_end_matches('/')
            .eq_ignore_ascii_case(connection.issuer_url.trim_end_matches('/'))
        {
            return Err(SsoError::NonceMismatch);
        }
        match &claims.nonce {
            Some(nonce) if nonce == expected_nonce => {}
            _ => return Err(SsoError::NonceMismatch),
        }

        Ok(OidcClaims {
            issuer: claims.iss,
            subject: claims.sub,
            email: claims.email,
        })
    }
}
