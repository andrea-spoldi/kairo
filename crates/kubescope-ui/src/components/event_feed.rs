use std::collections::VecDeque;

use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kubescope_core::models::ClusterEvent;

use crate::theme::{
    BORDER, STATUS_FAILED, SURFACE, TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

const MAX_EVENTS: usize = 100;

/// Center-panel tab — live feed of cluster-wide Warning events.
pub struct EventFeedPanel {
    focus_handle: FocusHandle,
    events: VecDeque<ClusterEvent>,
}

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

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Render for EventFeedPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.events.len();
        let events: Vec<ClusterEvent> = self.events.iter().cloned().collect();

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
                        div()
                            .flex()
                            .flex_col()
                            .p_2()
                            .gap_2()
                            .children(events.iter().map(render_event_card))
                            .into_any_element()
                    }),
            )
    }
}

// ── Card renderer ─────────────────────────────────────────────────────────────

fn render_event_card(ev: &ClusterEvent) -> impl IntoElement {
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
        // ── Reason + count ────────────────────────────────────────────────────
        .child(
            h_flex()
                .gap_2()
                .child(
                    Label::new(ev.reason.clone())
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(STATUS_FAILED),
                )
                .child(div().flex_1())
                .child(
                    Label::new(format!("{}×", ev.count))
                        .text_sm()
                        .text_color(TEXT_MUTED),
                ),
        )
        // ── Namespace / object name ───────────────────────────────────────────
        .child(
            h_flex()
                .gap_1()
                .child(
                    Label::new(ev.namespace.clone())
                        .text_sm()
                        .text_color(TEXT_SECONDARY),
                )
                .child(Label::new("/").text_sm().text_color(TEXT_MUTED))
                .child(
                    Label::new(ev.object_name.clone())
                        .text_sm()
                        .text_color(TEXT_PRIMARY),
                ),
        )
        // ── Message ───────────────────────────────────────────────────────────
        .child(
            Label::new(ev.message.clone())
                .text_sm()
                .text_color(TEXT_MUTED),
        )
}
