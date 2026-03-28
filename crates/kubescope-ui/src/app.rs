use gpui::*;
use gpui_component::{
    TitleBar,
    dock::{DockArea, DockItem},
    label::Label,
    select::{Select, SelectEvent, SelectState},
};
use kubescope_core::{KubeClient, watchers::NamespaceWatcher};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::components::{
    log_viewer::LogViewerPanel, pod_detail::PodDetailPanel, pod_list::PodListPanel,
};

const DOCK_ID: &str = "kubescope-dock";
const DOCK_VERSION: usize = 1;

/// Root view: title bar + dock area.
pub struct Workspace {
    dock_area: Entity<DockArea>,
    context_select: Entity<SelectState<Vec<SharedString>>>,
    ns_select: Entity<SelectState<Vec<SharedString>>>,
    /// All namespaces seen from the current cluster (excludes the sentinel "All").
    namespaces: Vec<SharedString>,
    active_namespace: SharedString,
    active_context: Option<SharedString>,
    kube_client: Option<KubeClient>,
    /// Running namespace-watcher tokio task; aborting it closes the sender channel.
    _ns_watcher: Option<tokio::task::JoinHandle<()>>,
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
            SelectState::new(contexts.clone(), initial_ctx_ix, window, cx)
        });

        let ns_select = cx.new(|cx| {
            SelectState::new(vec![SharedString::from("All")], Some(gpui_component::IndexPath::default()), window, cx)
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

        // ── Subscribe to context changes ──────────────────────────────────────
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

        // ── Subscribe to namespace changes ────────────────────────────────────
        cx.subscribe_in(
            &ns_select,
            window,
            |this, _, event: &SelectEvent<Vec<SharedString>>, _, _cx| {
                if let SelectEvent::Confirm(Some(ns)) = event {
                    this.active_namespace = ns.clone();
                }
            },
        )
        .detach();

        let mut ws = Self {
            dock_area,
            context_select,
            ns_select,
            namespaces: Vec::new(),
            active_namespace: SharedString::from("All"),
            active_context: current_ctx.clone(),
            kube_client: None,
            _ns_watcher: None,
        };

        // ── Connect to the current context ────────────────────────────────────
        if let Some(ctx) = current_ctx {
            ws.switch_context(ctx.to_string(), window, cx);
        }

        ws
    }

    // ── Context switching ──────────────────────────────────────────────────────

    fn switch_context(&mut self, context: String, window: &mut Window, cx: &mut Context<Self>) {
        info!("switching to context: {context}");

        // Cancel the old watcher (dropping the JoinHandle aborts the tokio task,
        // which drops the mpsc sender, which closes the channel).
        if let Some(handle) = self._ns_watcher.take() {
            handle.abort();
            drop(handle);
        }

        // Reset namespace list to just the sentinel "All".
        self.namespaces.clear();
        self.active_namespace = SharedString::from("All");
        self.active_context = Some(SharedString::from(context.clone()));
        self.kube_client = None;

        let ns_items = self.ns_items();
        self.ns_select.update(cx, |state, cx| {
            state.set_items(ns_items, window, cx);
        });

        // Spawn async task: connect → start watcher → drain namespace channel.
        cx.spawn_in(window, async move |weak_ws, cx| {
            match KubeClient::for_context(&context).await {
                Ok(client) => {
                    let (tx, mut rx) = mpsc::channel::<String>(64);
                    let watcher_handle = NamespaceWatcher::start(client.clone(), tx);

                    let keep_going = weak_ws
                        .update_in(cx, |this, _, cx| {
                            this.kube_client = Some(client);
                            this._ns_watcher = Some(watcher_handle);
                            cx.notify();
                        })
                        .is_ok();

                    if !keep_going {
                        return;
                    }

                    while let Some(ns) = rx.recv().await {
                        let stopped = weak_ws
                            .update_in(cx, |this, window, cx| {
                                let name = SharedString::from(ns);
                                if !this.namespaces.contains(&name) {
                                    this.namespaces.push(name);
                                    this.namespaces.sort();
                                    let items = this.ns_items();
                                    this.ns_select.update(cx, |state, cx| {
                                        state.set_items(items, window, cx);
                                    });
                                    cx.notify();
                                }
                            })
                            .is_err();
                        if stopped {
                            break;
                        }
                    }
                }
                Err(e) => error!("failed to connect to context '{context}': {e}"),
            }
        })
        .detach();
    }

    /// Build the namespace select items: "All" sentinel + sorted namespaces.
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
