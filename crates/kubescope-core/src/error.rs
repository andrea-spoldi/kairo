use thiserror::Error;

/// Top-level error type for `kubescope-core`.
#[derive(Debug, Error)]
pub enum CoreError {
    /// Kubeconfig could not be loaded or parsed.
    #[error("kubeconfig error: {0}")]
    KubeConfig(#[from] kube::config::KubeconfigError),

    /// A Kubernetes API call failed.
    #[error("kube error: {0}")]
    Kube(#[from] kube::Error),

    /// The requested context does not exist in the kubeconfig.
    #[error("context not found: {0}")]
    ContextNotFound(String),
}
