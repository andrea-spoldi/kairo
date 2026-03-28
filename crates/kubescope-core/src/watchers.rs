use std::collections::HashMap;

use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client, runtime::watcher};
use tokio::sync::mpsc::UnboundedSender;

use crate::{CoreError, models::PodSummary};

/// Watch pods in the given namespace (or all namespaces if `None`) and send
/// a full sorted `Vec<PodSummary>` snapshot on every change.
pub async fn pod_watcher(
    client: Client,
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
