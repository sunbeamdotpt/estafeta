use tonic::Extensions;

#[derive(Debug, Clone)]
pub struct AuthClaims {
    pub subject: String,
    pub scopes: Vec<String>,
}

impl AuthClaims {
    pub fn from_extensions(ext: &Extensions) -> Result<&Self, tonic::Status> {
        ext.get::<Self>()
            .ok_or_else(|| tonic::Status::unauthenticated("missing auth claims"))
    }

    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_scope() {
        let claims = AuthClaims {
            subject: "user-1".into(),
            scopes: vec!["admin".into(), "read".into()],
        };
        assert!(claims.has_scope("admin"));
        assert!(claims.has_scope("read"));
        assert!(!claims.has_scope("write"));
    }

    #[test]
    fn test_has_scope_empty() {
        let claims = AuthClaims {
            subject: "user-1".into(),
            scopes: vec![],
        };
        assert!(!claims.has_scope("anything"));
    }

    #[test]
    fn test_from_extensions_missing() {
        let ext = Extensions::default();
        let result = AuthClaims::from_extensions(&ext);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_from_extensions_present() {
        let mut ext = Extensions::default();
        ext.insert(AuthClaims {
            subject: "user-42".into(),
            scopes: vec!["test".into()],
        });
        let claims = AuthClaims::from_extensions(&ext).unwrap();
        assert_eq!(claims.subject, "user-42");
    }
}
