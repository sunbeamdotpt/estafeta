use anyhow::Result;
use jsonwebtoken::{decode_header, Algorithm, DecodingKey, Validation};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::claims::AuthClaims;

#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

#[derive(Debug, Deserialize)]
struct JwkKey {
    kid: String,
    kty: String,
    alg: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

/// Client that fetches and caches JSON Web Key Sets for JWT validation.
///
/// Keys are cached in memory and automatically refreshed when an unknown `kid` is encountered
/// or via a periodic background task.
#[derive(Clone)]
pub struct JwksClient {
    http: Client,
    jwks_url: String,
    issuer: Option<String>,
    keys: Arc<RwLock<HashMap<String, DecodingKey>>>,
}

impl JwksClient {
    /// Create a new client pointing at the given JWKS endpoint.
    pub fn new(jwks_url: String, issuer: Option<String>) -> Self {
        Self {
            http: Client::new(),
            jwks_url,
            issuer,
            keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Fetch the latest key set from the JWKS endpoint and replace the local cache.
    pub async fn refresh(&self) -> Result<()> {
        let resp: JwksResponse = self
            .http
            .get(&self.jwks_url)
            .send()
            .await?
            .json()
            .await?;

        let mut keys = HashMap::new();
        for jwk in &resp.keys {
            if jwk.kty == "RSA" {
                if let (Some(n), Some(e)) = (&jwk.n, &jwk.e) {
                    match DecodingKey::from_rsa_components(n, e) {
                        Ok(key) => {
                            keys.insert(jwk.kid.clone(), key);
                        }
                        Err(err) => {
                            warn!(kid = %jwk.kid, error = %err, "failed to parse JWK");
                        }
                    }
                }
            }
        }

        info!(key_count = keys.len(), "refreshed JWKS keys");
        *self.keys.write().await = keys;
        Ok(())
    }

    /// Decode and validate a Bearer JWT, returning the extracted [`AuthClaims`].
    ///
    /// If the token's `kid` is not in the local cache, a JWKS refresh is triggered automatically.
    pub async fn validate_token(&self, token: &str) -> Result<AuthClaims, tonic::Status> {
        let header = decode_header(token)
            .map_err(|e| tonic::Status::unauthenticated(format!("invalid token header: {e}")))?;

        let kid = header
            .kid
            .ok_or_else(|| tonic::Status::unauthenticated("token missing kid"))?;

        // Try cached key first
        let key = {
            let keys = self.keys.read().await;
            keys.get(&kid).cloned()
        };

        let key = match key {
            Some(k) => k,
            None => {
                // Force refresh on unknown kid
                self.refresh().await.map_err(|e| {
                    tonic::Status::internal(format!("failed to refresh JWKS: {e}"))
                })?;
                let keys = self.keys.read().await;
                keys.get(&kid)
                    .cloned()
                    .ok_or_else(|| tonic::Status::unauthenticated("unknown signing key"))?
            }
        };

        let mut validation = Validation::new(Algorithm::RS256);
        if let Some(ref issuer) = self.issuer {
            validation.set_issuer(&[issuer]);
        } else {
            validation.validate_aud = false;
        }
        validation.validate_aud = false;

        let token_data =
            jsonwebtoken::decode::<serde_json::Value>(token, &key, &validation)
                .map_err(|e| tonic::Status::unauthenticated(format!("token validation failed: {e}")))?;

        let claims = &token_data.claims;
        let subject = claims
            .get("sub")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let scopes = claims
            .get("scp")
            .or_else(|| claims.get("scope"))
            .and_then(|v| {
                if let Some(arr) = v.as_array() {
                    Some(
                        arr.iter()
                            .filter_map(|s| s.as_str().map(String::from))
                            .collect(),
                    )
                } else if let Some(s) = v.as_str() {
                    Some(s.split_whitespace().map(String::from).collect())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        Ok(AuthClaims { subject, scopes })
    }

    /// Spawn a background task that refreshes JWKS keys periodically.
    pub fn spawn_refresh_task(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1800));
            loop {
                interval.tick().await;
                if let Err(e) = self.refresh().await {
                    warn!(error = %e, "background JWKS refresh failed");
                }
            }
        })
    }
}
