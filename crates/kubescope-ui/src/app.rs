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
    dock::{DockArea, DockItem},
    label::Label,
    select::{Select, SelectEvent, SelectState},
};
use kubescope_core::{KubeClient, watchers::NamespaceWatcher};
use tracing::{error, info};

use crate::{
    components::{
        log_viewer::LogViewerPanel, pod_detail::PodDetailPanel, pod_list::PodListPanel,
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
    Error(String),
}

type EventQueue = Arc<Mutex<VecDeque<KubeEvent>>>;

// ── Workspace ─────────────────────────────────────────────────────────────────

/// Root view: title bar + dock area.
pub struct Workspace {
    dock_area: Entity<DockArea>,
    context_select: Entity<SelectState<Vec<SharedString>>>,
    ns_select: Entity<SelectState<Vec<SharedString>>>,
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

        let pod_list_panel = cx.new(|cx| PodListPanel::new(cx));
        let pod_detail_panel = cx.new(|cx| PodDetailPanel::new(cx));
        let log_panel = cx.new(|cx| LogViewerPanel::new(cx));

        let center = DockItem::tab(pod_list_panel, &weak_dock, window, cx);
        let right_panel = DockItem::tab(pod_detail_panel, &weak_dock, window, cx);
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
            |this, _, event: &SelectEvent<Vec<SharedString>>, _window, _cx| {
                if let SelectEvent::Confirm(Some(ns)) = event {
                    this.active_namespace = ns.clone();
                }
            },
        )
        .detach();

        // ── Start GPUI poll loop ──────────────────────────────────────────────
        let poll_task = Self::start_poll_loop(events.clone(), window, cx);

        let mut ws = Self {
            dock_area,
            context_select,
            ns_select,
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

        // Reset namespace state.
        self.namespaces.clear();
        self.active_namespace = SharedString::from("All");
        self.active_context = Some(SharedString::from(context.clone()));
        self.kube_client = None;

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

                    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);
                    let _watcher = NamespaceWatcher::start(client, tx);

                    while let Some(ns) = rx.recv().await {
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
