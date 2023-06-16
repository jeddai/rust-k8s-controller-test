use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),

    #[error("Kube Error: {0}")]
    KubeError(#[source] kube::Error),

    #[error("Finalizer Error: {0}")]
    // NB: awkward type because finalizer::Error embeds the reconciler error (which is this)
    // so boxing this error to break cycles
    FinalizerError(#[source] Box<kube::runtime::finalizer::Error<Error>>),

    // #[error("IllegalDocument")]
    // IllegalDocument,
    #[error("Invalid configuration: {0}")]
    InvalidConfigurationError(String),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Expose all controller components used by main
pub mod controller;
pub mod constants;
pub mod deployments;
pub use crate::controller::*;

// #[cfg(test)] pub mod fixtures;