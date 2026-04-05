use reqwest::Client;
use serde::Serialize;
use tracing::warn;

/// Client for the Ory Keto authorization service.
///
/// All permission checks operate in the `estafeta` namespace and return
/// gRPC-compatible status errors on failure.
#[derive(Clone)]
pub struct KetoClient {
    http: Client,
    base_url: String,
}

#[derive(Serialize)]
struct CheckRequest {
    namespace: &'static str,
    object: String,
    relation: String,
    subject_id: String,
}

impl KetoClient {
    /// Create a new client pointing at the given Keto base URL.
    pub fn new(base_url: String) -> Self {
        Self {
            http: Client::new(),
            base_url,
        }
    }

    /// Check if a subject has a relation on an object in the estafeta namespace.
    async fn check(
        &self,
        object: &str,
        relation: &str,
        subject_id: &str,
    ) -> Result<bool, tonic::Status> {
        let resp = self
            .http
            .post(format!("{}/relation-tuples/check", self.base_url))
            .json(&CheckRequest {
                namespace: "estafeta",
                object: object.to_string(),
                relation: relation.to_string(),
                subject_id: subject_id.to_string(),
            })
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "keto check request failed");
                tonic::Status::internal("authorization service unavailable")
            })?;

        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().await.map_err(|e| {
                warn!(error = %e, "keto response parse failed");
                tonic::Status::internal("authorization service error")
            })?;
            Ok(body.get("allowed").and_then(|v| v.as_bool()).unwrap_or(false))
        } else if resp.status().as_u16() == 403 {
            Ok(false)
        } else {
            warn!(status = %resp.status(), "unexpected keto response");
            Err(tonic::Status::internal("authorization service error"))
        }
    }

    /// Check if subject is an admin.
    pub async fn check_admin(&self, subject_id: &str) -> Result<bool, tonic::Status> {
        self.check("admin", "access", subject_id).await
    }

    /// Check if subject can send notifications for a service.
    pub async fn check_service_send(
        &self,
        service_slug: &str,
        subject_id: &str,
    ) -> Result<bool, tonic::Status> {
        self.check(
            &format!("services/{service_slug}"),
            "send",
            subject_id,
        )
        .await
    }

    /// Require admin, returning a gRPC PermissionDenied if not.
    pub async fn require_admin(&self, subject_id: &str) -> Result<(), tonic::Status> {
        if self.check_admin(subject_id).await? {
            Ok(())
        } else {
            Err(tonic::Status::permission_denied("admin access required"))
        }
    }

    /// Require service send permission.
    pub async fn require_service_send(
        &self,
        service_slug: &str,
        subject_id: &str,
    ) -> Result<(), tonic::Status> {
        if self.check_service_send(service_slug, subject_id).await? {
            Ok(())
        } else {
            Err(tonic::Status::permission_denied(format!(
                "no send permission for service '{service_slug}'"
            )))
        }
    }
}
