use std::sync::mpsc;

use gpui::*;
use gpui_component::table::TableEvent;
use kubescope_core::{client::KubeClient, watchers::pod_watcher};

use crate::components::pod_list::PodList;

/// Root application view.
pub struct AppRoot {
    pod_list: Entity<PodList>,
    _subscriptions: Vec<Subscription>,
    _update_task: Task<()>,
}

impl AppRoot {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pod_list = cx.new(|cx| PodList::new(window, cx));

        // Subscribe to row-selection events (extended in Phase 7 to open the detail panel).
        let table_entity = pod_list.read(cx).table.clone();
        let _subscriptions = vec![cx.subscribe_in(
            &table_entity,
            window,
            |_this, _table, event: &TableEvent, _window, _cx| {
                if let TableEvent::SelectRow(ix) = event {
                    tracing::debug!("pod row selected: {}", ix);
                }
            },
        )];

        // Channel that bridges the tokio watcher thread → GPUI polling task.
        let (std_tx, std_rx) = mpsc::channel::<Vec<kubescope_core::models::PodSummary>>();

        // Dedicated OS thread with its own tokio runtime for the K8s watcher.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(async move {
                let (pod_tx, mut pod_rx) = tokio::sync::mpsc::unbounded_channel();
                match KubeClient::try_default().await {
                    Ok(kc) => {
                        tokio::spawn(async move {
                            if let Err(e) = pod_watcher(kc.client, None, pod_tx).await {
                                tracing::error!("pod watcher error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!("no cluster connection: {}", e);
                        return;
                    }
                }
                while let Some(pods) = pod_rx.recv().await {
                    if std_tx.send(pods).is_err() {
                        break;
                    }
                }
            });
        });

        // GPUI polling task: drain std channel every 100 ms, push into the table delegate.
        let _update_task = cx.spawn(async move |_this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                while let Ok(pods) = std_rx.try_recv() {
                    cx.update(|cx| {
                        table_entity.update(cx, |table, _| {
                            table.delegate_mut().pods = pods;
                        });
                    });
                }
            }
        });

        AppRoot { pod_list, _subscriptions, _update_task }
    }
}

impl Render for AppRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1E1E2E))
            .text_color(rgb(0xCDD6F4))
            .text_sm()
            .child(self.pod_list.clone())
    }
}
