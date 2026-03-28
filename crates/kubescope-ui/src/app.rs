use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use gpui::*;
use gpui_component::{
    TitleBar,
    dock::{DockArea, DockItem, DockPlacement},
    label::Label,
    select::{Select, SelectEvent, SelectState},
};
use kubescope_core::{KubeClient, models::PodSummary, watchers::NamespaceWatcher};
use tracing::{error, info};

use crate::{
    components::{
        log_viewer::LogViewerPanel,
        pod_detail::PodDetailPanel,
        pod_list::{PodListPanel, PodSelected},
    },
    kube_runtime,
};

const DOCK_ID: &str = "kubescope-dock";
const DOCK_VERSION: usize = 1;
const POLL_INTERVAL_MS: u64 = 100;

// ── Event queue shared between the tokio kube tasks and the GPUI poll loop ──

enum KubeEvent {
    Connected(KubeClient),
    Namespace(String),
    PodList(Vec<PodSummary>),
    PodDetail(kubescope_core::models::PodDetail),
    PodDetailError(String),
    Error(String),
}

type EventQueue = Arc<Mutex<VecDeque<KubeEvent>>>;

// ── Workspace ─────────────────────────────────────────────────────────────────

/// Root view: title bar + dock area.
pub struct Workspace {
    dock_area: Entity<DockArea>,
    context_select: Entity<SelectState<Vec<SharedString>>>,
    ns_select: Entity<SelectState<Vec<SharedString>>>,
    pod_list_panel: Entity<PodListPanel>,
    pod_detail_panel: Entity<PodDetailPanel>,
    /// Full unfiltered pod list from the watcher.
    all_pods: Vec<PodSummary>,
    /// Sorted list of namespace names seen from the current cluster.
    namespaces: Vec<SharedString>,
    active_namespace: SharedString,
    active_context: Option<SharedString>,
    kube_client: Option<KubeClient>,
    /// Events posted by tokio tasks, drained by the GPUI poll loop.
    events: EventQueue,
    /// Set to true to signal the running watcher/receiver to stop.
    abort_flag: Arc<AtomicBool>,
    /// GPUI task that polls `events` on a timer; kept alive by storing it.
    _poll_task: Task<()>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // ── Load kubeconfig contexts synchronously ────────────────────────────
        let (contexts, current_ctx) = match KubeClient::list_contexts() {
            Ok(names) => {
                let current = KubeClient::current_context().ok().flatten();
                (
                    names.into_iter().map(SharedString::from).collect::<Vec<_>>(),
                    current.map(SharedString::from),
                )
            }
            Err(e) => {
                error!("failed to read kubeconfig: {e}");
                (Vec::new(), None)
            }
        };

        let initial_ctx_ix = current_ctx.as_ref().and_then(|c| {
            contexts
                .iter()
                .position(|x| x == c)
                .map(|i| gpui_component::IndexPath::default().row(i))
        });

        let context_select = cx.new(|cx| {
            SelectState::new(contexts, initial_ctx_ix, window, cx)
        });

        let ns_select = cx.new(|cx| {
            SelectState::new(
                vec![SharedString::from("All")],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            )
        });

        // ── Build dock layout ─────────────────────────────────────────────────
        let dock_area = cx.new(|cx| DockArea::new(DOCK_ID, Some(DOCK_VERSION), window, cx));
        let weak_dock = dock_area.downgrade();

        let pod_list_panel = cx.new(|cx| PodListPanel::new(window, cx));
        let pod_detail_panel = cx.new(|cx| PodDetailPanel::new(cx));
        let log_panel = cx.new(|cx| LogViewerPanel::new(cx));

        let center = DockItem::tab(pod_list_panel.clone(), &weak_dock, window, cx);
        let right_panel = DockItem::tab(pod_detail_panel.clone(), &weak_dock, window, cx);
        let bottom_panel = DockItem::tab(log_panel, &weak_dock, window, cx);

        dock_area.update(cx, |dock, cx| {
            dock.set_center(center, window, cx);
            dock.set_right_dock(right_panel, Some(px(340.)), false, window, cx);
            dock.set_bottom_dock(bottom_panel, Some(px(220.)), false, window, cx);
        });

        let events: EventQueue = Arc::new(Mutex::new(VecDeque::new()));
        let abort_flag = Arc::new(AtomicBool::new(false));

        // ── Subscribe to context selection ────────────────────────────────────
        cx.subscribe_in(
            &context_select,
            window,
            |this, _, event: &SelectEvent<Vec<SharedString>>, window, cx| {
                if let SelectEvent::Confirm(Some(ctx)) = event {
                    this.switch_context(ctx.to_string(), window, cx);
                }
            },
        )
        .detach();

        // ── Subscribe to namespace selection ──────────────────────────────────
        cx.subscribe_in(
            &ns_select,
            window,
            |this, _, event: &SelectEvent<Vec<SharedString>>, _window, cx| {
                if let SelectEvent::Confirm(Some(ns)) = event {
                    this.active_namespace = ns.clone();
                    this.apply_namespace_filter(cx);
                }
            },
        )
        .detach();

        // ── Subscribe to pod row selection ───────────────────────────────────
        cx.subscribe_in(
            &pod_list_panel,
            window,
            |this, _, event: &PodSelected, _window, cx| {
                this.on_pod_selected(&event.name, &event.namespace, cx);
            },
        )
        .detach();

        // ── Start GPUI poll loop ──────────────────────────────────────────────
        let poll_task = Self::start_poll_loop(events.clone(), window, cx);

        let mut ws = Self {
            dock_area,
            context_select,
            ns_select,
            pod_list_panel,
            pod_detail_panel,
            all_pods: Vec::new(),
            namespaces: Vec::new(),
            active_namespace: SharedString::from("All"),
            active_context: current_ctx.clone(),
            kube_client: None,
            events,
            abort_flag,
            _poll_task: poll_task,
        };

        // Connect to the current context
        if let Some(ctx) = current_ctx {
            ws.switch_context(ctx.to_string(), window, cx);
        }

        ws
    }

    // ── Poll loop ──────────────────────────────────────────────────────────────

    /// Spawn a GPUI task that drains [`KubeEvent`]s posted by tokio tasks.
    fn start_poll_loop(
        events: EventQueue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |weak_ws, cx| loop {
            executor.timer(Duration::from_millis(POLL_INTERVAL_MS)).await;

            let batch: Vec<KubeEvent> = {
                let mut q = events.lock().unwrap();
                q.drain(..).collect()
            };
            if batch.is_empty() {
                continue;
            }
            let keep_going = weak_ws
                .update_in(cx, |this, window, cx| {
                    for ev in batch {
                        this.handle_kube_event(ev, window, cx);
                    }
                })
                .is_ok();
            if !keep_going {
                break;
            }
        })
    }

    fn handle_kube_event(
        &mut self,
        event: KubeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            KubeEvent::Connected(client) => {
                info!("connected to context: {}", client.context);
                self.kube_client = Some(client);
                cx.notify();
            }
            KubeEvent::Namespace(ns) => {
                let name = SharedString::from(ns);
                if !self.namespaces.contains(&name) {
                    self.namespaces.push(name);
                    self.namespaces.sort();
                    let items = self.ns_items();
                    self.ns_select.update(cx, |state, cx| {
                        state.set_items(items, window, cx);
                    });
                    cx.notify();
                }
            }
            KubeEvent::PodList(pods) => {
                self.all_pods = pods;
                self.apply_namespace_filter(cx);
            }
            KubeEvent::PodDetail(detail) => {
                self.pod_detail_panel.update(cx, |panel, cx| {
                    panel.set_detail(detail);
                    cx.notify();
                });
                if !self.dock_area.read(cx).is_dock_open(DockPlacement::Right, cx) {
                    self.dock_area.update(cx, |dock, cx| {
                        dock.toggle_dock(DockPlacement::Right, window, cx);
                    });
                }
            }
            KubeEvent::PodDetailError(msg) => {
                error!("pod detail fetch error: {msg}");
            }
            KubeEvent::Error(msg) => {
                error!("kube error: {msg}");
            }
        }
    }

    // ── Context switching ──────────────────────────────────────────────────────

    fn switch_context(&mut self, context: String, window: &mut Window, cx: &mut Context<Self>) {
        info!("switching to context: {context}");

        // Signal the previous watcher/receiver task to stop.
        self.abort_flag.store(true, Ordering::SeqCst);
        let abort_flag = Arc::new(AtomicBool::new(false));
        self.abort_flag = abort_flag.clone();

        // Reset namespace and pod state.
        self.all_pods.clear();
        self.namespaces.clear();
        self.active_namespace = SharedString::from("All");
        self.active_context = Some(SharedString::from(context.clone()));
        self.kube_client = None;
        self.pod_list_panel.update(cx, |panel, cx| {
            panel.table.update(cx, |table, _| {
                table.delegate_mut().pods.clear();
            });
        });
        self.pod_detail_panel.update(cx, |panel, cx| {
            panel.clear_detail();
            cx.notify();
        });

        let items = self.ns_items();
        self.ns_select.update(cx, |state, cx| {
            state.set_items(items, window, cx);
        });

        // Submit kube work to the dedicated tokio runtime.
        let events = self.events.clone();
        kube_runtime::handle().spawn(async move {
            match KubeClient::for_context(&context).await {
                Ok(client) => {
                    events
                        .lock()
                        .unwrap()
                        .push_back(KubeEvent::Connected(client.clone()));

                    // Start pod watcher.
                    let pod_events = events.clone();
                    let pod_abort = abort_flag.clone();
                    let pod_client = client.clone();
                    tokio::spawn(async move {
                        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                        tokio::spawn(async move {
                            if let Err(e) =
                                kubescope_core::watchers::pod_watcher(pod_client.client, None, tx)
                                    .await
                            {
                                tracing::error!("pod watcher error: {e}");
                            }
                        });
                        while let Some(pods) = rx.recv().await {
                            if pod_abort.load(Ordering::SeqCst) {
                                break;
                            }
                            pod_events
                                .lock()
                                .unwrap()
                                .push_back(KubeEvent::PodList(pods));
                        }
                    });

                    // Start namespace watcher.
                    let (ns_tx, mut ns_rx) = tokio::sync::mpsc::channel::<String>(64);
                    let _watcher = NamespaceWatcher::start(client, ns_tx);

                    while let Some(ns) = ns_rx.recv().await {
                        if abort_flag.load(Ordering::SeqCst) {
                            break;
                        }
                        events
                            .lock()
                            .unwrap()
                            .push_back(KubeEvent::Namespace(ns));
                    }
                }
                Err(e) => {
                    events
                        .lock()
                        .unwrap()
                        .push_back(KubeEvent::Error(e.to_string()));
                }
            }
        });
    }

    /// Fetch pod detail + events asynchronously when a pod row is clicked.
    fn on_pod_selected(&mut self, name: &str, namespace: &str, _cx: &mut Context<Self>) {
        let Some(client) = self.kube_client.clone() else { return };
        let events = self.events.clone();
        let name = name.to_string();
        let namespace = namespace.to_string();

        kube_runtime::handle().spawn(async move {
            match client.fetch_pod_detail(&namespace, &name).await {
                Ok(mut detail) => {
                    if let Ok(pod_events) = client.fetch_pod_events(&namespace, &name).await {
                        detail.events = pod_events;
                    }
                    events.lock().unwrap().push_back(KubeEvent::PodDetail(detail));
                }
                Err(e) => {
                    events
                        .lock()
                        .unwrap()
                        .push_back(KubeEvent::PodDetailError(e.to_string()));
                }
            }
        });
    }

    /// Push the namespace-filtered pod list to the table.
    fn apply_namespace_filter(&mut self, cx: &mut Context<Self>) {
        let filtered: Vec<PodSummary> = if self.active_namespace.as_ref() == "All" {
            self.all_pods.clone()
        } else {
            self.all_pods
                .iter()
                .filter(|p| p.namespace.as_str() == self.active_namespace.as_ref())
                .cloned()
                .collect()
        };
        self.pod_list_panel.update(cx, |panel, cx| {
            panel.table.update(cx, |table, _| {
                table.delegate_mut().pods = filtered;
            });
        });
    }

    /// Build the namespace select items: "All" sentinel + sorted namespace names.
    fn ns_items(&self) -> Vec<SharedString> {
        let mut items = vec![SharedString::from("All")];
        items.extend(self.namespaces.iter().cloned());
        items
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(
                TitleBar::new()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .pl_2()
                            .child(Label::new("KubeScope").text_sm()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                Select::new(&self.context_select)
                                    .placeholder("Select context")
                                    .menu_width(gpui::rems(14.)),
                            )
                            .child(
                                Select::new(&self.ns_select)
                                    .placeholder("Namespace")
                                    .menu_width(gpui::rems(10.)),
                            ),
                    ),
            )
            .child(self.dock_area.clone())
            .children(gpui_component::Root::render_sheet_layer(_window, cx))
            .children(gpui_component::Root::render_dialog_layer(_window, cx))
            .children(gpui_component::Root::render_notification_layer(_window, cx))
    }
}
