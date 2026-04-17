use std::collections::VecDeque;

use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kairo_core::models::ClusterEvent;

use crate::analyze::AnalyzeEventRequest;
use crate::theme::{
    ACCENT, BORDER, HOVER_BG, STATUS_FAILED, SURFACE, TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY,
    TEXT_SECONDARY,
};

const MAX_EVENTS: usize = 100;

/// Emitted when the user clicks an event card body. Drives the right-dock
/// inspector to show the involved resource (details, YAML, logs, stats).
#[derive(Clone)]
pub struct EventResourceSelected {
    pub kind: String,
    pub namespace: String,
    pub name: String,
}

/// Center-panel tab — live feed of cluster-wide Warning events.
pub struct EventFeedPanel {
    focus_handle: FocusHandle,
    events: VecDeque<ClusterEvent>,
}

impl EventEmitter<AnalyzeEventRequest> for EventFeedPanel {}
impl EventEmitter<EventResourceSelected> for EventFeedPanel {}

impl EventFeedPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            events: VecDeque::new(),
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
}

impl EventEmitter<PanelEvent> for EventFeedPanel {}

impl Focusable for EventFeedPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for EventFeedPanel {
    fn panel_name(&self) -> &'static str {
        "EventFeedPanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Events"
    }

    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> {
        None
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Render for EventFeedPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.events.len();
        let events: Vec<ClusterEvent> = self.events.iter().cloned().collect();

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
    let ev_clone = ev.clone();
    let json = serde_json::json!({
        "resource": format!("{} {}/{}", ev.object_kind, ev.namespace, ev.object_name),
        "kind": ev.object_kind,
        "event_type": ev.event_type,
        "reason": ev.reason,
        "message": ev.message,
        "count": ev.count,
        "last_time": ev.last_time,
    });
    let json_str = serde_json::to_string_pretty(&json).unwrap_or_default();

    // Card body click → select resource in inspector.
    let sel_kind = ev.object_kind.clone();
    let sel_ns = ev.namespace.clone();
    let sel_name = ev.object_name.clone();
    let card_id = ElementId::Name(
        format!("event-card-{}-{}-{}", ev.namespace, ev.object_name, ev.last_time).into(),
    );

    div()
        .id(card_id)
        .flex()
        .flex_col()
        .gap(px(3.))
        .px_3()
        .py_2()
        .rounded(px(6.))
        .border_1()
        .border_color(BORDER)
        .bg(SURFACE)
        .cursor_pointer()
        .hover(|s| s.bg(HOVER_BG))
        .on_click(cx.listener(move |_this, _: &ClickEvent, _window, cx| {
            if !sel_kind.is_empty() && !sel_name.is_empty() {
                cx.emit(EventResourceSelected {
                    kind: sel_kind.clone(),
                    namespace: sel_ns.clone(),
                    name: sel_name.clone(),
                });
            }
        }))
        // ── Reason + count + Analyze button ──────────────────────────────────
        .child(
            h_flex()
                .gap_2()
                .child(
                    Label::new(ev_clone.reason.clone())
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(STATUS_FAILED),
                )
                .child(div().flex_1())
                .child(
                    Label::new(format!("{}×", ev_clone.count))
                        .text_sm()
                        .text_color(TEXT_MUTED),
                )
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
                                cx.stop_propagation();
                                cx.emit(AnalyzeEventRequest(json_str.clone()));
                            }),
                        )
                        .child(Label::new("⬡ Analyze").text_xs().text_color(ACCENT)),
                ),
        )
        // ── Namespace / object name ───────────────────────────────────────────
        .child(
            h_flex()
                .gap_1()
                .child(
                    Label::new(ev_clone.namespace.clone())
                        .text_sm()
                        .text_color(TEXT_SECONDARY),
                )
                .child(Label::new("/").text_sm().text_color(TEXT_MUTED))
                .child(
                    Label::new(ev_clone.object_name.clone())
                        .text_sm()
                        .text_color(TEXT_PRIMARY),
                ),
        )
        // ── Message ───────────────────────────────────────────────────────────
        .child(
            Label::new(ev_clone.message.clone())
                .text_sm()
                .text_color(TEXT_MUTED),
        )
}
