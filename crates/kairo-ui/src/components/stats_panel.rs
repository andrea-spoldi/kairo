use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kairo_core::{
    fmt_cpu, fmt_memory,
    models::{ConfigMapSummary, DeploymentSummary, NodeSummary, ServiceSummary},
};

use crate::components::pod_detail::ResourceDetail;
use crate::theme::{
    BORDER, STATUS_FAILED, STATUS_PENDING, STATUS_RUNNING, SURFACE, TEXT_HEADING, TEXT_MUTED,
    TEXT_PRIMARY, TEXT_SECONDARY,
};

/// Inspector "Stats" tab — numbers-at-a-glance view for the selected resource.
pub struct StatsPanel {
    focus_handle: FocusHandle,
    detail: Option<ResourceDetail>,
}

impl StatsPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            detail: None,
        }
    }

    pub fn set_detail(&mut self, detail: Option<ResourceDetail>, cx: &mut Context<Self>) {
        self.detail = detail;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.detail = None;
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for StatsPanel {}

impl Focusable for StatsPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for StatsPanel {
    fn panel_name(&self) -> &'static str {
        "StatsPanel"
    }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Stats"
    }
    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> {
        None
    }
    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Render for StatsPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(detail) = self.detail.as_ref() else {
            return empty_state("Select a resource to view stats");
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scrollbar()
            .p_4()
            .gap_4()
            .child(match detail {
                ResourceDetail::Pod(d) => render_pod_stats(d),
                ResourceDetail::Deployment(d) => render_deployment_stats(d),
                ResourceDetail::Service(s) => render_service_stats(s),
                ResourceDetail::ConfigMap(c) => render_configmap_stats(c),
                ResourceDetail::Node(n) => render_node_stats(n),
                ResourceDetail::Generic(_) => empty_state("No stats for this resource kind"),
            })
            .into_any_element()
    }
}

// ── Shared primitives ─────────────────────────────────────────────────────────

fn empty_state(msg: &str) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(TEXT_MUTED)
        .child(msg.to_string())
        .into_any_element()
}

fn section_title(text: &str) -> impl IntoElement {
    Label::new(text.to_string())
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_HEADING)
}

fn stat_tile(label: &str, value: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px_3()
        .py_2()
        .rounded(px(6.))
        .border_1()
        .border_color(BORDER)
        .bg(SURFACE)
        .child(
            Label::new(label.to_string())
                .text_xs()
                .text_color(TEXT_MUTED),
        )
        .child(
            Label::new(value.into())
                .text_size(rems(1.1))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color),
        )
}

fn kv_row(key: &str, value: impl Into<SharedString>) -> impl IntoElement {
    h_flex()
        .gap_3()
        .py(px(1.))
        .child(
            div()
                .w(px(140.))
                .flex_shrink_0()
                .child(
                    Label::new(key.to_string())
                        .text_sm()
                        .text_color(TEXT_MUTED),
                ),
        )
        .child(
            Label::new(value.into())
                .text_sm()
                .text_color(TEXT_PRIMARY),
        )
}

fn section<F>(title: &str, body: F) -> Div
where
    F: FnOnce(Div) -> Div,
{
    let section = div().flex().flex_col().gap_1().child(section_title(title));
    body(section)
}

// ── Pod ───────────────────────────────────────────────────────────────────────

fn render_pod_stats(detail: &kairo_core::models::PodDetail) -> AnyElement {
    let summary = &detail.summary;
    let ready = summary.ready.clone();
    let status_color = match summary.status.as_str() {
        "Running" => STATUS_RUNNING,
        "Pending" | "ContainerCreating" | "Initializing" => STATUS_PENDING,
        _ => STATUS_FAILED,
    };

    let ready_containers = detail.containers.iter().filter(|c| c.ready).count();

    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            h_flex()
                .gap_2()
                .child(stat_tile("Status", summary.status.clone(), status_color))
                .child(stat_tile("Ready", ready, TEXT_PRIMARY))
                .child(stat_tile(
                    "Restarts",
                    summary.restarts.to_string(),
                    if summary.restarts > 0 {
                        STATUS_PENDING
                    } else {
                        TEXT_PRIMARY
                    },
                ))
                .child(stat_tile("Age", summary.age.clone(), TEXT_PRIMARY)),
        )
        .child(section("Placement", |s| {
            s.child(kv_row("Node", summary.node.clone()))
                .child(kv_row("Namespace", summary.namespace.clone()))
                .child(kv_row(
                    "Containers",
                    format!("{ready_containers}/{}", detail.containers.len()),
                ))
        }))
        .child(section("Owner", |s| {
            let owner = if summary.owner_kind.is_empty() {
                "<standalone>".to_string()
            } else {
                format!("{} / {}", summary.owner_kind, summary.owner_name)
            };
            s.child(kv_row("Controller", owner))
        }))
        .into_any_element()
}

// ── Deployment ────────────────────────────────────────────────────────────────

fn render_deployment_stats(d: &DeploymentSummary) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            h_flex()
                .gap_2()
                .child(stat_tile("Ready", d.ready.clone(), STATUS_RUNNING))
                .child(stat_tile(
                    "Up-to-date",
                    d.up_to_date.to_string(),
                    TEXT_PRIMARY,
                ))
                .child(stat_tile(
                    "Available",
                    d.available.to_string(),
                    STATUS_RUNNING,
                ))
                .child(stat_tile("Age", d.age.clone(), TEXT_PRIMARY)),
        )
        .child(section("Placement", |s| {
            s.child(kv_row("Namespace", d.namespace.clone()))
                .child(kv_row("Name", d.name.clone()))
        }))
        .into_any_element()
}

// ── Service ───────────────────────────────────────────────────────────────────

fn render_service_stats(s: &ServiceSummary) -> AnyElement {
    let type_color = match s.type_.as_str() {
        "LoadBalancer" => STATUS_RUNNING,
        "NodePort" => STATUS_PENDING,
        _ => TEXT_SECONDARY,
    };

    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            h_flex()
                .gap_2()
                .child(stat_tile("Type", s.type_.clone(), type_color))
                .child(stat_tile(
                    "Cluster IP",
                    s.cluster_ip.clone(),
                    TEXT_PRIMARY,
                ))
                .child(stat_tile("Age", s.age.clone(), TEXT_PRIMARY)),
        )
        .child(section("Network", |sec| {
            sec.child(kv_row("External IP", s.external_ip.clone()))
                .child(kv_row("Ports", s.ports.clone()))
                .child(kv_row("Namespace", s.namespace.clone()))
        }))
        .into_any_element()
}

// ── ConfigMap ─────────────────────────────────────────────────────────────────

fn render_configmap_stats(c: &ConfigMapSummary) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            h_flex()
                .gap_2()
                .child(stat_tile(
                    "Entries",
                    c.data_count.to_string(),
                    TEXT_PRIMARY,
                ))
                .child(stat_tile("Age", c.age.clone(), TEXT_PRIMARY)),
        )
        .child(section("Placement", |s| {
            s.child(kv_row("Namespace", c.namespace.clone()))
                .child(kv_row("Name", c.name.clone()))
        }))
        .into_any_element()
}

// ── Node ──────────────────────────────────────────────────────────────────────

fn render_node_stats(n: &NodeSummary) -> AnyElement {
    let status_color = match n.status.as_str() {
        "Ready" => STATUS_RUNNING,
        "NotReady" => STATUS_FAILED,
        _ => STATUS_PENDING,
    };

    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            h_flex()
                .gap_2()
                .child(stat_tile("Status", n.status.clone(), status_color))
                .child(stat_tile(
                    "Pod capacity",
                    n.pod_capacity.to_string(),
                    TEXT_PRIMARY,
                ))
                .child(stat_tile("Age", n.age.clone(), TEXT_PRIMARY)),
        )
        .child(section("CPU", |s| {
            s.child(kv_row("Allocatable", fmt_cpu(n.cpu_allocatable_milli)))
                .child(kv_row("Capacity", fmt_cpu(n.cpu_capacity_milli)))
        }))
        .child(section("Memory", |s| {
            s.child(kv_row("Allocatable", fmt_memory(n.memory_allocatable_bytes)))
                .child(kv_row("Capacity", fmt_memory(n.memory_capacity_bytes)))
        }))
        .child(section("System", |s| {
            s.child(kv_row("Version", n.version.clone()))
                .child(kv_row("OS", n.os_image.clone()))
                .child(kv_row("Roles", n.roles.clone()))
        }))
        .into_any_element()
    }
