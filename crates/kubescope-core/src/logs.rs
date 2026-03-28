use futures::AsyncBufReadExt;
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::{api::LogParams, Api};
use tokio::sync::mpsc;
use tracing::warn;

use crate::{CoreError, KubeClient};

/// Streams log lines for a pod container over an mpsc channel.
pub struct LogStream;

impl LogStream {
    /// Spawn a log-follow task for `pod_name` / `container` in `namespace`.
    ///
    /// Each successfully read line is sent as `Ok(String)`.
    /// On error, `Err(CoreError)` is sent and the task exits.
    /// The task also exits when `tx` is dropped or the log stream ends naturally.
    pub fn start(
        kube_client: KubeClient,
        namespace: impl Into<String>,
        pod_name: impl Into<String>,
        container: impl Into<String>,
        tx: mpsc::Sender<Result<String, CoreError>>,
    ) -> tokio::task::JoinHandle<()> {
        let namespace = namespace.into();
        let pod_name = pod_name.into();
        let container = container.into();

        tokio::spawn(async move {
            let api: Api<Pod> = Api::namespaced(kube_client.client, &namespace);
            let params = LogParams {
                follow: true,
                container: Some(container),
                ..Default::default()
            };

            let reader = match api.log_stream(&pod_name, &params).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Err(CoreError::Kube(e))).await;
                    return;
                }
            };

            let mut lines = reader.lines();
            loop {
                match lines.try_next().await {
                    Ok(Some(line)) => {
                        if tx.send(Ok(line)).await.is_err() {
                            break; // receiver dropped
                        }
                    }
                    Ok(None) => break, // stream ended
                    Err(e) => {
                        warn!("log stream error for {pod_name}: {e}");
                        let _ = tx.send(Err(CoreError::Io(e))).await;
                        break;
                    }
                }
            }
        })
    }
}
