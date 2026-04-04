use super::jwks::JwksClient;
use tonic::service::Interceptor;

#[derive(Clone)]
pub struct AuthInterceptor {
    jwks: JwksClient,
}

impl AuthInterceptor {
    pub fn new(jwks: JwksClient) -> Self {
        Self { jwks }
    }
}

impl Interceptor for AuthInterceptor {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        let token = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        let token = token
            .ok_or_else(|| tonic::Status::unauthenticated("missing authorization header"))?;

        // We need to validate async, but interceptors are sync.
        // Store the raw token; we'll validate in a tower layer or per-RPC instead.
        // For a sync interceptor, we do a blocking validation via a handle.
        let jwks = self.jwks.clone();
        let claims = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(jwks.validate_token(&token))
        })?;

        request.extensions_mut().insert(claims);
        Ok(request)
    }
}
