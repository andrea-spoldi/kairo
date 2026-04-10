use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{ConfigMap, Event, Node, Pod, Service};
use kube::api::ListParams;
use kube::config::{Config, KubeConfigOptions, Kubeconfig};
use kube::Api;

use crate::models::{PodDetail, PodEvent};
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

    /// Fetch full pod detail for the given pod name in a namespace.
    pub async fn fetch_pod_detail(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<PodDetail, CoreError> {
        let api: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        let pod = api.get(name).await?;
        Ok(PodDetail::from(pod))
    }

    /// Fetch the raw YAML representation of a Pod.
    pub async fn fetch_pod_yaml(&self, namespace: &str, name: &str) -> Result<String, CoreError> {
        let api: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        let obj = api.get(name).await?;
        serde_yaml::to_string(&obj).map_err(|e| CoreError::Other(e.to_string()))
    }

    /// Fetch the raw YAML representation of a Deployment.
    pub async fn fetch_deployment_yaml(&self, namespace: &str, name: &str) -> Result<String, CoreError> {
        let api: Api<Deployment> = Api::namespaced(self.client.clone(), namespace);
        let obj = api.get(name).await?;
        serde_yaml::to_string(&obj).map_err(|e| CoreError::Other(e.to_string()))
    }

    /// Fetch the raw YAML representation of a Service.
    pub async fn fetch_service_yaml(&self, namespace: &str, name: &str) -> Result<String, CoreError> {
        let api: Api<Service> = Api::namespaced(self.client.clone(), namespace);
        let obj = api.get(name).await?;
        serde_yaml::to_string(&obj).map_err(|e| CoreError::Other(e.to_string()))
    }

    /// Fetch the raw YAML representation of a ConfigMap.
    pub async fn fetch_configmap_yaml(&self, namespace: &str, name: &str) -> Result<String, CoreError> {
        let api: Api<ConfigMap> = Api::namespaced(self.client.clone(), namespace);
        let obj = api.get(name).await?;
        serde_yaml::to_string(&obj).map_err(|e| CoreError::Other(e.to_string()))
    }

    /// Fetch the raw YAML representation of a Node (cluster-scoped).
    pub async fn fetch_node_yaml(&self, name: &str) -> Result<String, CoreError> {
        let api: Api<Node> = Api::all(self.client.clone());
        let obj = api.get(name).await?;
        serde_yaml::to_string(&obj).map_err(|e| CoreError::Other(e.to_string()))
    }

    /// Fetch Kubernetes events related to a specific pod.
    pub async fn fetch_pod_events(
        &self,
        namespace: &str,
        pod_name: &str,
    ) -> Result<Vec<PodEvent>, CoreError> {
        let api: Api<Event> = Api::namespaced(self.client.clone(), namespace);
        let params = ListParams::default().fields(
            &format!("involvedObject.name={pod_name},involvedObject.kind=Pod"),
        );
        let event_list = api.list(&params).await?;
        Ok(event_list.into_iter().map(PodEvent::from).collect())
    }
}
