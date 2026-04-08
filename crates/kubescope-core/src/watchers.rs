use std::collections::HashMap;

use futures::StreamExt;
use k8s_openapi::api::core::v1::{Event as K8sEvent, Namespace, Pod};
use kube::{Api, runtime::watcher};
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

use crate::{CoreError, KubeClient, models::{ClusterEvent, PodSummary}};

/// Watch pods in the given namespace (or all namespaces if `None`) and send
/// a full sorted `Vec<PodSummary>` snapshot on every change.
pub async fn pod_watcher(
    client: kube::Client,
    namespace: Option<String>,
    tx: UnboundedSender<Vec<PodSummary>>,
) -> Result<(), CoreError> {
    let api: Api<Pod> = match namespace.as_deref() {
        Some(ns) => Api::namespaced(client, ns),
        None => Api::all(client),
    };
    let mut store: HashMap<String, PodSummary> = HashMap::new();
    let stream = watcher(api, watcher::Config::default());
    futures::pin_mut!(stream);
    while let Some(ev) = stream.next().await {
        match ev? {
            watcher::Event::Apply(pod) => {
                store.insert(pod_key(&pod), PodSummary::from(pod));
                send_snapshot(&store, &tx);
            }
            watcher::Event::Delete(pod) => {
                store.remove(&pod_key(&pod));
                send_snapshot(&store, &tx);
            }
            watcher::Event::Init => {
                store.clear();
            }
            watcher::Event::InitApply(pod) => {
                store.insert(pod_key(&pod), PodSummary::from(pod));
            }
            watcher::Event::InitDone => {
                send_snapshot(&store, &tx);
            }
        }
    }
    Ok(())
}

fn pod_key(pod: &Pod) -> String {
    format!(
        "{}/{}",
        pod.metadata.namespace.as_deref().unwrap_or(""),
        pod.metadata.name.as_deref().unwrap_or("")
    )
}

fn send_snapshot(store: &HashMap<String, PodSummary>, tx: &UnboundedSender<Vec<PodSummary>>) {
    let mut pods: Vec<PodSummary> = store.values().cloned().collect();
    pods.sort_by(|a, b| a.namespace.cmp(&b.namespace).then(a.name.cmp(&b.name)));
    let _ = tx.send(pods);
}

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

/// Watches Kubernetes Events cluster-wide and forwards Warning-type events.
pub struct ClusterEventWatcher;

impl ClusterEventWatcher {
    /// Spawn a cluster event watcher task.
    ///
    /// Sends every Warning event as it is applied or initially listed.
    /// The task exits automatically when `tx` is dropped.
    pub fn start(
        kube_client: KubeClient,
        tx: mpsc::Sender<ClusterEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let api: Api<K8sEvent> = Api::all(kube_client.client);
            let mut stream = watcher(api, watcher::Config::default()).boxed();
            while let Some(result) = stream.next().await {
                match result {
                    Ok(watcher::Event::Apply(ev) | watcher::Event::InitApply(ev)) => {
                        // Client-side filter: only forward Warning events.
                        if ev.type_.as_deref() == Some("Warning") {
                            let cluster_ev = ClusterEvent::from(ev);
                            if tx.send(cluster_ev).await.is_err() {
                                break;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => warn!("cluster event watcher error: {e}"),
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
