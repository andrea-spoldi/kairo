use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    TitleBar,
    dock::{DockArea, DockItem, DockPlacement, PanelView},
    h_flex,
    label::Label,
    select::{Select, SelectEvent, SelectState},
};
use kairo_core::{
    ClusterEvent, ConfigMapSummary, DeploymentSummary, KubeClient, NodeSummary, ServiceSummary,
    models::PodSummary,
    watchers::{ClusterEventWatcher, ConfigMapWatcher, DeploymentWatcher, NamespaceWatcher, NodeWatcher, ServiceWatcher},
};
use tracing::{error, info};

use kairo_core::logs::LogStream;

use kairo_config::KairoConfig;

use crate::{
    actions::{OpenCommandPalette, OpenSettings},
    ai_client,
    analyze::AnalyzeEventRequest,
    mcp_client::{McpClient, McpTool},
    components::{
        ai_panel::{AiPanel, AiSendMessage},
        cluster_health::{ClusterHealthPanel, SidebarNamespaceSelected},
        command_palette::{CommandPalette, PaletteAction},
        event_feed::EventFeedPanel,
        log_viewer::{ContainerSelected, LogViewerPanel},
        pod_detail::{DetailPanel, ResourceDetail},
        pod_list::{PodListPanel, PodSelected},
        resource_list::{
            ConfigMapListPanel, DeploymentListPanel, NodeListPanel, ResourceSelected,
            ServiceListPanel,
        },
        settings_panel::{McpTestRequest, SettingsPanel, SettingsSaved},
        yaml_viewer::{yaml_title, YamlViewerPanel},
    },
    kube_runtime,
    theme::{
        BORDER, HOVER_BG, STATUS_FAILED, STATUS_PENDING, STATUS_RUNNING, SURFACE,
        TEXT_MUTED, TEXT_SECONDARY,
    },
};

const DOCK_ID: &str = "kairo-dock";
const DOCK_VERSION: usize = 5;
const POLL_INTERVAL_MS: u64 = 100;

// ── Event queue shared between the tokio kube tasks and the GPUI poll loop ──

enum KubeEvent {
    Connected(KubeClient),
    Namespace(String),
    PodList(Vec<PodSummary>),
    DeploymentList(Vec<DeploymentSummary>),
    ServiceList(Vec<ServiceSummary>),
    ConfigMapList(Vec<ConfigMapSummary>),
    NodeList(Vec<NodeSummary>),
    PodDetail(kairo_core::models::PodDetail),
    PodDetailError(String),
    /// YAML fetched for a resource — (panel title, yaml string).
    ResourceYaml(String, String),
    ResourceYamlError(String),
    LogLine(String),
    LogError(String),
    WarningEvent(ClusterEvent),
    Error(String),
    /// Incremental AI token from the streaming response.
    AiToken(String),
    AiDone,
    AiError(String),
    /// MCP server connected and tool list fetched.
    McpReady(Arc<McpClient>, Vec<McpTool>),
    /// The AI agent is about to call a named MCP tool.
    AiToolCallStart(String),
    /// Result of a manual "Test Connection" from the Settings panel.
    McpTestResult(String, Vec<String>),
}

type EventQueue = Arc<Mutex<VecDeque<KubeEvent>>>;

// ── Workspace ─────────────────────────────────────────────────────────────────

/// Root view: title bar + dock area + status bar.
pub struct Workspace {
    dock_area: Entity<DockArea>,
    context_select: Entity<SelectState<Vec<SharedString>>>,
    ns_select: Entity<SelectState<Vec<SharedString>>>,
    health_panel: Entity<ClusterHealthPanel>,
    event_feed: Entity<EventFeedPanel>,
    pod_list_panel: Entity<PodListPanel>,
    deployment_panel: Entity<DeploymentListPanel>,
    service_panel: Entity<ServiceListPanel>,
    configmap_panel: Entity<ConfigMapListPanel>,
    node_panel: Entity<NodeListPanel>,
    detail_panel: Entity<DetailPanel>,
    yaml_panel: Entity<YamlViewerPanel>,
    log_panel: Entity<LogViewerPanel>,
    palette: Entity<CommandPalette>,
    settings_panel: Entity<SettingsPanel>,
    ai_panel: Entity<AiPanel>,
    /// Current application config (updated on every save).
    config: KairoConfig,
    /// Full unfiltered pod list from the watcher.
    all_pods: Vec<PodSummary>,
    /// Full unfiltered lists for namespaced resources.
    all_deployments: Vec<DeploymentSummary>,
    all_services: Vec<ServiceSummary>,
    all_configmaps: Vec<ConfigMapSummary>,
    all_nodes: Vec<NodeSummary>,
    /// Context names loaded from kubeconfig.
    contexts: Vec<SharedString>,
    /// Sorted list of namespace names seen from the current cluster.
    namespaces: Vec<SharedString>,
    active_namespace: SharedString,
    active_context: Option<SharedString>,
    kube_client: Option<KubeClient>,
    /// Connected MCP client (None when MCP is disabled or not yet connected).
    mcp_client: Option<Arc<McpClient>>,
    /// Tools fetched from the MCP server at connection time.
    mcp_tools: Vec<McpTool>,
    /// Events posted by tokio tasks, drained by the GPUI poll loop.
    events: EventQueue,
    /// Set to true to signal the running watcher/receiver to stop.
    abort_flag: Arc<AtomicBool>,
    /// Set to true to abort the active log stream.
    log_abort: Arc<AtomicBool>,
    /// Pod name currently streamed in the log panel.
    active_log_pod: Option<String>,
    /// Namespace of the pod currently streamed in the log panel.
    active_log_ns: Option<String>,
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
            SelectState::new(contexts.clone(), initial_ctx_ix, window, cx)
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

        let health_panel = cx.new(|cx| ClusterHealthPanel::new(cx));
        let event_feed = cx.new(|cx| EventFeedPanel::new(cx));
        let pod_list_panel = cx.new(|cx| PodListPanel::new(window, cx));
        let deployment_panel = cx.new(|cx| DeploymentListPanel::new(cx));
        let service_panel = cx.new(|cx| ServiceListPanel::new(cx));
        let configmap_panel = cx.new(|cx| ConfigMapListPanel::new(cx));
        let node_panel = cx.new(|cx| NodeListPanel::new(cx));
        let detail_panel = cx.new(|cx| DetailPanel::new(cx));
        let yaml_panel = cx.new(|cx| YamlViewerPanel::new(cx));
        let log_panel = cx.new(|cx| LogViewerPanel::new(cx));
        let ai_panel = cx.new(|cx| AiPanel::new(window, cx));

        let left_panel = DockItem::tab(health_panel.clone(), &weak_dock, window, cx);
        // Center dock: Pods | Deployments | Services | ConfigMaps | Nodes | Events
        let center = DockItem::tabs(
            vec![
                Arc::new(pod_list_panel.clone())    as Arc<dyn PanelView>,
                Arc::new(deployment_panel.clone())  as Arc<dyn PanelView>,
                Arc::new(service_panel.clone())     as Arc<dyn PanelView>,
                Arc::new(configmap_panel.clone())   as Arc<dyn PanelView>,
                Arc::new(node_panel.clone())        as Arc<dyn PanelView>,
                Arc::new(event_feed.clone())        as Arc<dyn PanelView>,
            ],
            &weak_dock,
            window,
            cx,
        );
        // Bottom dock: Details | YAML | Logs — all detail views together.
        // Right dock is reserved for the AI panel (Phase 15).
        let bottom = DockItem::tabs(
            vec![
                Arc::new(detail_panel.clone()) as Arc<dyn PanelView>,
                Arc::new(yaml_panel.clone())   as Arc<dyn PanelView>,
                Arc::new(log_panel.clone())    as Arc<dyn PanelView>,
            ],
            &weak_dock,
            window,
            cx,
        );

        // Right dock: AI Agent panel (hidden by default; opens on first use).
        let right = DockItem::tab(ai_panel.clone(), &weak_dock, window, cx);

        dock_area.update(cx, |dock, cx| {
            dock.set_left_dock(left_panel, Some(px(220.)), true, window, cx);
            dock.set_center(center, window, cx);
            dock.set_bottom_dock(bottom, Some(px(280.)), false, window, cx);
            dock.set_right_dock(right, Some(px(360.)), false, window, cx);
        });

        let events: EventQueue = Arc::new(Mutex::new(VecDeque::new()));
        let abort_flag = Arc::new(AtomicBool::new(false));
        let log_abort = Arc::new(AtomicBool::new(false));

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
                    let ns_val = this.active_namespace.clone();
                    this.health_panel.update(cx, |panel, cx| {
                        panel.set_active_namespace(ns_val, cx);
                    });
                }
            },
        )
        .detach();

        // ── Subscribe to sidebar namespace click ──────────────────────────────
        cx.subscribe_in(
            &health_panel,
            window,
            |this, _, event: &SidebarNamespaceSelected, _window, cx| {
                this.active_namespace = match &event.namespace {
                    Some(ns) => SharedString::from(ns.clone()),
                    None => SharedString::from("All"),
                };
                this.apply_namespace_filter(cx);
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

        // ── Subscribe to resource row selection (Details + YAML viewer) ──────
        cx.subscribe_in(
            &deployment_panel,
            window,
            |this, _, ev: &ResourceSelected, window, cx| {
                this.fetch_resource_yaml("Deployment", &ev.namespace, &ev.name);
                let ctx = format!("Deployment {}/{}", ev.namespace, ev.name);
                this.ai_panel.update(cx, |p, _| p.set_context(ctx));
                if let Some(d) = this.all_deployments.iter().find(|d| d.name == ev.name && d.namespace == ev.namespace).cloned() {
                    this.show_detail(ResourceDetail::Deployment(d), window, cx);
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &service_panel,
            window,
            |this, _, ev: &ResourceSelected, window, cx| {
                this.fetch_resource_yaml("Service", &ev.namespace, &ev.name);
                let ctx = format!("Service {}/{}", ev.namespace, ev.name);
                this.ai_panel.update(cx, |p, _| p.set_context(ctx));
                if let Some(s) = this.all_services.iter().find(|s| s.name == ev.name && s.namespace == ev.namespace).cloned() {
                    this.show_detail(ResourceDetail::Service(s), window, cx);
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &configmap_panel,
            window,
            |this, _, ev: &ResourceSelected, window, cx| {
                this.fetch_resource_yaml("ConfigMap", &ev.namespace, &ev.name);
                let ctx = format!("ConfigMap {}/{}", ev.namespace, ev.name);
                this.ai_panel.update(cx, |p, _| p.set_context(ctx));
                if let Some(c) = this.all_configmaps.iter().find(|c| c.name == ev.name && c.namespace == ev.namespace).cloned() {
                    this.show_detail(ResourceDetail::ConfigMap(c), window, cx);
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &node_panel,
            window,
            |this, _, ev: &ResourceSelected, window, cx| {
                this.fetch_resource_yaml("Node", "", &ev.name);
                let ctx = format!("Node {}", ev.name);
                this.ai_panel.update(cx, |p, _| p.set_context(ctx));
                if let Some(n) = this.all_nodes.iter().find(|n| n.name == ev.name).cloned() {
                    this.show_detail(ResourceDetail::Node(n), window, cx);
                }
            },
        )
        .detach();

        // ── Build settings panel ──────────────────────────────────────────────
        let config = KairoConfig::load().unwrap_or_default();
        let settings_panel = cx.new(|cx| SettingsPanel::new(window, cx));

        // ── Build command palette ─────────────────────────────────────────────
        let palette = cx.new(|cx| CommandPalette::new(window, cx));

        // Forward palette actions back to Workspace.
        cx.subscribe_in(
            &palette,
            window,
            |this, _, action: &PaletteAction, window, cx| match action.clone() {
                PaletteAction::SelectPod { name, namespace } => {
                    this.on_pod_selected(&name, &namespace, cx);
                    let (n, ns) = (name.clone(), namespace.clone());
                    this.pod_list_panel.update(cx, |panel, cx| {
                        panel.scroll_to_pod(&n, &ns, cx);
                    });
                }
                PaletteAction::SwitchContext(ctx) => {
                    this.switch_context(ctx, window, cx);
                }
                PaletteAction::SwitchNamespace(ns) => {
                    this.active_namespace = SharedString::from(ns.clone());
                    this.apply_namespace_filter(cx);
                    let ns_val = this.active_namespace.clone();
                    this.health_panel.update(cx, |panel, cx| {
                        panel.set_active_namespace(ns_val, cx);
                    });
                }
            },
        )
        .detach();

        // ── Subscribe to settings saved ───────────────────────────────────────
        cx.subscribe_in(
            &settings_panel,
            window,
            |this, _, event: &SettingsSaved, _window, _cx| {
                this.config = event.0.clone();
                this.init_mcp();
            },
        )
        .detach();

        // ── Subscribe to MCP test connection requests ─────────────────────────
        cx.subscribe_in(
            &settings_panel,
            window,
            |this, _, event: &McpTestRequest, _window, _cx| {
                this.handle_mcp_test(event.0.clone());
            },
        )
        .detach();

        // ── Subscribe to AI send message ──────────────────────────────────────
        cx.subscribe_in(
            &ai_panel,
            window,
            |this, _, event: &AiSendMessage, window, cx| {
                this.handle_ai_send(event.0.clone(), window, cx);
            },
        )
        .detach();

        // ── Subscribe to "Analyze" buttons in event cards ─────────────────────
        cx.subscribe_in(
            &event_feed,
            window,
            |this, _, event: &AnalyzeEventRequest, window, cx| {
                this.handle_analyze_event(event.0.clone(), window, cx);
            },
        )
        .detach();

        cx.subscribe_in(
            &detail_panel,
            window,
            |this, _, event: &AnalyzeEventRequest, window, cx| {
                this.handle_analyze_event(event.0.clone(), window, cx);
            },
        )
        .detach();

        // ── Subscribe to container tab selection in log panel ─────────────────
        cx.subscribe_in(
            &log_panel,
            window,
            |this, _, event: &ContainerSelected, _window, _cx| {
                let Some(client) = this.kube_client.clone() else { return };
                // Determine the current pod from the pod label stored in the panel.
                // We re-use the info cached on on_pod_selected call.
                let pod = this.active_log_pod.clone();
                let ns = this.active_log_ns.clone();
                if let (Some(pod), Some(ns)) = (pod, ns) {
                    this.start_log_stream(client, &ns, &pod, &event.name);
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
            health_panel,
            event_feed,
            pod_list_panel,
            deployment_panel,
            service_panel,
            configmap_panel,
            node_panel,
            detail_panel,
            yaml_panel,
            log_panel,
            palette,
            settings_panel,
            ai_panel,
            config,
            all_pods: Vec::new(),
            all_deployments: Vec::new(),
            all_services: Vec::new(),
            all_configmaps: Vec::new(),
            all_nodes: Vec::new(),
            contexts: contexts.clone(),
            namespaces: Vec::new(),
            active_namespace: SharedString::from("All"),
            active_context: current_ctx.clone(),
            kube_client: None,
            mcp_client: None,
            mcp_tools: Vec::new(),
            events,
            abort_flag,
            log_abort,
            active_log_pod: None,
            active_log_ns: None,
            _poll_task: poll_task,
        };

        // Connect to the current context
        if let Some(ctx) = current_ctx {
            ws.switch_context(ctx.to_string(), window, cx);
        }

        // Attempt MCP connection if enabled in config.
        ws.init_mcp();

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
                    let ns_list = self.namespaces.clone();
                    self.palette.update(cx, |p, cx| p.set_namespaces(&ns_list, cx));
                    cx.notify();
                }
            }
            KubeEvent::PodList(pods) => {
                self.all_pods = pods;
                self.apply_namespace_filter(cx);
                let pods_ref = self.all_pods.clone();
                self.health_panel.update(cx, |panel, cx| {
                    panel.update_pods(&pods_ref, cx);
                });
                // Keep palette index up to date.
                self.palette.update(cx, |p, cx| p.set_pods(&pods_ref, cx));
            }
            KubeEvent::DeploymentList(items) => {
                self.health_panel.update(cx, |p, _| p.resource_counts.deployments = items.len());
                self.health_panel.update(cx, |_, cx| cx.notify());
                self.all_deployments = items;
                self.apply_namespace_filter(cx);
            }
            KubeEvent::ServiceList(items) => {
                self.health_panel.update(cx, |p, _| p.resource_counts.services = items.len());
                self.health_panel.update(cx, |_, cx| cx.notify());
                self.all_services = items;
                self.apply_namespace_filter(cx);
            }
            KubeEvent::ConfigMapList(items) => {
                self.health_panel.update(cx, |p, _| p.resource_counts.configmaps = items.len());
                self.health_panel.update(cx, |_, cx| cx.notify());
                self.all_configmaps = items;
                self.apply_namespace_filter(cx);
            }
            KubeEvent::NodeList(items) => {
                let count = items.len();
                self.all_nodes = items.clone();
                self.node_panel.update(cx, |p, cx| p.set_items(items, cx));
                self.health_panel.update(cx, |p, _| p.resource_counts.nodes = count);
                self.health_panel.update(cx, |_, cx| cx.notify());
            }
            KubeEvent::PodDetail(detail) => {
                // Collect container names before moving detail.
                let containers: Vec<String> =
                    detail.containers.iter().map(|c| c.name.clone()).collect();
                let pod_name = detail.summary.name.clone();
                let namespace = detail.summary.namespace.clone();

                self.show_detail(ResourceDetail::Pod(detail), window, cx);

                // Start log streaming for first container.
                if let Some(first_container) = containers.first().cloned() {
                    self.active_log_pod = Some(pod_name.clone());
                    self.active_log_ns = Some(namespace.clone());
                    self.log_panel.update(cx, |panel, cx| {
                        panel.set_pod(pod_name.clone(), namespace.clone(), containers, cx);
                    });
                    if let Some(client) = self.kube_client.clone() {
                        self.start_log_stream(client, &namespace, &pod_name, &first_container);
                    }
                }
            }
            KubeEvent::PodDetailError(msg) => {
                error!("pod detail fetch error: {msg}");
            }
            KubeEvent::ResourceYaml(title, yaml) => {
                self.yaml_panel.update(cx, |panel, cx| {
                    panel.set_yaml(title, yaml);
                    cx.notify();
                });
                self.ensure_bottom_dock_open(window, cx);
            }
            KubeEvent::ResourceYamlError(msg) => {
                error!("yaml fetch error: {msg}");
            }
            KubeEvent::LogLine(line) => {
                self.log_panel.update(cx, |panel, cx| {
                    panel.push_line(line, cx);
                });
            }
            KubeEvent::LogError(msg) => {
                error!("log stream error: {msg}");
            }
            KubeEvent::WarningEvent(ev) => {
                self.health_panel.update(cx, |panel, cx| {
                    panel.push_warning(ev.clone(), cx);
                });
                self.event_feed.update(cx, |feed, cx| {
                    feed.push_event(ev, cx);
                });
            }
            KubeEvent::Error(msg) => {
                error!("kube error: {msg}");
            }
            KubeEvent::AiToken(token) => {
                self.ai_panel.update(cx, |panel, cx| panel.push_token(&token, cx));
            }
            KubeEvent::AiDone => {
                self.ai_panel.update(cx, |panel, cx| panel.finish_streaming(cx));
            }
            KubeEvent::AiError(msg) => {
                error!("ai error: {msg}");
                self.ai_panel.update(cx, |panel, cx| panel.set_error(&msg, cx));
            }
            KubeEvent::McpReady(client, tools) => {
                info!("MCP connected — {} tool(s) available", tools.len());
                self.mcp_client = Some(client);
                self.mcp_tools = tools;
            }
            KubeEvent::AiToolCallStart(name) => {
                self.ai_panel.update(cx, |panel, cx| panel.push_tool_call(&name, cx));
            }
            KubeEvent::McpTestResult(status, tools) => {
                self.settings_panel
                    .update(cx, |p, cx| p.set_mcp_test_result(status, tools, cx));
            }
        }
    }

    // ── Log stream ────────────────────────────────────────────────────────────

    fn start_log_stream(&mut self, client: KubeClient, namespace: &str, pod: &str, container: &str) {
        // Stop any running log stream.
        self.log_abort.store(true, Ordering::SeqCst);
        let log_abort = Arc::new(AtomicBool::new(false));
        self.log_abort = log_abort.clone();

        let events = self.events.clone();
        let namespace = namespace.to_string();
        let pod = pod.to_string();
        let container = container.to_string();

        kube_runtime::handle().spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel(256);
            let _handle = LogStream::start(client, &namespace, &pod, &container, tx);
            while let Some(result) = rx.recv().await {
                if log_abort.load(Ordering::SeqCst) {
                    break;
                }
                match result {
                    Ok(line) => {
                        events.lock().unwrap().push_back(KubeEvent::LogLine(line));
                    }
                    Err(e) => {
                        events
                            .lock()
                            .unwrap()
                            .push_back(KubeEvent::LogError(e.to_string()));
                        break;
                    }
                }
            }
        });
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
        self.all_deployments.clear();
        self.all_services.clear();
        self.all_configmaps.clear();
        self.namespaces.clear();
        self.active_namespace = SharedString::from("All");
        self.active_context = Some(SharedString::from(context.clone()));
        self.kube_client = None;
        self.pod_list_panel.update(cx, |panel, cx| {
            panel.set_pods(vec![], cx);
        });
        self.deployment_panel.update(cx, |p, cx| p.set_items(vec![], cx));
        self.service_panel.update(cx, |p, cx| p.set_items(vec![], cx));
        self.configmap_panel.update(cx, |p, cx| p.set_items(vec![], cx));
        self.node_panel.update(cx, |p, cx| p.set_items(vec![], cx));
        self.detail_panel.update(cx, |panel, cx| {
            panel.clear_detail();
            cx.notify();
        });
        self.all_nodes.clear();
        self.yaml_panel.update(cx, |panel, _| panel.clear());
        // Stop log stream and clear log panel.
        self.log_abort.store(true, Ordering::SeqCst);
        self.log_abort = Arc::new(AtomicBool::new(false));
        self.active_log_pod = None;
        self.active_log_ns = None;
        self.log_panel.update(cx, |panel, cx| {
            panel.clear(cx);
        });
        self.health_panel.update(cx, |panel, cx| {
            panel.clear(cx);
        });
        self.event_feed.update(cx, |feed, cx| {
            feed.clear(cx);
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

                    // Start pod watcher sub-task.
                    let pod_events = events.clone();
                    let pod_abort = abort_flag.clone();
                    let pod_client = client.clone();
                    tokio::spawn(async move {
                        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                        tokio::spawn(async move {
                            if let Err(e) =
                                kairo_core::watchers::pod_watcher(pod_client.client, None, tx)
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

                    // Start namespace watcher sub-task.
                    let ns_events = events.clone();
                    let ns_abort = abort_flag.clone();
                    let ns_client = client.clone();
                    tokio::spawn(async move {
                        let (ns_tx, mut ns_rx) = tokio::sync::mpsc::channel::<String>(64);
                        let _watcher = NamespaceWatcher::start(ns_client, ns_tx);
                        while let Some(ns) = ns_rx.recv().await {
                            if ns_abort.load(Ordering::SeqCst) {
                                break;
                            }
                            ns_events
                                .lock()
                                .unwrap()
                                .push_back(KubeEvent::Namespace(ns));
                        }
                    });

                    // Start cluster event watcher sub-task.
                    let ev_events = events.clone();
                    let ev_abort = abort_flag.clone();
                    let ev_client = client.clone();
                    tokio::spawn(async move {
                        let (ev_tx, mut ev_rx) =
                            tokio::sync::mpsc::channel::<ClusterEvent>(64);
                        let _watcher = ClusterEventWatcher::start(ev_client, ev_tx);
                        while let Some(ev) = ev_rx.recv().await {
                            if ev_abort.load(Ordering::SeqCst) {
                                break;
                            }
                            ev_events
                                .lock()
                                .unwrap()
                                .push_back(KubeEvent::WarningEvent(ev));
                        }
                    });

                    // ── Resource watchers ─────────────────────────────────────

                    macro_rules! spawn_resource_watcher {
                        ($watcher:ident, $variant:ident, $label:literal) => {{
                            let res_events = events.clone();
                            let res_abort = abort_flag.clone();
                            let res_client = client.clone();
                            tokio::spawn(async move {
                                let (tx, mut rx) = tokio::sync::mpsc::channel(64);
                                let _w = $watcher::start(res_client, tx);
                                while let Some(items) = rx.recv().await {
                                    if res_abort.load(Ordering::SeqCst) { break; }
                                    res_events.lock().unwrap().push_back(KubeEvent::$variant(items));
                                }
                            });
                        }};
                    }

                    spawn_resource_watcher!(DeploymentWatcher, DeploymentList, "deployment");
                    spawn_resource_watcher!(ServiceWatcher,    ServiceList,    "service");
                    spawn_resource_watcher!(ConfigMapWatcher,  ConfigMapList,  "configmap");
                    spawn_resource_watcher!(NodeWatcher,       NodeList,       "node");
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
    fn on_pod_selected(&mut self, name: &str, namespace: &str, cx: &mut Context<Self>) {
        self.fetch_resource_yaml("Pod", namespace, name);
        // Update AI context for this resource.
        let ctx = format!("Pod {namespace}/{name}");
        self.ai_panel.update(cx, |p, _| p.set_context(ctx));
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

    /// Fetch raw YAML for a resource and push it to the YAML viewer.
    fn fetch_resource_yaml(&mut self, kind: &str, namespace: &str, name: &str) {
        let Some(client) = self.kube_client.clone() else { return };
        let events = self.events.clone();
        let kind = kind.to_string();
        let namespace = namespace.to_string();
        let name = name.to_string();

        kube_runtime::handle().spawn(async move {
            let title = yaml_title(&kind, Some(namespace.as_str()).filter(|s| !s.is_empty()), &name);
            let result = match kind.as_str() {
                "Deployment" => client.fetch_deployment_yaml(&namespace, &name).await,
                "Service"    => client.fetch_service_yaml(&namespace, &name).await,
                "ConfigMap"  => client.fetch_configmap_yaml(&namespace, &name).await,
                "Node"       => client.fetch_node_yaml(&name).await,
                _            => client.fetch_pod_yaml(&namespace, &name).await,
            };
            match result {
                Ok(yaml) => {
                    events.lock().unwrap().push_back(KubeEvent::ResourceYaml(title, yaml));
                }
                Err(e) => {
                    events.lock().unwrap().push_back(KubeEvent::ResourceYamlError(e.to_string()));
                }
            }
        });
    }

    /// Open the command palette with current pods / contexts / namespaces.
    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pods = self.all_pods.clone();
        let contexts = self.contexts.clone();
        let namespaces = self.namespaces.clone();
        self.palette.update(cx, |p, cx| {
            p.set_pods(&pods, cx);
            p.set_contexts(&contexts, cx);
            p.set_namespaces(&namespaces, cx);
            p.show(window, cx);
        });
    }

    fn ensure_bottom_dock_open(&self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dock_area.read(cx).is_dock_open(DockPlacement::Bottom, cx) {
            self.dock_area.update(cx, |dock, cx| {
                dock.toggle_dock(DockPlacement::Bottom, window, cx);
            });
        }
    }

    // ── AI integration ─────────────────────────────────────────────────────────

    /// Called when the user submits a message in the AI panel.
    fn handle_ai_send(&mut self, user_msg: String, window: &mut Window, cx: &mut Context<Self>) {
        // Push the user message to panel immediately (clears input, shows it).
        self.ai_panel
            .update(cx, |p, cx| p.push_user_message(&user_msg, window, cx));

        // Build full message history (the user message is now included).
        let messages = self.ai_panel.read(cx).build_api_messages();
        let config = self.config.clone();
        let system_prompt = self.build_system_prompt(cx);
        let mcp_client = self.mcp_client.clone();
        let mcp_tools = self.mcp_tools.clone();
        let events = self.events.clone();

        kube_runtime::handle().spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<ai_client::StreamChunk>(128);
            let events_inner = events.clone();

            tokio::spawn(async move {
                if let Err(e) = ai_client::stream_completion(
                    config,
                    system_prompt,
                    messages,
                    mcp_client,
                    mcp_tools,
                    tx,
                )
                .await
                {
                    events_inner
                        .lock()
                        .unwrap()
                        .push_back(KubeEvent::AiError(e.to_string()));
                }
            });

            while let Some(chunk) = rx.recv().await {
                match chunk {
                    ai_client::StreamChunk::Token(t) => {
                        events.lock().unwrap().push_back(KubeEvent::AiToken(t));
                    }
                    ai_client::StreamChunk::ToolCallStart(name) => {
                        events.lock().unwrap().push_back(KubeEvent::AiToolCallStart(name));
                    }
                    ai_client::StreamChunk::Done => {
                        events.lock().unwrap().push_back(KubeEvent::AiDone);
                        break;
                    }
                }
            }
        });

        // Open the right dock if it's closed.
        if !self.dock_area.read(cx).is_dock_open(DockPlacement::Right, cx) {
            self.dock_area.update(cx, |dock, cx| {
                dock.toggle_dock(DockPlacement::Right, window, cx);
            });
        }
    }

    fn show_detail(&mut self, detail: ResourceDetail, window: &mut Window, cx: &mut Context<Self>) {
        self.detail_panel.update(cx, |p, cx| { p.set_detail(detail); cx.notify(); });
        self.ensure_bottom_dock_open(window, cx);
    }

    /// Build the system prompt: SRE base + live cluster context appended.
    fn build_system_prompt(&self, cx: &App) -> String {
        let cluster = self
            .active_context
            .as_ref()
            .map(|s| s.as_ref())
            .unwrap_or("unknown");
        let ns = self.active_namespace.as_ref();
        let (running, total) = self.all_pods.iter().fold((0usize, 0usize), |(r, t), p| {
            (r + (p.status == "Running") as usize, t + 1)
        });
        let mcp_line = if self.config.mcp.enabled {
            format!("\n- MCP server: {}", self.config.mcp.server_url)
        } else {
            String::new()
        };
        let resource_ctx = self.ai_panel.read(cx).context_text().map(str::to_owned);

        let mut prompt = format!(
            "{}\n\nCurrent cluster context:\n\
             - Cluster: {cluster}\n\
             - Namespace filter: {ns}\n\
             - Pods: {running}/{total} running{mcp_line}",
            ai_client::SYSTEM_PROMPT,
        );

        if let Some(ctx) = resource_ctx {
            prompt.push_str(&format!("\n- Selected resource: {ctx}"));
        }

        prompt
    }

    /// Connect to the MCP server if enabled in config.  Clears any previous
    /// client if MCP is disabled.  Called on startup and after every settings save.
    fn init_mcp(&mut self) {
        if !self.config.mcp.enabled || self.config.mcp.server_url.trim().is_empty() {
            self.mcp_client = None;
            self.mcp_tools.clear();
            return;
        }
        let url = self.config.mcp.server_url.clone();
        let queue = self.events.clone();
        kube_runtime::handle().spawn(async move {
            match McpClient::connect(&url).await {
                Ok((client, tools)) => {
                    queue
                        .lock()
                        .unwrap()
                        .push_back(KubeEvent::McpReady(Arc::new(client), tools));
                }
                Err(e) => {
                    tracing::warn!("MCP connect failed ({}): {e}", url);
                }
            }
        });
    }

    /// Run a one-shot MCP connect to test the URL from the Settings panel.
    /// Pushes `KubeEvent::McpTestResult` with the outcome so the panel can display it.
    fn handle_mcp_test(&self, url: String) {
        let queue = self.events.clone();
        kube_runtime::handle().spawn(async move {
            match McpClient::connect(&url).await {
                Ok((_, tools)) => {
                    let names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
                    let status = format!("Connected — {} tool(s)", names.len());
                    queue.lock().unwrap().push_back(KubeEvent::McpTestResult(status, names));
                }
                Err(e) => {
                    queue
                        .lock()
                        .unwrap()
                        .push_back(KubeEvent::McpTestResult(format!("Error: {e}"), vec![]));
                }
            }
        });
    }

    /// Handle an "Analyze" button click: send a structured event analysis prompt to the AI.
    fn handle_analyze_event(&mut self, context_json: String, window: &mut Window, cx: &mut Context<Self>) {
        let user_prompt = ai_client::build_event_analysis_prompt(&context_json);
        // Display a short summary in the chat bubble; send the full prompt to the API.
        let display = context_json
            .lines()
            .find(|l| l.contains("\"reason\""))
            .and_then(|l| l.split('"').nth(3))
            .map(|r| format!("Analyze event: {r}"))
            .unwrap_or_else(|| "Analyze Kubernetes event".to_string());

        self.ai_panel.update(cx, |p, cx| {
            p.push_analysis_message(&display, &user_prompt, window, cx);
        });

        let messages = self.ai_panel.read(cx).build_api_messages();
        let config = self.config.clone();
        let system_prompt = self.build_system_prompt(cx);
        let mcp_client = self.mcp_client.clone();
        let mcp_tools = self.mcp_tools.clone();
        let events = self.events.clone();

        kube_runtime::handle().spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<ai_client::StreamChunk>(128);
            let events_inner = events.clone();

            tokio::spawn(async move {
                if let Err(e) = ai_client::stream_completion(
                    config,
                    system_prompt,
                    messages,
                    mcp_client,
                    mcp_tools,
                    tx,
                )
                .await
                {
                    events_inner
                        .lock()
                        .unwrap()
                        .push_back(KubeEvent::AiError(e.to_string()));
                }
            });

            while let Some(chunk) = rx.recv().await {
                match chunk {
                    ai_client::StreamChunk::Token(t) => {
                        events.lock().unwrap().push_back(KubeEvent::AiToken(t));
                    }
                    ai_client::StreamChunk::ToolCallStart(name) => {
                        events.lock().unwrap().push_back(KubeEvent::AiToolCallStart(name));
                    }
                    ai_client::StreamChunk::Done => {
                        events.lock().unwrap().push_back(KubeEvent::AiDone);
                        break;
                    }
                }
            }
        });

        if !self.dock_area.read(cx).is_dock_open(DockPlacement::Right, cx) {
            self.dock_area.update(cx, |dock, cx| {
                dock.toggle_dock(DockPlacement::Right, window, cx);
            });
        }
    }

    /// Push the namespace-filtered pod list to the panel (which re-applies search filters).
    fn apply_namespace_filter(&mut self, cx: &mut Context<Self>) {
        let ns = self.active_namespace.as_ref();
        let all = ns == "All";

        let pods = if all {
            self.all_pods.clone()
        } else {
            self.all_pods.iter().filter(|p| p.namespace.as_str() == ns).cloned().collect()
        };
        self.pod_list_panel.update(cx, |panel, cx| panel.set_pods(pods, cx));

        let deployments = if all {
            self.all_deployments.clone()
        } else {
            self.all_deployments.iter().filter(|d| d.namespace.as_str() == ns).cloned().collect()
        };
        self.deployment_panel.update(cx, |p, cx| p.set_items(deployments, cx));

        let services = if all {
            self.all_services.clone()
        } else {
            self.all_services.iter().filter(|s| s.namespace.as_str() == ns).cloned().collect()
        };
        self.service_panel.update(cx, |p, cx| p.set_items(services, cx));

        let configmaps = if all {
            self.all_configmaps.clone()
        } else {
            self.all_configmaps.iter().filter(|c| c.namespace.as_str() == ns).cloned().collect()
        };
        self.configmap_panel.update(cx, |p, cx| p.set_items(configmaps, cx));
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
        // Compute pod stats for the status bar.
        let running = self
            .all_pods
            .iter()
            .filter(|p| p.status == "Running")
            .count();
        let pending = self
            .all_pods
            .iter()
            .filter(|p| {
                matches!(
                    p.status.as_str(),
                    "Pending" | "ContainerCreating" | "Initializing"
                )
            })
            .count();
        let failed = self.all_pods.len().saturating_sub(running + pending);
        let total = self.all_pods.len();
        let ctx_name = self
            .active_context
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "no cluster".to_string());
        let ns_name = self.active_namespace.to_string();

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .on_action(cx.listener(|this, _: &OpenCommandPalette, window, cx| {
                this.open_palette(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.settings_panel.update(cx, |panel, cx| panel.show(window, cx));
            }))
            .child(
                TitleBar::new()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .pl_2()
                            .child(Label::new("Kairo").text_sm()),
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
                            )
                            // Gear icon — opens settings panel.
                            .child(
                                div()
                                    .cursor_pointer()
                                    .px_1()
                                    .rounded(px(4.))
                                    .hover(|s| s.bg(HOVER_BG))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.settings_panel.update(cx, |panel, cx| {
                                                panel.show(window, cx);
                                            });
                                        }),
                                    )
                                    .child(Label::new("⚙").text_sm().text_color(TEXT_MUTED)),
                            ),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.dock_area.clone()))
            .child(render_status_bar(&ctx_name, &ns_name, running, pending, failed, total))
            .children(gpui_component::Root::render_sheet_layer(_window, cx))
            .children(gpui_component::Root::render_dialog_layer(_window, cx))
            .children(gpui_component::Root::render_notification_layer(_window, cx))
            // Command palette overlay — rendered last so it sits on top.
            .when(self.palette.read(cx).is_visible(), |d: Div| {
                d.child(self.palette.clone())
            })
            // Settings panel overlay — above command palette.
            .when(self.settings_panel.read(cx).is_visible(), |d: Div| {
                d.child(self.settings_panel.clone())
            })
    }
}

fn render_status_bar(
    context: &str,
    namespace: &str,
    running: usize,
    pending: usize,
    failed: usize,
    total: usize,
) -> impl IntoElement {
    h_flex()
        .h(px(22.))
        .px_3()
        .gap_3()
        .bg(SURFACE)
        .border_t_1()
        .border_color(BORDER)
        .flex_shrink_0()
        .child(Label::new(context.to_string()).text_sm().text_color(TEXT_MUTED))
        .child(Label::new("│").text_sm().text_color(TEXT_MUTED))
        .child(Label::new(namespace.to_string()).text_sm().text_color(TEXT_SECONDARY))
        .child(div().flex_1())
        .child(Label::new(format!("● {running}")).text_sm().text_color(STATUS_RUNNING))
        .child(Label::new(format!("◐ {pending}")).text_sm().text_color(STATUS_PENDING))
        .child(Label::new(format!("✖ {failed}")).text_sm().text_color(STATUS_FAILED))
        .child(Label::new(format!("{total} pods")).text_sm().text_color(TEXT_MUTED))
        .child(Label::new("│").text_sm().text_color(TEXT_MUTED))
        .child(
            Label::new(concat!("v", env!("CARGO_PKG_VERSION")))
                .text_sm()
                .text_color(TEXT_MUTED),
        )
}
