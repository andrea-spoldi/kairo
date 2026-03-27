use kube::config::{Config, KubeConfigOptions, Kubeconfig};

use crate::CoreError;

impl std::fmt::Debug for KubeClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KubeClient")
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}

/// Wraps a `kube::Client` and records which context it was built from.
#[derive(Clone)]
pub struct KubeClient {
    /// The underlying kube client.
    pub client: kube::Client,
    /// The kubeconfig context this client was created for.
    pub context: String,
}

impl KubeClient {
    /// Create a client using the kubeconfig's current-context.
    pub async fn try_default() -> Result<Self, CoreError> {
        let kubeconfig = Kubeconfig::read()?;
        let context = kubeconfig
            .current_context
            .clone()
            .unwrap_or_default();
        let config = Config::from_custom_kubeconfig(
            kubeconfig,
            &KubeConfigOptions::default(),
        )
        .await?;
        let client = kube::Client::try_from(config)?;
        Ok(Self { client, context })
    }

    /// Create a client for the given named context.
    pub async fn for_context(context: impl Into<String>) -> Result<Self, CoreError> {
        let context = context.into();
        let kubeconfig = Kubeconfig::read()?;

        // Verify the context exists before building.
        let exists = kubeconfig.contexts.iter().any(|c| c.name == context);
        if !exists {
            return Err(CoreError::ContextNotFound(context));
        }

        let config = Config::from_custom_kubeconfig(
            kubeconfig,
            &KubeConfigOptions {
                context: Some(context.clone()),
                ..Default::default()
            },
        )
        .await?;
        let client = kube::Client::try_from(config)?;
        Ok(Self { client, context })
    }

    /// Return all context names present in the default kubeconfig.
    pub fn list_contexts() -> Result<Vec<String>, CoreError> {
        let kubeconfig = Kubeconfig::read()?;
        Ok(kubeconfig.contexts.into_iter().map(|c| c.name).collect())
    }

    /// Return the current-context name from the default kubeconfig.
    pub fn current_context() -> Result<Option<String>, CoreError> {
        let kubeconfig = Kubeconfig::read()?;
        Ok(kubeconfig.current_context)
    }
}
