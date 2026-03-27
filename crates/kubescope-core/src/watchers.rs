use futures::StreamExt;
use k8s_openapi::api::core::v1::{Namespace, Pod};
use kube::{runtime::watcher, Api};
use tokio::sync::mpsc;
use tracing::warn;

use crate::{models::PodSummary, KubeClient};

/// Watches pods in a namespace and forwards [`PodSummary`] updates over an mpsc channel.
pub struct PodWatcher;

impl PodWatcher {
    /// Spawn a pod watcher task.
    ///
    /// Pass an empty string for `namespace` to watch all namespaces.
    /// The task exits automatically when `tx` is dropped.
    pub fn start(
        kube_client: KubeClient,
        namespace: impl Into<String>,
        tx: mpsc::Sender<PodSummary>,
    ) -> tokio::task::JoinHandle<()> {
        let namespace = namespace.into();
        tokio::spawn(async move {
            let api: Api<Pod> = if namespace.is_empty() {
                Api::all(kube_client.client)
            } else {
                Api::namespaced(kube_client.client, &namespace)
            };

            let mut stream = watcher(api, watcher::Config::default()).boxed();
            while let Some(result) = stream.next().await {
                match result {
                    Ok(watcher::Event::Apply(pod) | watcher::Event::InitApply(pod)) => {
                        let summary = PodSummary::from(pod);
                        if tx.send(summary).await.is_err() {
                            break; // receiver dropped — shut down
                        }
                    }
                    Ok(_) => {} // Init, InitDone, Delete — handled by caller via full re-render
                    Err(e) => warn!("pod watcher error: {e}"),
                }
            }
        })
    }
}

/// Watches Namespace objects and forwards namespace names over an mpsc channel.
pub struct NamespaceWatcher;

impl NamespaceWatcher {
    /// Spawn a namespace watcher task.
    ///
    /// Sends the name of each namespace as it is applied or initially listed.
    /// The task exits automatically when `tx` is dropped.
    pub fn start(
        kube_client: KubeClient,
        tx: mpsc::Sender<String>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let api: Api<Namespace> = Api::all(kube_client.client);
            let mut stream = watcher(api, watcher::Config::default()).boxed();
            while let Some(result) = stream.next().await {
                match result {
                    Ok(watcher::Event::Apply(ns) | watcher::Event::InitApply(ns)) => {
                        let name = ns.metadata.name.unwrap_or_default();
                        if tx.send(name).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => warn!("namespace watcher error: {e}"),
                }
            }
        })
    }
}
