use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kairo_core::models::PodDetail;

use crate::theme::{
    status_color, status_symbol, BORDER, STATUS_FAILED, SURFACE,
    TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

/// Right panel — pod detail view.
pub struct PodDetailPanel {
    focus_handle: FocusHandle,
    detail: Option<PodDetail>,
}

impl PodDetailPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            detail: None,
        }
    }

    pub fn set_detail(&mut self, detail: PodDetail) {
        self.detail = Some(detail);
    }

    pub fn clear_detail(&mut self) {
        self.detail = None;
    }
}

impl EventEmitter<PanelEvent> for PodDetailPanel {}

impl Focusable for PodDetailPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PodDetailPanel {
    fn panel_name(&self) -> &'static str {
        "PodDetailPanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Pod Detail"
    }
}

impl Render for PodDetailPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(detail) = self.detail.clone() else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(TEXT_MUTED)
                .child("Select a pod to view details")
                .into_any_element();
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scrollbar()
            .p_4()
            .gap_4()
            .child(render_header(&detail))
            .child(render_metadata(&detail))
            .child(render_containers(&detail))
            .child(render_events(&detail))
            .into_any_element()
    }
}

// ── Section renderers ─────────────────────────────────────────────────────────

fn render_header(detail: &PodDetail) -> AnyElement {
    let s = &detail.summary;
    let color = status_color(&s.status);
    let symbol = status_symbol(&s.status);

    div()
        .flex()
        .flex_col()
        .gap_2()
        // Pod name — largest text, semibold, truncated if necessary
        .child(
            div()
                .w_full()
                .overflow_hidden()
                .child(
                    Label::new(s.name.clone())
                        .text_size(rems(1.2))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(TEXT_PRIMARY),
                ),
        )
        // Status row — colored symbol + text
        .child(
            h_flex()
                .gap(px(6.))
                .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(color))
                .child(
                    Label::new(format!("{symbol} {}", s.status))
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color),
                ),
        )
        // Metadata row — namespace / age / node
        .child(
            h_flex()
                .gap_3()
                .flex_wrap()
                .child(Label::new(format!("ns: {}", s.namespace)).text_sm().text_color(TEXT_SECONDARY))
                .child(Label::new(format!("age: {}", s.age)).text_sm().text_color(TEXT_MUTED))
                .child(Label::new(format!("node: {}", s.node)).text_sm().text_color(TEXT_SECONDARY)),
        )
        // Ready / restarts row
        .child(
            h_flex()
                .gap_3()
                .child(Label::new(format!("Ready: {}", s.ready)).text_sm().text_color(TEXT_SECONDARY))
                .child(Label::new(format!("Restarts: {}", s.restarts)).text_sm().text_color(TEXT_SECONDARY)),
        )
        .into_any_element()
}

fn render_metadata(detail: &PodDetail) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(render_kv_section("Labels", &detail.labels))
        .child(render_kv_section("Annotations", &detail.annotations))
        .into_any_element()
}

fn render_kv_section(title: &str, map: &std::collections::BTreeMap<String, String>) -> AnyElement {
    let mut section = div().flex().flex_col().gap_1().child(
        Label::new(title.to_string())
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(TEXT_HEADING),
    );

    if map.is_empty() {
        section = section.child(
            Label::new("  (none)")
                .text_sm()
                .text_color(TEXT_MUTED),
        );
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

fn render_containers(detail: &PodDetail) -> AnyElement {
    let mut section = div().flex().flex_col().gap_2().child(
        Label::new("Containers")
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(TEXT_HEADING),
    );

    for c in &detail.containers {
        let state_color = status_color(&c.state);
        let state_symbol = status_symbol(&c.state);

        section = section.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .px_3()
                .py_2()
                .rounded(px(6.))
                .border_1()
                .border_color(BORDER)
                .bg(SURFACE)
                // Container name + state dot
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
                // Image
                .child(
                    Label::new(c.image.clone())
                        .text_sm()
                        .text_color(TEXT_MUTED),
                )
                // Stats
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

fn render_events(detail: &PodDetail) -> AnyElement {
    let mut section = div().flex().flex_col().gap_2().child(
        Label::new("Events")
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(TEXT_HEADING),
    );

    if detail.events.is_empty() {
        section = section.child(
            Label::new("  No events")
                .text_sm()
                .text_color(TEXT_MUTED),
        );
    } else {
        for ev in &detail.events {
            let type_color = if ev.event_type == "Warning" {
                STATUS_FAILED
            } else {
                TEXT_SECONDARY
            };

            section = section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px_2()
                    .py_1()
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
                            ),
                    )
                    .child(
                        Label::new(ev.message.clone())
                            .text_sm()
                            .text_color(TEXT_SECONDARY),
                    ),
            );
        }
    }

    section.into_any_element()
}
