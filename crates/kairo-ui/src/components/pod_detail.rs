use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kairo_core::{
    fmt_cpu, fmt_memory,
    models::{ConfigMapSummary, DeploymentSummary, NodeSummary, PodDetail, ServiceSummary},
};

use crate::analyze::AnalyzeEventRequest;
use crate::theme::{
    status_color, status_symbol, ACCENT, BORDER, HOVER_BG, STATUS_FAILED, STATUS_PENDING,
    STATUS_RUNNING, SURFACE, TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

// ── ResourceDetail ────────────────────────────────────────────────────────────

/// The detail content currently shown in the Details panel.
#[derive(Clone)]
pub enum ResourceDetail {
    Pod(PodDetail),
    Deployment(DeploymentSummary),
    Service(ServiceSummary),
    ConfigMap(ConfigMapSummary),
    Node(NodeSummary),
}

// ── DetailPanel ───────────────────────────────────────────────────────────────

/// Bottom-dock "Details" panel — shows structured detail for any selected resource.
pub struct DetailPanel {
    focus_handle: FocusHandle,
    detail: Option<ResourceDetail>,
}

impl DetailPanel {
    pub fn new(cx: &mut App) -> Self {
        Self { focus_handle: cx.focus_handle(), detail: None }
    }

    pub fn set_detail(&mut self, detail: ResourceDetail) {
        self.detail = Some(detail);
    }
    pub fn clear_detail(&mut self) {
        self.detail = None;
    }
}

impl EventEmitter<PanelEvent> for DetailPanel {}
impl EventEmitter<AnalyzeEventRequest> for DetailPanel {}

impl Focusable for DetailPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Panel for DetailPanel {
    fn panel_name(&self) -> &'static str { "DetailPanel" }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { "Details" }
    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> { None }
    fn closable(&self, _: &App) -> bool { false }
}

impl Render for DetailPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(detail) = self.detail.as_ref() else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(TEXT_MUTED)
                .child("Select a resource to view details")
                .into_any_element();
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scrollbar()
            .p_4()
            .gap_4()
            .child(match &detail {
                ResourceDetail::Pod(d)        => render_pod(d, cx),
                ResourceDetail::Deployment(d) => render_deployment(d),
                ResourceDetail::Service(d)    => render_service(d),
                ResourceDetail::ConfigMap(d)  => render_configmap(d),
                ResourceDetail::Node(d)       => render_node(d),
            })
            .into_any_element()
    }
}

// ── Shared primitives ─────────────────────────────────────────────────────────

fn section_title(text: &str) -> impl IntoElement {
    Label::new(text.to_string())
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_HEADING)
}

fn kv_row(key: &str, value: impl Into<SharedString>) -> impl IntoElement {
    h_flex()
        .gap_3()
        .py(px(1.))
        .child(
            div()
                .w(px(130.))
                .flex_shrink_0()
                .child(Label::new(key.to_string()).text_sm().text_color(TEXT_MUTED)),
        )
        .child(Label::new(value.into()).text_sm().text_color(TEXT_PRIMARY))
}

fn kv_row_colored(key: &str, value: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    h_flex()
        .gap_3()
        .py(px(1.))
        .child(
            div()
                .w(px(130.))
                .flex_shrink_0()
                .child(Label::new(key.to_string()).text_sm().text_color(TEXT_MUTED)),
        )
        .child(
            Label::new(value.into())
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(color),
        )
}

fn resource_bar(label: &str, value: i64, total: i64, detail_text: impl Into<SharedString>) -> impl IntoElement {
    let ratio = if total > 0 { (value as f32 / total as f32).clamp(0., 1.) } else { 0. };
    let bar_color = if ratio > 0.85 {
        STATUS_FAILED
    } else if ratio > 0.70 {
        STATUS_PENDING
    } else {
        STATUS_RUNNING
    };

    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .child(
            h_flex()
                .justify_between()
                .child(Label::new(label.to_string()).text_xs().text_color(TEXT_MUTED))
                .child(Label::new(detail_text.into()).text_xs().text_color(TEXT_SECONDARY)),
        )
        .child(
            div()
                .w_full()
                .h(px(5.))
                .rounded(px(3.))
                .bg(BORDER)
                .child(div().h_full().w(relative(ratio)).rounded(px(3.)).bg(bar_color)),
        )
}

fn kind_badge(kind: &str) -> impl IntoElement {
    div()
        .px(px(6.))
        .py(px(2.))
        .rounded(px(4.))
        .bg(SURFACE)
        .border_1()
        .border_color(BORDER)
        .child(Label::new(kind.to_string()).text_xs().text_color(TEXT_SECONDARY))
}

fn status_dot_label(status: &str) -> impl IntoElement {
    let color = status_color(status);
    let symbol = status_symbol(status);
    h_flex()
        .gap(px(5.))
        .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(color))
        .child(
            Label::new(format!("{symbol} {status}"))
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(color),
        )
}

fn name_header(name: &str, kind: &str, namespace: &str, age: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div().child(
                Label::new(name.to_string())
                    .text_size(rems(1.1))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT_PRIMARY),
            ),
        )
        .child(
            h_flex()
                .gap_2()
                .child(kind_badge(kind))
                .child(Label::new(namespace.to_string()).text_sm().text_color(TEXT_SECONDARY))
                .child(Label::new(format!("age: {age}")).text_sm().text_color(TEXT_MUTED)),
        )
}

// ── Pod renderer ──────────────────────────────────────────────────────────────

fn render_pod(detail: &PodDetail, cx: &mut Context<DetailPanel>) -> AnyElement {
    let s = &detail.summary;

    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            name_header(&s.name, "Pod", &s.namespace, &s.age)
                .child(status_dot_label(&s.status))
                .child(
                    h_flex()
                        .gap_3()
                        .child(Label::new(format!("node: {}", s.node)).text_sm().text_color(TEXT_SECONDARY))
                        .child(Label::new(format!("ready: {}", s.ready)).text_sm().text_color(TEXT_SECONDARY))
                        .child(Label::new(format!("restarts: {}", s.restarts)).text_sm().text_color(TEXT_SECONDARY)),
                ),
        )
        .child(render_kv_section("Labels", &detail.labels))
        .child(render_kv_section("Annotations", &detail.annotations))
        .child(render_pod_containers(detail))
        .child(render_pod_events(detail, cx))
        .into_any_element()
}

fn render_kv_section(title: &str, map: &std::collections::BTreeMap<String, String>) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(section_title(title));

    if map.is_empty() {
        section = section.child(Label::new("  (none)").text_sm().text_color(TEXT_MUTED));
    } else {
        for (k, v) in map {
            section = section.child(
                Label::new(format!("  {k}={v}"))
                    .text_sm()
                    .text_color(TEXT_SECONDARY),
            );
        }
    }
    section.into_any_element()
}

fn render_pod_containers(detail: &PodDetail) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_title("Containers"));

    for c in &detail.containers {
        let state_color = status_color(&c.state);
        let state_symbol = status_symbol(&c.state);
        section = section.child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .px_3()
                .py_2()
                .rounded(px(6.))
                .border_1()
                .border_color(BORDER)
                .bg(SURFACE)
                .child(
                    h_flex()
                        .gap(px(6.))
                        .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(state_color))
                        .child(
                            Label::new(format!("{state_symbol} {}", c.name))
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(TEXT_PRIMARY),
                        ),
                )
                .child(Label::new(c.image.clone()).text_sm().text_color(TEXT_MUTED))
                .child(
                    Label::new(format!(
                        "ready: {} | restarts: {} | {}",
                        c.ready, c.restart_count, c.state
                    ))
                    .text_sm()
                    .text_color(TEXT_SECONDARY),
                ),
        );
    }
    section.into_any_element()
}

fn render_pod_events(detail: &PodDetail, cx: &mut Context<DetailPanel>) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_title("Events"));

    if detail.events.is_empty() {
        section = section.child(Label::new("  No events").text_sm().text_color(TEXT_MUTED));
    } else {
        let pod_name = detail.summary.name.clone();
        let pod_ns = detail.summary.namespace.clone();
        let pod_status = detail.summary.status.clone();

        for ev in &detail.events {
            let type_color = if ev.event_type == "Warning" { STATUS_FAILED } else { TEXT_SECONDARY };
            let json = serde_json::json!({
                "pod": format!("{pod_ns}/{pod_name}"),
                "pod_status": pod_status,
                "event_type": ev.event_type,
                "reason": ev.reason,
                "message": ev.message,
                "count": ev.count,
                "first_time": ev.first_time,
                "last_time": ev.last_time,
            });
            let json_str = serde_json::to_string_pretty(&json).unwrap_or_default();

            section = section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(BORDER)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Label::new(ev.event_type.clone())
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(type_color),
                            )
                            .child(
                                Label::new(ev.reason.clone())
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(TEXT_PRIMARY),
                            )
                            .child(
                                Label::new(format!("×{}", ev.count))
                                    .text_sm()
                                    .text_color(TEXT_MUTED),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .cursor_pointer()
                                    .px(px(6.))
                                    .py(px(2.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(ACCENT)
                                    .hover(|s| s.bg(HOVER_BG))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_this, _, _, cx| {
                                            cx.emit(AnalyzeEventRequest(json_str.clone()));
                                        }),
                                    )
                                    .child(Label::new("⬡ Analyze").text_xs().text_color(ACCENT)),
                            ),
                    )
                    .child(Label::new(ev.message.clone()).text_sm().text_color(TEXT_SECONDARY)),
            );
        }
    }
    section.into_any_element()
}

// ── Deployment renderer ───────────────────────────────────────────────────────

fn render_deployment(d: &DeploymentSummary) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(name_header(&d.name, "Deployment", &d.namespace, &d.age))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(section_title("Replicas"))
                .child(kv_row("Ready", d.ready.clone()))
                .child(kv_row("Up-to-date", d.up_to_date.to_string()))
                .child(kv_row("Available", d.available.to_string())),
        )
        .into_any_element()
}

// ── Service renderer ──────────────────────────────────────────────────────────

fn render_service(s: &ServiceSummary) -> AnyElement {
    let type_color = match s.type_.as_str() {
        "LoadBalancer" => STATUS_RUNNING,
        "NodePort"     => STATUS_PENDING,
        _              => TEXT_SECONDARY,
    };

    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(name_header(&s.name, "Service", &s.namespace, &s.age))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(section_title("Network"))
                .child(kv_row_colored("Type", s.type_.clone(), type_color))
                .child(kv_row("Cluster IP", s.cluster_ip.clone()))
                .child(kv_row("External IP", s.external_ip.clone()))
                .child(kv_row("Ports", s.ports.clone())),
        )
        .into_any_element()
}

// ── ConfigMap renderer ────────────────────────────────────────────────────────

fn render_configmap(c: &ConfigMapSummary) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(name_header(&c.name, "ConfigMap", &c.namespace, &c.age))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(section_title("Data"))
                .child(kv_row("Keys", c.data_count.to_string()))
                .child(
                    Label::new("Open the YAML tab to inspect key values")
                        .text_xs()
                        .text_color(TEXT_MUTED),
                ),
        )
        .into_any_element()
}

// ── Node renderer ─────────────────────────────────────────────────────────────

fn render_node(n: &NodeSummary) -> AnyElement {
    let cpu_detail = format!(
        "{} / {}",
        fmt_cpu(n.cpu_allocatable_milli),
        fmt_cpu(n.cpu_capacity_milli),
    );
    let mem_detail = format!(
        "{} / {}",
        fmt_memory(n.memory_allocatable_bytes),
        fmt_memory(n.memory_capacity_bytes),
    );

    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            name_header(&n.name, "Node", &n.roles, &n.age)
                .child(status_dot_label(&n.status)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(section_title("System"))
                .child(kv_row("Version", n.version.clone()))
                .child(kv_row("OS", n.os_image.clone()))
                .child(kv_row("Pod capacity", n.pod_capacity.to_string())),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(section_title("Allocatable Resources"))
                .child(resource_bar(
                    "CPU (allocatable / capacity)",
                    n.cpu_allocatable_milli,
                    n.cpu_capacity_milli,
                    cpu_detail,
                ))
                .child(resource_bar(
                    "Memory (allocatable / capacity)",
                    n.memory_allocatable_bytes,
                    n.memory_capacity_bytes,
                    mem_detail,
                )),
        )
        .into_any_element()
}
