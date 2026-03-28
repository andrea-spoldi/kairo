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

    /// A watcher stream error.
    #[error("watcher error: {0}")]
    Watcher(#[from] kube::runtime::watcher::Error),

    /// The requested context does not exist in the kubeconfig.
    #[error("context not found: {0}")]
    ContextNotFound(String),

    /// An I/O error (e.g. reading log stream lines).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
