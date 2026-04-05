mod common;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Test JWT validation against a real wiremock JWKS endpoint.
#[tokio::test]
async fn test_jwks_client_refresh() {
    let mock_server = MockServer::start().await;

    // Serve a JWKS with one RSA key
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "kid": "test-key-1",
                "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                "e": "AQAB"
            }]
        })))
        .mount(&mock_server)
        .await;

    let jwks = estafeta_server::auth::JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        None,
    );

    // Refresh should succeed
    jwks.refresh().await.unwrap();

    // Validate a garbage token — should fail with proper error
    let result = jwks.validate_token("garbage.token.here").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_jwks_client_unknown_kid_triggers_refresh() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "keys": []
        })))
        .expect(2..) // initial refresh + force-refresh on unknown kid
        .mount(&mock_server)
        .await;

    let jwks = estafeta_server::auth::JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        None,
    );
    jwks.refresh().await.unwrap();

    // Create a token with a header that has a kid not in the JWKS
    // We'll manually craft a JWT header
    let header = base64_url_encode(r#"{"alg":"RS256","kid":"unknown-kid"}"#);
    let payload = base64_url_encode(r#"{"sub":"user-1"}"#);
    let fake_token = format!("{header}.{payload}.fake-signature");

    let result = jwks.validate_token(&fake_token).await;
    assert!(result.is_err()); // unknown signing key after refresh
}

#[tokio::test]
async fn test_jwks_client_server_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let jwks = estafeta_server::auth::JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        None,
    );

    // Refresh should fail gracefully
    let result = jwks.refresh().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_keto_client_check_allowed() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/relation-tuples/check"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "allowed": true
        })))
        .mount(&mock_server)
        .await;

    let keto = estafeta_server::auth::KetoClient::new(mock_server.uri());

    assert!(keto.check_admin("admin-user").await.unwrap());
    assert!(keto.check_service_send("email-svc", "svc-user").await.unwrap());
    keto.require_admin("admin-user").await.unwrap();
    keto.require_service_send("email-svc", "svc-user").await.unwrap();
}

#[tokio::test]
async fn test_keto_client_check_denied() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/relation-tuples/check"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock_server)
        .await;

    let keto = estafeta_server::auth::KetoClient::new(mock_server.uri());

    assert!(!keto.check_admin("nobody").await.unwrap());
    let err = keto.require_admin("nobody").await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::PermissionDenied);

    let err = keto.require_service_send("svc", "nobody").await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn test_keto_client_server_unavailable() {
    // Connect to a port that nothing is listening on
    let keto = estafeta_server::auth::KetoClient::new("http://127.0.0.1:1".to_string());

    let err = keto.check_admin("user").await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Internal);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_auth_interceptor() {
    use estafeta_server::auth::{AuthInterceptor, JwksClient};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "keys": []
        })))
        .mount(&mock_server)
        .await;

    let jwks = JwksClient::new(
        format!("{}/.well-known/jwks.json", mock_server.uri()),
        None,
    );
    jwks.refresh().await.unwrap();

    let mut interceptor = AuthInterceptor::new(jwks);

    // Request without authorization header should fail
    use tonic::service::Interceptor;
    let request = tonic::Request::new(());
    let result = interceptor.call(request);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);

    // Request with invalid token should fail
    let mut request = tonic::Request::new(());
    request
        .metadata_mut()
        .insert("authorization", "Bearer invalid.token.here".parse().unwrap());
    let result = interceptor.call(request);
    assert!(result.is_err());
}

fn base64_url_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let data = input.as_bytes();
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
