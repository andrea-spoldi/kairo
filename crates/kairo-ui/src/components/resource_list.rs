use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kairo_core::models::{
    ConfigMapSummary, DeploymentSummary, NodeSummary, ServiceSummary,
    fmt_cpu, fmt_memory,
};

use crate::theme::{
    BORDER, HOVER_BG, STATUS_FAILED, STATUS_PENDING, STATUS_RUNNING, STATUS_SUCCEEDED,
    SURFACE, TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

// ── Shared selection event ────────────────────────────────────────────────────

/// Emitted when the user clicks a row in any resource list panel.
#[derive(Clone)]
pub struct ResourceSelected {
    pub name: String,
    /// Empty for cluster-scoped resources (Nodes).
    pub namespace: String,
}

// ── Shared table primitives ───────────────────────────────────────────────────

fn header_cell(label: &'static str, width: f32) -> impl IntoElement {
    div()
        .w(px(width))
        .flex_shrink_0()
        .px(px(8.))
        .py(px(4.))
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_MUTED)
        .child(label)
}

fn cell(text: impl Into<SharedString>, width: f32) -> impl IntoElement {
    div()
        .w(px(width))
        .flex_shrink_0()
        .px(px(8.))
        .py(px(6.))
        .text_sm()
        .text_color(TEXT_PRIMARY)
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(text.into())
}

fn muted_cell(text: impl Into<SharedString>, width: f32) -> impl IntoElement {
    div()
        .w(px(width))
        .flex_shrink_0()
        .px(px(8.))
        .py(px(6.))
        .text_sm()
        .text_color(TEXT_SECONDARY)
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(text.into())
}

// ── DeploymentListPanel ───────────────────────────────────────────────────────

pub struct DeploymentListPanel {
    pub items: Vec<DeploymentSummary>,
    focus_handle: FocusHandle,
}

impl DeploymentListPanel {
    pub fn new(cx: &mut App) -> Self {
        Self { items: vec![], focus_handle: cx.focus_handle() }
    }

    pub fn set_items(&mut self, items: Vec<DeploymentSummary>, cx: &mut Context<Self>) {
        self.items = items;
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for DeploymentListPanel {}
impl EventEmitter<ResourceSelected> for DeploymentListPanel {}

impl Focusable for DeploymentListPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Panel for DeploymentListPanel {
    fn panel_name(&self) -> &'static str { "DeploymentListPanel" }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { "Deployments" }
    fn closable(&self, _: &App) -> bool { false }
    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> { None }
}

impl Render for DeploymentListPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.items.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            // Header
            .child(
                h_flex()
                    .bg(SURFACE)
                    .border_b_1()
                    .border_color(BORDER)
                    .child(header_cell("NAMESPACE", 140.))
                    .child(header_cell("NAME", 220.))
                    .child(header_cell("READY", 70.))
                    .child(header_cell("UP-TO-DATE", 100.))
                    .child(header_cell("AVAILABLE", 90.))
                    .child(header_cell("AGE", 70.)),
            )
            // Rows
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex_col()
                            .children(items.into_iter().enumerate().map(|(ix, d)| {
                                let name = d.name.clone();
                                let ns = d.namespace.clone();
                                let listener = cx.listener(move |_this, _: &ClickEvent, _w, cx| {
                                    cx.emit(ResourceSelected { name: name.clone(), namespace: ns.clone() });
                                });
                                h_flex()
                                    .id(("dep-row", ix))
                                    .border_b_1()
                                    .border_color(BORDER)
                                    .cursor_pointer()
                                    .hover(|s| s.bg(HOVER_BG))
                                    .on_click(listener)
                                    .child(muted_cell(d.namespace, 140.))
                                    .child(cell(d.name, 220.))
                                    .child(cell(d.ready, 70.))
                                    .child(muted_cell(d.up_to_date.to_string(), 100.))
                                    .child(muted_cell(d.available.to_string(), 90.))
                                    .child(muted_cell(d.age, 70.))
                            }))
                            .overflow_y_scrollbar(),
                    ),
            )
    }
}

// ── ServiceListPanel ──────────────────────────────────────────────────────────

pub struct ServiceListPanel {
    pub items: Vec<ServiceSummary>,
    focus_handle: FocusHandle,
}

impl ServiceListPanel {
    pub fn new(cx: &mut App) -> Self {
        Self { items: vec![], focus_handle: cx.focus_handle() }
    }

    pub fn set_items(&mut self, items: Vec<ServiceSummary>, cx: &mut Context<Self>) {
        self.items = items;
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for ServiceListPanel {}
impl EventEmitter<ResourceSelected> for ServiceListPanel {}

impl Focusable for ServiceListPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Panel for ServiceListPanel {
    fn panel_name(&self) -> &'static str { "ServiceListPanel" }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { "Services" }
    fn closable(&self, _: &App) -> bool { false }
    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> { None }
}

impl Render for ServiceListPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.items.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                h_flex()
                    .bg(SURFACE)
                    .border_b_1()
                    .border_color(BORDER)
                    .child(header_cell("NAMESPACE", 140.))
                    .child(header_cell("NAME", 200.))
                    .child(header_cell("TYPE", 110.))
                    .child(header_cell("CLUSTER-IP", 120.))
                    .child(header_cell("EXTERNAL-IP", 130.))
                    .child(header_cell("PORTS", 160.))
                    .child(header_cell("AGE", 70.)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex_col()
                            .children(items.into_iter().enumerate().map(|(ix, s)| {
                                let type_color = match s.type_.as_str() {
                                    "LoadBalancer" => STATUS_RUNNING,
                                    "NodePort"     => STATUS_PENDING,
                                    "ExternalName" => STATUS_SUCCEEDED,
                                    _              => TEXT_SECONDARY,
                                };
                                let name = s.name.clone();
                                let ns = s.namespace.clone();
                                let listener = cx.listener(move |_this, _: &ClickEvent, _w, cx| {
                                    cx.emit(ResourceSelected { name: name.clone(), namespace: ns.clone() });
                                });
                                h_flex()
                                    .id(("svc-row", ix))
                                    .border_b_1()
                                    .border_color(BORDER)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(HOVER_BG))
                                    .on_click(listener)
                                    .child(muted_cell(s.namespace, 140.))
                                    .child(cell(s.name, 200.))
                                    .child(
                                        div().w(px(110.)).flex_shrink_0().px(px(8.)).py(px(6.))
                                            .text_sm().text_color(type_color)
                                            .child(s.type_),
                                    )
                                    .child(muted_cell(s.cluster_ip, 120.))
                                    .child(muted_cell(s.external_ip, 130.))
                                    .child(muted_cell(s.ports, 160.))
                                    .child(muted_cell(s.age, 70.))
                            }))
                            .overflow_y_scrollbar(),
                    ),
            )
    }
}

// ── ConfigMapListPanel ────────────────────────────────────────────────────────

pub struct ConfigMapListPanel {
    pub items: Vec<ConfigMapSummary>,
    focus_handle: FocusHandle,
}

impl ConfigMapListPanel {
    pub fn new(cx: &mut App) -> Self {
        Self { items: vec![], focus_handle: cx.focus_handle() }
    }

    pub fn set_items(&mut self, items: Vec<ConfigMapSummary>, cx: &mut Context<Self>) {
        self.items = items;
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for ConfigMapListPanel {}
impl EventEmitter<ResourceSelected> for ConfigMapListPanel {}

impl Focusable for ConfigMapListPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Panel for ConfigMapListPanel {
    fn panel_name(&self) -> &'static str { "ConfigMapListPanel" }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { "ConfigMaps" }
    fn closable(&self, _: &App) -> bool { false }
    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> { None }
}

impl Render for ConfigMapListPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.items.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                h_flex()
                    .bg(SURFACE)
                    .border_b_1()
                    .border_color(BORDER)
                    .child(header_cell("NAMESPACE", 160.))
                    .child(header_cell("NAME", 280.))
                    .child(header_cell("DATA", 70.))
                    .child(header_cell("AGE", 70.)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex_col()
                            .children(items.into_iter().enumerate().map(|(ix, cm)| {
                                let name = cm.name.clone();
                                let ns = cm.namespace.clone();
                                let listener = cx.listener(move |_this, _: &ClickEvent, _w, cx| {
                                    cx.emit(ResourceSelected { name: name.clone(), namespace: ns.clone() });
                                });
                                h_flex()
                                    .id(("cm-row", ix))
                                    .border_b_1()
                                    .border_color(BORDER)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(HOVER_BG))
                                    .on_click(listener)
                                    .child(muted_cell(cm.namespace, 160.))
                                    .child(cell(cm.name, 280.))
                                    .child(muted_cell(cm.data_count.to_string(), 70.))
                                    .child(muted_cell(cm.age, 70.))
                            }))
                            .overflow_y_scrollbar(),
                    ),
            )
    }
}

// ── NodeListPanel ─────────────────────────────────────────────────────────────

pub struct NodeListPanel {
    pub items: Vec<NodeSummary>,
    focus_handle: FocusHandle,
}

impl NodeListPanel {
    pub fn new(cx: &mut App) -> Self {
        Self { items: vec![], focus_handle: cx.focus_handle() }
    }

    pub fn set_items(&mut self, items: Vec<NodeSummary>, cx: &mut Context<Self>) {
        self.items = items;
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for NodeListPanel {}
impl EventEmitter<ResourceSelected> for NodeListPanel {}

impl Focusable for NodeListPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Panel for NodeListPanel {
    fn panel_name(&self) -> &'static str { "NodeListPanel" }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { "Nodes" }
    fn closable(&self, _: &App) -> bool { false }
    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> { None }
}

impl Render for NodeListPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.items.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(8.))
                            .p(px(12.))
                            .children(items.into_iter().enumerate().map(|(ix, n)| {
                                let name = n.name.clone();
                                let listener = cx.listener(move |_this, _: &ClickEvent, _w, cx| {
                                    cx.emit(ResourceSelected {
                                        name: name.clone(),
                                        namespace: String::new(),
                                    });
                                });
                                render_node_card(n, ix, listener)
                            }))
                            .overflow_y_scrollbar(),
                    ),
            )
    }
}

// ── Node card ─────────────────────────────────────────────────────────────────

fn render_node_card(
    n: NodeSummary,
    ix: usize,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let status_color = if n.status == "Ready" { STATUS_RUNNING } else { STATUS_FAILED };
    let status_dot = if n.status == "Ready" { "●" } else { "✖" };

    let cpu_ratio = ratio(n.cpu_allocatable_milli, n.cpu_capacity_milli);
    let cpu_label = format!(
        "{} / {}",
        fmt_cpu(n.cpu_allocatable_milli),
        fmt_cpu(n.cpu_capacity_milli),
    );

    let mem_ratio = ratio(n.memory_allocatable_bytes, n.memory_capacity_bytes);
    let mem_label = format!(
        "{} / {}",
        fmt_memory(n.memory_allocatable_bytes),
        fmt_memory(n.memory_capacity_bytes),
    );

    div()
        .id(("node-card", ix))
        .flex()
        .flex_col()
        .gap(px(8.))
        .p(px(12.))
        .rounded(px(6.))
        .border_1()
        .border_color(BORDER)
        .bg(SURFACE)
        .cursor_pointer()
        .hover(|s| s.border_color(STATUS_RUNNING))
        .on_click(listener)
        // ── Name + status ─────────────────────────────────────────────────────
        .child(
            h_flex()
                .justify_between()
                .child(
                    Label::new(format!("{status_dot} {}", n.name))
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(TEXT_PRIMARY),
                )
                .child(
                    h_flex()
                        .gap(px(8.))
                        .child(
                            Label::new(n.status.clone())
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(status_color),
                        )
                        .child(
                            Label::new(format!("age: {}", n.age))
                                .text_sm()
                                .text_color(TEXT_MUTED),
                        ),
                ),
        )
        // ── Roles / version / OS ──────────────────────────────────────────────
        .child(
            h_flex()
                .gap(px(16.))
                .child(
                    Label::new(format!("roles: {}", n.roles))
                        .text_sm()
                        .text_color(TEXT_SECONDARY),
                )
                .child(
                    Label::new(format!("v{}", n.version))
                        .text_sm()
                        .text_color(TEXT_MUTED),
                )
                .child(
                    Label::new(n.os_image.clone())
                        .text_sm()
                        .text_color(TEXT_MUTED),
                ),
        )
        // ── CPU bar ───────────────────────────────────────────────────────────
        .child(resource_bar_row("CPU", cpu_ratio, &cpu_label, STATUS_RUNNING))
        // ── Memory bar ───────────────────────────────────────────────────────
        .child(resource_bar_row("MEM", mem_ratio, &mem_label, STATUS_PENDING))
}

/// Build a labeled resource allocation bar row.
fn resource_bar_row(
    label: &'static str,
    ratio: f32,
    detail: &str,
    bar_color: Hsla,
) -> impl IntoElement {
    h_flex()
        .gap(px(8.))
        .child(
            Label::new(label)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(TEXT_MUTED)
                .w(px(32.)),
        )
        .child(
            div()
                .flex_1()
                .h(px(6.))
                .rounded_full()
                .bg(BORDER)
                .child(
                    div()
                        .h_full()
                        .w(relative(ratio))
                        .rounded_full()
                        .bg(bar_color),
                ),
        )
        .child(
            Label::new(detail.to_string())
                .text_xs()
                .text_color(TEXT_MUTED),
        )
}

/// Safe ratio: value / total, clamped to [0, 1].  Returns 0 if total == 0.
fn ratio(value: i64, total: i64) -> f32 {
    if total <= 0 { return 0.0; }
    (value as f32 / total as f32).clamp(0.0, 1.0)
}

// ── Resource counts for the sidebar ──────────────────────────────────────────

/// Aggregated resource counts for display in the sidebar tree.
#[derive(Default, Clone)]
pub struct ResourceCounts {
    pub deployments: usize,
    pub services: usize,
    pub configmaps: usize,
    pub nodes: usize,
}

/// Renders the resource tree section used in `ClusterHealthPanel`.
pub fn render_resource_tree(counts: &ResourceCounts) -> impl IntoElement {
    let row = |icon: &'static str, label: &'static str, count: usize| {
        h_flex()
            .px(px(12.))
            .py(px(4.))
            .gap(px(8.))
            .child(Label::new(icon).text_xs().text_color(TEXT_MUTED))
            .child(Label::new(label).text_sm().text_color(TEXT_PRIMARY))
            .child(div().flex_1())
            .child(Label::new(count.to_string()).text_xs().text_color(TEXT_MUTED))
    };

    div()
        .flex_col()
        .border_b_1()
        .border_color(BORDER)
        .child(
            div()
                .px(px(12.))
                .py(px(6.))
                .child(
                    Label::new("Resources")
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(TEXT_HEADING),
                ),
        )
        .child(row("▣", "Deployments", counts.deployments))
        .child(row("⬡", "Services",    counts.services))
        .child(row("≡", "ConfigMaps",  counts.configmaps))
        .child(row("◈", "Nodes",       counts.nodes))
        .pb(px(6.))
}
