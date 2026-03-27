//! Integration example: connect to the current kubeconfig context and stream
//! pod events from all namespaces to stdout.
//!
//! Requires a live cluster:
//!   cargo run -p kubescope-core --example watch_pods

use kubescope_core::{
    client::KubeClient,
    watchers::{NamespaceWatcher, PodWatcher},
};
use tokio::sync::mpsc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let client = KubeClient::try_default().await?;
    info!("connected to context: {}", client.context);

    // Watch namespaces.
    let (ns_tx, mut ns_rx) = mpsc::channel(32);
    NamespaceWatcher::start(client.clone(), ns_tx);

    // Watch pods across all namespaces.
    let (pod_tx, mut pod_rx) = mpsc::channel(256);
    PodWatcher::start(client.clone(), "", pod_tx);

    println!("Watching pods and namespaces — press Ctrl-C to stop.\n");

    loop {
        tokio::select! {
            Some(ns) = ns_rx.recv() => {
                println!("[namespace] {ns}");
            }
            Some(pod) = pod_rx.recv() => {
                println!(
                    "[pod] {}/{} \tstatus={} ready={} restarts={} node={}",
                    pod.namespace, pod.name, pod.status, pod.ready, pod.restarts, pod.node
                );
            }
        }
    }
}
