use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kairo_core::models::{ConfigMapSummary, DeploymentSummary, NodeSummary, ServiceSummary};

use crate::theme::{
    BORDER, STATUS_FAILED, STATUS_PENDING, STATUS_RUNNING, STATUS_SUCCEEDED,
    SURFACE, TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
                            .children(items.into_iter().map(|d| {
                                h_flex()
                                    .border_b_1()
                                    .border_color(BORDER)
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
                            .children(items.into_iter().map(|s| {
                                let type_color = match s.type_.as_str() {
                                    "LoadBalancer" => STATUS_RUNNING,
                                    "NodePort"     => STATUS_PENDING,
                                    "ExternalName" => STATUS_SUCCEEDED,
                                    _              => TEXT_SECONDARY,
                                };
                                h_flex()
                                    .border_b_1()
                                    .border_color(BORDER)
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
                            .children(items.into_iter().map(|cm| {
                                h_flex()
                                    .border_b_1()
                                    .border_color(BORDER)
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(header_cell("NAME", 200.))
                    .child(header_cell("STATUS", 90.))
                    .child(header_cell("ROLES", 160.))
                    .child(header_cell("VERSION", 120.))
                    .child(header_cell("OS", 200.))
                    .child(header_cell("AGE", 70.)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex_col()
                            .children(items.into_iter().map(|n| {
                                let status_color = if n.status == "Ready" {
                                    STATUS_RUNNING
                                } else {
                                    STATUS_FAILED
                                };
                                h_flex()
                                    .border_b_1()
                                    .border_color(BORDER)
                                    .child(cell(n.name, 200.))
                                    .child(
                                        div().w(px(90.)).flex_shrink_0().px(px(8.)).py(px(6.))
                                            .text_sm().text_color(status_color)
                                            .child(n.status),
                                    )
                                    .child(muted_cell(n.roles, 160.))
                                    .child(muted_cell(n.version, 120.))
                                    .child(muted_cell(n.os_image, 200.))
                                    .child(muted_cell(n.age, 70.))
                            }))
                            .overflow_y_scrollbar(),
                    ),
            )
    }
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
