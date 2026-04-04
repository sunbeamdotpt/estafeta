pub mod estafeta {
    pub mod v1 {
        tonic::include_proto!("estafeta.v1");
    }
}

// Re-export for convenience
pub use estafeta::v1::*;
