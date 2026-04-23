use std::collections::VecDeque;

use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kairo_core::models::ClusterEvent;

use crate::scope::{AgentScope, EventRef, ResourceRef};
use crate::theme::{
    ACCENT_BG, ACCENT_BORDER, ACCENT_FG, BORDER, HOVER_BG,
    SEV_CRITICAL_BG, SEV_CRITICAL_BORDER, SEV_CRITICAL_FG,
    SEV_INFO_BG, SEV_INFO_BORDER, SEV_INFO_FG,
    SEV_WARNING_BG, SEV_WARNING_BORDER, SEV_WARNING_FG,
    SURFACE, TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

const MAX_EVENTS: usize = 100;

// ── Severity ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum Severity { Critical, Warning, Info }

fn event_severity(ev: &ClusterEvent) -> Severity {
    match ev.event_type.as_str() {
        "Warning" => match ev.reason.as_str() {
            r if r.contains("BackOff") || r.contains("Failed") || r.contains("OOM") => Severity::Critical,
            _ => Severity::Warning,
        },
        _ => Severity::Info,
    }
}

fn sev_colors(sev: Severity) -> (Hsla, Hsla, Hsla) {
    match sev {
        Severity::Critical => (SEV_CRITICAL_BORDER, SEV_CRITICAL_BG, SEV_CRITICAL_FG),
        Severity::Warning  => (SEV_WARNING_BORDER,  SEV_WARNING_BG,  SEV_WARNING_FG),
        Severity::Info     => (SEV_INFO_BORDER,     SEV_INFO_BG,     SEV_INFO_FG),
    }
}

// ── Public events ─────────────────────────────────────────────────────────────

/// Emitted when the user clicks the "Inspect" button on an event card.
#[derive(Clone)]
pub struct EventBodyClicked(pub ClusterEvent);

/// Emitted when the user clicks "Send to Agent" directly on an event card,
/// bypassing the inspector step.
#[derive(Clone)]
pub struct SendEventDirectToAgent(pub AgentScope);

/// Center-panel tab — live feed of cluster-wide Warning events.
pub struct EventFeedPanel {
    focus_handle: FocusHandle,
    events: VecDeque<ClusterEvent>,
    /// When non-empty, only show events in this namespace.
    filter_namespace: String,
}

impl EventEmitter<EventBodyClicked> for EventFeedPanel {}
impl EventEmitter<SendEventDirectToAgent> for EventFeedPanel {}

impl EventFeedPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            events: VecDeque::new(),
            filter_namespace: String::new(),
        }
    }

    /// Prepend a new event (newest first). Drops oldest beyond MAX_EVENTS.
    pub fn push_event(&mut self, ev: ClusterEvent, cx: &mut Context<Self>) {
        self.events.push_front(ev);
        if self.events.len() > MAX_EVENTS {
            self.events.pop_back();
        }
        cx.notify();
    }

    /// Clear all events (e.g. on context switch).
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.events.clear();
        cx.notify();
    }

    /// Set the namespace filter. Empty string means "all namespaces".
    pub fn set_namespace_filter(&mut self, ns: String, cx: &mut Context<Self>) {
        self.filter_namespace = ns;
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for EventFeedPanel {}

impl Focusable for EventFeedPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for EventFeedPanel {
    fn panel_name(&self) -> &'static str { "EventFeedPanel" }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Events"
    }

    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> { None }

    fn closable(&self, _: &App) -> bool { false }
}

impl Render for EventFeedPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ns_filter = self.filter_namespace.clone();
        let events: Vec<ClusterEvent> = self.events.iter()
            .filter(|ev| ns_filter.is_empty() || ev.namespace == ns_filter)
            .cloned()
            .collect();
        let count = events.len();

        let mut event_list = div().flex().flex_col().p_2().gap_2();
        for ev in &events {
            event_list = event_list.child(render_event_card(ev, cx));
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            // ── Header strip ──────────────────────────────────────────────────
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(BORDER)
                    .flex_shrink_0()
                    .child(
                        Label::new(format!("Warning Events ({count})"))
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(TEXT_HEADING),
                    ),
            )
            // ── Event list ────────────────────────────────────────────────────
            .child(
                div()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .child(if events.is_empty() {
                        div()
                            .p_4()
                            .child(
                                Label::new("No warning events — cluster looks healthy.")
                                    .text_sm()
                                    .text_color(TEXT_MUTED),
                            )
                            .into_any_element()
                    } else {
                        event_list.into_any_element()
                    }),
            )
    }
}

// ── Card renderer ─────────────────────────────────────────────────────────────

fn render_event_card(ev: &ClusterEvent, cx: &mut Context<EventFeedPanel>) -> impl IntoElement {
    let sev = event_severity(ev);
    let (border_col, bg_col, fg_col) = sev_colors(sev);

    let card_id = ElementId::Name(
        format!("event-card-{}-{}-{}", ev.namespace, ev.object_name, ev.last_time).into(),
    );

    let ev_inspect = ev.clone();
    let ev_agent = ev.clone();

    div()
        .id(card_id)
        .flex()
        .flex_col()
        .gap(px(6.))
        .px_3()
        .py_3()
        .rounded(px(16.))
        .border_1()
        .border_color(border_col)
        .bg(bg_col)
        // ── Header row: time + namespace ──────────────────────────────────────
        .child(
            h_flex()
                .gap_2()
                .child(
                    Label::new(ev.last_time.clone())
                        .text_xs()
                        .text_color(TEXT_MUTED),
                )
                .child(div().flex_1())
                .child(
                    Label::new(ev.namespace.clone())
                        .text_xs()
                        .text_color(TEXT_SECONDARY),
                ),
        )
        // ── Body: reason + target ─────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    Label::new(ev.reason.clone())
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(fg_col),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Label::new(ev.object_name.clone())
                                .text_sm()
                                .text_color(TEXT_PRIMARY),
                        )
                        .child(
                            Label::new(format!("· {}", ev.object_kind))
                                .text_xs()
                                .text_color(TEXT_MUTED),
                        ),
                )
                .child(
                    Label::new(ev.message.clone())
                        .text_xs()
                        .text_color(TEXT_MUTED),
                ),
        )
        // ── Footer: count + action buttons ───────────────────────────────────
        .child(
            h_flex()
                .gap_2()
                .child(
                    Label::new(format!("×{}", ev.count))
                        .text_xs()
                        .text_color(TEXT_MUTED),
                )
                .child(div().flex_1())
                // "Inspect" button — opens the inspector
                .child(
                    div()
                        .cursor_pointer()
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(BORDER)
                        .bg(SURFACE)
                        .hover(|s| s.bg(HOVER_BG))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_this, _, _, cx| {
                                cx.stop_propagation();
                                cx.emit(EventBodyClicked(ev_inspect.clone()));
                            }),
                        )
                        .child(Label::new("Inspect").text_xs().text_color(TEXT_PRIMARY)),
                )
                // "Send to Agent" button — skips inspector, goes straight to agent
                .child(
                    div()
                        .cursor_pointer()
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(ACCENT_BORDER)
                        .bg(ACCENT_BG)
                        .hover(|s| s.bg(HOVER_BG))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_this, _, _, cx| {
                                cx.stop_propagation();
                                let ev = &ev_agent;
                                let resource_ref = ResourceRef {
                                    kind: ev.object_kind.clone(),
                                    name: ev.object_name.clone(),
                                    namespace: if ev.namespace.is_empty() {
                                        None
                                    } else {
                                        Some(ev.namespace.clone())
                                    },
                                };
                                let event_ref = EventRef {
                                    reason: ev.reason.clone(),
                                    message: ev.message.clone(),
                                    event_type: ev.event_type.clone(),
                                    count: ev.count,
                                };
                                let scope = AgentScope::Event { resource: resource_ref, event: event_ref };
                                cx.emit(SendEventDirectToAgent(scope));
                            }),
                        )
                        .child(Label::new("Send to Agent").text_xs().text_color(ACCENT_FG)),
                ),
        )
}
