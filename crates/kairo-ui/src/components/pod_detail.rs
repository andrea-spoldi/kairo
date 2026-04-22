use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use gpui_component::text::TextView;
use kairo_core::{
    fmt_cpu, fmt_memory,
    models::{
        ClusterEvent, ConfigMapSummary, DeploymentSummary, GenericResourceDetail, NodeSummary,
        PodDetail, ServiceSummary,
    },
};

use crate::analyze::AnalyzeEventRequest;
use crate::theme::{
    status_color, status_symbol, ACCENT, ACCENT_BG, ACCENT_BORDER, ACCENT_FG,
    BORDER, HOVER_BG, STATUS_FAILED, STATUS_PENDING, STATUS_RUNNING,
    SURFACE, TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

/// Emitted when the user clicks "Send to Agent" after an event selected a resource.
#[derive(Clone, Debug)]
pub struct SendEventToAgent(pub ClusterEvent);

/// Emitted when the user clicks "Analyze with Agent" for the currently displayed resource.
#[derive(Clone, Debug)]
pub struct AnalyzeResourceRequest {
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
}

/// Small inline chip that copies `value` to the clipboard on click.
fn copy_chip(
    id: impl Into<ElementId>,
    value: impl Into<String>,
    cx: &mut Context<DetailPanel>,
) -> impl IntoElement {
    let value = value.into();
    div()
        .id(id.into())
        .px(px(5.))
        .py(px(1.))
        .rounded(px(3.))
        .cursor_pointer()
        .text_xs()
        .text_color(TEXT_MUTED)
        .hover(|s| s.text_color(TEXT_PRIMARY).bg(HOVER_BG))
        .child("⎘")
        .on_click(cx.listener(move |_, _: &ClickEvent, _window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
        }))
}

// ── ResourceDetail ────────────────────────────────────────────────────────────

/// The detail content currently shown in the Details panel.
#[derive(Clone)]
pub enum ResourceDetail {
    Pod(PodDetail),
    Deployment(DeploymentSummary),
    Service(ServiceSummary),
    ConfigMap(ConfigMapSummary),
    Node(NodeSummary),
    /// Fallback view for kinds without a typed renderer, or for resources whose
    /// typed fetch failed (deleted / forbidden).
    Generic(GenericResourceDetail),
}

// ── DetailPanel ───────────────────────────────────────────────────────────────

/// Bottom-dock "Details" panel — shows structured detail for any selected resource.
pub struct DetailPanel {
    focus_handle: FocusHandle,
    detail: Option<ResourceDetail>,
    /// Set when the detail panel was opened by clicking an event card body.
    /// Enables the "Send to Agent" button in the action bar.
    associated_event: Option<ClusterEvent>,
}

impl DetailPanel {
    pub fn new(cx: &mut App) -> Self {
        Self { focus_handle: cx.focus_handle(), detail: None, associated_event: None }
    }

    pub fn set_detail(&mut self, detail: ResourceDetail) {
        self.detail = Some(detail);
        // New resource selection clears the event association — user must click event again.
        self.associated_event = None;
    }

    pub fn clear_detail(&mut self) {
        self.detail = None;
        self.associated_event = None;
    }

    /// Bind a cluster event to the current detail view (enables "Send to Agent" button).
    /// Called by Workspace after opening a resource via an event card body click.
    pub fn set_associated_event(&mut self, event: Option<ClusterEvent>) {
        self.associated_event = event;
    }

    pub fn current_detail(&self) -> Option<&ResourceDetail> {
        self.detail.as_ref()
    }
}

impl EventEmitter<PanelEvent> for DetailPanel {}
impl EventEmitter<AnalyzeEventRequest> for DetailPanel {}
impl EventEmitter<SendEventToAgent> for DetailPanel {}
impl EventEmitter<AnalyzeResourceRequest> for DetailPanel {}

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

        let detail = detail.clone();
        let has_event = self.associated_event.is_some();

        div()
            .size_full()
            .flex()
            .flex_col()
            // ── Action bar ────────────────────────────────────────────────────
            .child(
                h_flex()
                    .px_3()
                    .py_1()
                    .border_b_1()
                    .border_color(BORDER)
                    .flex_shrink_0()
                    .gap_2()
                    .child(div().flex_1())
                    .when(has_event, |el| {
                        el.child(
                            div()
                                .cursor_pointer()
                                .px(px(6.))
                                .py(px(2.))
                                .rounded(px(4.))
                                .border_1()
                                .border_color(BORDER)
                                .hover(|s| s.bg(HOVER_BG))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        if let Some(ev) = this.associated_event.clone() {
                                            cx.emit(SendEventToAgent(ev));
                                        }
                                    }),
                                )
                                .child(Label::new("⬡ Send to Agent").text_xs().text_color(ACCENT)),
                        )
                    })
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
                                cx.listener(|this, _, _, cx| {
                                    let req = match &this.detail {
                                        Some(ResourceDetail::Pod(d)) => Some(AnalyzeResourceRequest {
                                            kind: "Pod".to_string(),
                                            name: d.summary.name.clone(),
                                            namespace: Some(d.summary.namespace.clone()),
                                        }),
                                        Some(ResourceDetail::Deployment(d)) => Some(AnalyzeResourceRequest {
                                            kind: "Deployment".to_string(),
                                            name: d.name.clone(),
                                            namespace: Some(d.namespace.clone()),
                                        }),
                                        Some(ResourceDetail::Service(d)) => Some(AnalyzeResourceRequest {
                                            kind: "Service".to_string(),
                                            name: d.name.clone(),
                                            namespace: Some(d.namespace.clone()),
                                        }),
                                        Some(ResourceDetail::ConfigMap(d)) => Some(AnalyzeResourceRequest {
                                            kind: "ConfigMap".to_string(),
                                            name: d.name.clone(),
                                            namespace: Some(d.namespace.clone()),
                                        }),
                                        Some(ResourceDetail::Node(d)) => Some(AnalyzeResourceRequest {
                                            kind: "Node".to_string(),
                                            name: d.name.clone(),
                                            namespace: None,
                                        }),
                                        Some(ResourceDetail::Generic(d)) => Some(AnalyzeResourceRequest {
                                            kind: d.kind.clone(),
                                            name: d.name.clone(),
                                            namespace: if d.namespace.is_empty() { None } else { Some(d.namespace.clone()) },
                                        }),
                                        None => None,
                                    };
                                    if let Some(req) = req {
                                        cx.emit(req);
                                    }
                                }),
                            )
                            .child(Label::new("⬡ Analyze with Agent").text_xs().text_color(ACCENT)),
                    ),
            )
            // ── Detail content ─────────────────────────────────────────────────
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .p_4()
                            .gap_4()
                            .child(match &detail {
                                ResourceDetail::Pod(d)        => render_pod(d, cx),
                                ResourceDetail::Deployment(d) => render_deployment(d),
                                ResourceDetail::Service(d)    => render_service(d),
                                ResourceDetail::ConfigMap(d)  => render_configmap(d),
                                ResourceDetail::Node(d)       => render_node(d),
                                ResourceDetail::Generic(d)    => render_generic(d),
                            })
                            .overflow_y_scrollbar(),
                    ),
            )
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

fn stat_card(label: &str, value: impl Into<SharedString>, value_color: Hsla) -> impl IntoElement {
    div()
        .rounded(px(16.))
        .border_1()
        .border_color(BORDER)
        .bg(Hsla { h: 0.0, s: 0.0, l: 1.0, a: 0.03 })
        .p(px(12.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            Label::new(label.to_uppercase())
                .text_xs()
                .text_color(TEXT_MUTED),
        )
        .child(
            Label::new(value.into())
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(value_color),
        )
}

fn panel_section(content: impl IntoElement) -> impl IntoElement {
    div()
        .rounded(px(20.))
        .border_1()
        .border_color(BORDER)
        .bg(Hsla { h: 0.0, s: 0.0, l: 1.0, a: 0.03 })
        .p(px(16.))
        .child(content)
}

fn action_chip(
    id: impl Into<ElementId>,
    label: &str,
) -> impl IntoElement {
    div()
        .id(id.into())
        .cursor_pointer()
        .px(px(10.))
        .py(px(4.))
        .rounded_full()
        .border_1()
        .border_color(ACCENT_BORDER)
        .bg(ACCENT_BG)
        .child(Label::new(label.to_string()).text_xs().text_color(ACCENT_FG))
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
    let name = s.name.clone();
    let namespace = s.namespace.clone();
    let node = s.node.clone();

    let stat_color = status_color(&s.status);

    div()
        .flex()
        .flex_col()
        .gap_3()
        // ── Name header ───────────────────────────────────────────────────────
        .child(
            name_header(&s.name, "Pod", &s.namespace, &s.age)
                .child(status_dot_label(&s.status)),
        )
        // ── 2×2 StatCard grid ─────────────────────────────────────────────────
        .child(
            div()
                .grid()
                .grid_cols(2)
                .gap(px(8.))
                .child(stat_card("Namespace", s.namespace.clone(), TEXT_SECONDARY))
                .child(stat_card("Phase", s.status.clone(), stat_color))
                .child(stat_card("Restarts", s.restarts.to_string(), if s.restarts > 0 { STATUS_FAILED } else { TEXT_SECONDARY }))
                .child(stat_card("Node", s.node.clone(), TEXT_SECONDARY)),
        )
        // ── Labels ────────────────────────────────────────────────────────────
        .child(panel_section(render_kv_section("Labels", &detail.labels, cx)))
        // ── Annotations ───────────────────────────────────────────────────────
        .child(panel_section(render_kv_section("Annotations", &detail.annotations, cx)))
        // ── Containers ────────────────────────────────────────────────────────
        .child(panel_section(render_pod_containers(detail)))
        // ── Events ───────────────────────────────────────────────────────────
        .child(panel_section(render_pod_events(detail, cx)))
        // ── Suggested actions ─────────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(Label::new("Suggested actions").text_xs().text_color(TEXT_MUTED))
                .child(
                    h_flex()
                        .gap(px(6.))
                        .flex_wrap()
                        .child(action_chip("pod-action-logs", "Open logs"))
                        .child(action_chip("pod-action-analyze", "Analyze with Agent"))
                        .child(action_chip("pod-action-copy", "Copy name"))
                        .child(copy_chip("pod-copy-name", name, cx))
                        .child(copy_chip("pod-copy-ns", namespace, cx))
                        .child(copy_chip("pod-copy-node", node, cx)),
                ),
        )
        .into_any_element()
}

fn render_kv_section(
    title: &str,
    map: &std::collections::BTreeMap<String, String>,
    cx: &mut Context<DetailPanel>,
) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(section_title(title));

    if map.is_empty() {
        section = section.child(Label::new("  (none)").text_sm().text_color(TEXT_MUTED));
    } else {
        for (k, v) in map {
            let kv = format!("{k}={v}");
            let chip_id = SharedString::from(format!("copy-{title}-{k}"));
            section = section.child(
                h_flex()
                    .gap_1()
                    .child(
                        Label::new(format!("  {kv}"))
                            .text_sm()
                            .text_color(TEXT_SECONDARY),
                    )
                    .child(copy_chip(chip_id, kv, cx)),
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

        for (i, ev) in detail.events.iter().enumerate() {
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
                    .child(
                        TextView::markdown(
                            SharedString::from(format!("ev-msg-{i}")),
                            SharedString::from(ev.message.clone()),
                        )
                        .selectable(true),
                    ),
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
        .gap_3()
        .child(name_header(&d.name, "Deployment", &d.namespace, &d.age))
        .child(
            div()
                .grid()
                .grid_cols(2)
                .gap(px(8.))
                .child(stat_card("Namespace", d.namespace.clone(), TEXT_SECONDARY))
                .child(stat_card("Ready", d.ready.clone(), STATUS_RUNNING))
                .child(stat_card("Up-to-date", d.up_to_date.to_string(), TEXT_SECONDARY))
                .child(stat_card("Age", d.age.clone(), TEXT_MUTED)),
        )
        .child(panel_section(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(section_title("Replicas"))
                .child(kv_row("Ready", d.ready.clone()))
                .child(kv_row("Up-to-date", d.up_to_date.to_string()))
                .child(kv_row("Available", d.available.to_string())),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(Label::new("Suggested actions").text_xs().text_color(TEXT_MUTED))
                .child(
                    h_flex()
                        .gap(px(6.))
                        .flex_wrap()
                        .child(action_chip("dep-action-analyze", "Analyze with Agent"))
                        .child(action_chip("dep-action-pods", "Show pods"))
                        .child(action_chip("dep-action-copy", "Copy name")),
                ),
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
        .gap_3()
        .child(name_header(&s.name, "Service", &s.namespace, &s.age))
        .child(
            div()
                .grid()
                .grid_cols(2)
                .gap(px(8.))
                .child(stat_card("Namespace", s.namespace.clone(), TEXT_SECONDARY))
                .child(stat_card("Type", s.type_.clone(), type_color))
                .child(stat_card("Cluster IP", s.cluster_ip.clone(), TEXT_SECONDARY))
                .child(stat_card("Ports", s.ports.clone(), TEXT_SECONDARY)),
        )
        .child(panel_section(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(section_title("Network"))
                .child(kv_row_colored("Type", s.type_.clone(), type_color))
                .child(kv_row("Cluster IP", s.cluster_ip.clone()))
                .child(kv_row("External IP", s.external_ip.clone()))
                .child(kv_row("Ports", s.ports.clone())),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(Label::new("Suggested actions").text_xs().text_color(TEXT_MUTED))
                .child(
                    h_flex()
                        .gap(px(6.))
                        .flex_wrap()
                        .child(action_chip("svc-action-analyze", "Analyze with Agent"))
                        .child(action_chip("svc-action-endpoints", "Show endpoints"))
                        .child(action_chip("svc-action-copy", "Copy name")),
                ),
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

// ── Generic renderer ──────────────────────────────────────────────────────────

fn render_generic(d: &GenericResourceDetail) -> AnyElement {
    let ns = if d.namespace.is_empty() { "<cluster>".to_string() } else { d.namespace.clone() };
    let age = d.age.clone().unwrap_or_else(|| "?".to_string());

    let body: AnyElement = if let Some(reason) = d.error.as_ref() {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded(px(6.))
            .border_1()
            .border_color(STATUS_FAILED)
            .child(
                Label::new("Resource unavailable")
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(STATUS_FAILED),
            )
            .child(
                Label::new(format!(
                    "The {} may have been deleted or is not accessible. ({reason})",
                    d.kind
                ))
                .text_sm()
                .text_color(TEXT_MUTED),
            )
            .into_any_element()
    } else if d.loading {
        Label::new("Loading…")
            .text_sm()
            .text_color(TEXT_MUTED)
            .into_any_element()
    } else {
        let mut section = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(section_title("Overview"))
            .child(kv_row("Kind", d.kind.clone()))
            .child(kv_row("Namespace", ns.clone()))
            .child(kv_row("Name", d.name.clone()))
            .child(kv_row("Age", age.clone()));
        if let Some(status) = d.status_summary.as_ref() {
            section = section.child(kv_row("Status", status.clone()));
        }
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(section)
            .child(
                Label::new("Open the YAML tab for the full manifest")
                    .text_xs()
                    .text_color(TEXT_MUTED),
            )
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(name_header(&d.name, &d.kind, &ns, &age))
        .child(body)
        .into_any_element()
}

// ── Node renderer ─────────────────────────────────────────────────────────────

fn render_node(n: &NodeSummary) -> AnyElement {
    let cpu_ratio = if n.cpu_capacity_milli > 0 {
        (n.cpu_allocatable_milli as f64 / n.cpu_capacity_milli as f64 * 100.0) as i64
    } else { 0 };
    let mem_ratio = if n.memory_capacity_bytes > 0 {
        (n.memory_allocatable_bytes as f64 / n.memory_capacity_bytes as f64 * 100.0) as i64
    } else { 0 };
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
    let node_status_color = status_color(&n.status);

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            name_header(&n.name, "Node", &n.roles, &n.age)
                .child(status_dot_label(&n.status)),
        )
        .child(
            div()
                .grid()
                .grid_cols(2)
                .gap(px(8.))
                .child(stat_card("Status", n.status.clone(), node_status_color))
                .child(stat_card("CPU%", format!("{}%", cpu_ratio), TEXT_SECONDARY))
                .child(stat_card("Mem%", format!("{}%", mem_ratio), TEXT_SECONDARY))
                .child(stat_card("Pods", n.pod_capacity.to_string(), TEXT_SECONDARY)),
        )
        .child(panel_section(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(section_title("System"))
                .child(kv_row("Version", n.version.clone()))
                .child(kv_row("OS", n.os_image.clone()))
                .child(kv_row("Pod capacity", n.pod_capacity.to_string())),
        ))
        .child(panel_section(
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
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(Label::new("Suggested actions").text_xs().text_color(TEXT_MUTED))
                .child(
                    h_flex()
                        .gap(px(6.))
                        .flex_wrap()
                        .child(action_chip("node-action-analyze", "Analyze with Agent"))
                        .child(action_chip("node-action-pods", "Show pods")),
                ),
        )
        .into_any_element()
}
