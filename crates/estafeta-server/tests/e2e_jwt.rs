mod common;

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey};
use rsa::pkcs8::LineEnding;
use rsa::RsaPrivateKey;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Generate an RSA key pair and return (private_key_pem, n_base64url, e_base64url).
fn generate_rsa_keys() -> (String, String, String) {
    let mut rng = rand::thread_rng();
    let private_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
    let public_key = private_key.to_public_key();

    let private_pem = private_key.to_pkcs1_pem(LineEnding::LF).unwrap();

    // Extract n and e from the public key for JWKS
    let pub_der = public_key.to_pkcs1_der().unwrap();
    let pub_doc: rsa::pkcs1::RsaPublicKey<'_> =
        rsa::pkcs1::der::Decode::from_der(pub_der.as_bytes()).unwrap();

    let n_bytes = pub_doc.modulus.as_bytes();
    let e_bytes = pub_doc.public_exponent.as_bytes();

    let n_b64 = base64_url_encode(n_bytes);
    let e_b64 = base64_url_encode(e_bytes);

    (private_pem.to_string(), n_b64, e_b64)
}

fn base64_url_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((n >> 18) & 63) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        }
    }
    result
}

#[tokio::test]
async fn test_validate_real_jwt() {
    let (private_pem, n_b64, e_b64) = generate_rsa_keys();

    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "keys": [{
                "kty": "RSA",
                "kid": "test-key-1",
                "alg": "RS256",
                "n": n_b64,
                "e": e_b64,
                "use": "sig"
            }]
        })))
        .mount(&mock_server)
        .await;

    let jwks = estafeta_server::auth::JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        None,
    );
    jwks.refresh().await.unwrap();

    // Create a real JWT signed with our private key
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key-1".to_string());

    let claims = json!({
        "sub": "user-42",
        "iat": chrono::Utc::now().timestamp(),
        "exp": (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp(),
        "scp": ["admin", "read"]
    });

    let encoding_key = EncodingKey::from_rsa_pem(private_pem.as_bytes()).unwrap();
    let token = encode(&header, &claims, &encoding_key).unwrap();

    // Validate
    let auth_claims = jwks.validate_token(&token).await.unwrap();
    assert_eq!(auth_claims.subject, "user-42");
    assert_eq!(auth_claims.scopes, vec!["admin", "read"]);
}

#[tokio::test]
async fn test_validate_jwt_with_space_delimited_scope() {
    let (private_pem, n_b64, e_b64) = generate_rsa_keys();

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "keys": [{"kty": "RSA", "kid": "k1", "n": n_b64, "e": e_b64}]
        })))
        .mount(&mock_server)
        .await;

    let jwks = estafeta_server::auth::JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        None,
    );
    jwks.refresh().await.unwrap();

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("k1".to_string());

    // Use space-delimited scope string (OAuth2 style)
    let claims = json!({
        "sub": "user-space",
        "exp": (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp(),
        "scope": "admin read write"
    });

    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(private_pem.as_bytes()).unwrap(),
    )
    .unwrap();

    let auth_claims = jwks.validate_token(&token).await.unwrap();
    assert_eq!(auth_claims.subject, "user-space");
    assert_eq!(auth_claims.scopes, vec!["admin", "read", "write"]);
}

#[tokio::test]
async fn test_validate_expired_jwt() {
    let (private_pem, n_b64, e_b64) = generate_rsa_keys();

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "keys": [{"kty": "RSA", "kid": "k1", "n": n_b64, "e": e_b64}]
        })))
        .mount(&mock_server)
        .await;

    let jwks = estafeta_server::auth::JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        None,
    );
    jwks.refresh().await.unwrap();

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("k1".to_string());

    let claims = json!({
        "sub": "expired-user",
        "exp": (chrono::Utc::now() - chrono::Duration::hours(1)).timestamp(),
    });

    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(private_pem.as_bytes()).unwrap(),
    )
    .unwrap();

    let result = jwks.validate_token(&token).await;
    assert!(result.is_err());
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
    assert!(status.message().contains("token validation failed"));
}

#[tokio::test]
async fn test_validate_jwt_no_scopes() {
    let (private_pem, n_b64, e_b64) = generate_rsa_keys();

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "keys": [{"kty": "RSA", "kid": "k1", "n": n_b64, "e": e_b64}]
        })))
        .mount(&mock_server)
        .await;

    let jwks = estafeta_server::auth::JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        None,
    );
    jwks.refresh().await.unwrap();

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("k1".to_string());

    let claims = json!({
        "sub": "no-scopes-user",
        "exp": (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp(),
    });

    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(private_pem.as_bytes()).unwrap(),
    )
    .unwrap();

    let auth_claims = jwks.validate_token(&token).await.unwrap();
    assert_eq!(auth_claims.subject, "no-scopes-user");
    assert!(auth_claims.scopes.is_empty());
}

#[tokio::test]
async fn test_validate_jwt_with_issuer() {
    let (private_pem, n_b64, e_b64) = generate_rsa_keys();

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "keys": [{"kty": "RSA", "kid": "k1", "n": n_b64, "e": e_b64}]
        })))
        .mount(&mock_server)
        .await;

    let jwks = estafeta_server::auth::JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        Some("https://auth.example.com".to_string()),
    );
    jwks.refresh().await.unwrap();

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("k1".to_string());

    // Token with matching issuer
    let claims = json!({
        "sub": "issuer-user",
        "iss": "https://auth.example.com",
        "exp": (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp(),
    });

    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(private_pem.as_bytes()).unwrap(),
    )
    .unwrap();

    let auth_claims = jwks.validate_token(&token).await.unwrap();
    assert_eq!(auth_claims.subject, "issuer-user");

    // Token with wrong issuer should fail
    let claims = json!({
        "sub": "wrong-issuer",
        "iss": "https://wrong.example.com",
        "exp": (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp(),
    });

    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(private_pem.as_bytes()).unwrap(),
    )
    .unwrap();

    let result = jwks.validate_token(&token).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_validate_token_missing_kid() {
    let (private_pem, n_b64, e_b64) = generate_rsa_keys();

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "keys": [{"kty": "RSA", "kid": "k1", "n": n_b64, "e": e_b64}]
        })))
        .mount(&mock_server)
        .await;

    let jwks = estafeta_server::auth::JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        None,
    );
    jwks.refresh().await.unwrap();

    // Token without kid in header
    let header = Header::new(Algorithm::RS256); // no kid set

    let claims = json!({
        "sub": "no-kid",
        "exp": (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp(),
    });

    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(private_pem.as_bytes()).unwrap(),
    )
    .unwrap();

    let result = jwks.validate_token(&token).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("missing kid"));
}
