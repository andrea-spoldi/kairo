use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kubescope_core::models::PodDetail;

use crate::theme::{status_color, STATUS_FAILED};

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
                .text_color(hsla(0., 0., 0.5, 1.))
                .child("Select a pod to view details")
                .into_any_element();
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scrollbar()
            .p_3()
            .gap_3()
            .child(render_header(&detail))
            .child(render_metadata(&detail))
            .child(render_containers(&detail))
            .child(render_events(&detail))
            .into_any_element()
    }
}

// ── Section renderers (free functions to avoid &mut self borrow conflicts) ───

fn render_header(detail: &PodDetail) -> AnyElement {
    let s = &detail.summary;
    let color = status_color(&s.status);

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(Label::new(s.name.clone()).text_size(rems(1.1)))
        .child(
            h_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_1()
                        .child(div().w_2().h_2().rounded_full().bg(color))
                        .child(Label::new(s.status.clone()).text_size(rems(0.8))),
                )
                .child(Label::new(format!("ns: {}", s.namespace)).text_size(rems(0.8)))
                .child(Label::new(format!("age: {}", s.age)).text_size(rems(0.8)))
                .child(Label::new(format!("node: {}", s.node)).text_size(rems(0.8))),
        )
        .child(
            h_flex()
                .gap_2()
                .child(Label::new(format!("Ready: {}", s.ready)).text_size(rems(0.8)))
                .child(Label::new(format!("Restarts: {}", s.restarts)).text_size(rems(0.8))),
        )
        .into_any_element()
}

fn render_metadata(detail: &PodDetail) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(render_kv_section("Labels", &detail.labels))
        .child(render_kv_section("Annotations", &detail.annotations))
        .into_any_element()
}

fn render_kv_section(title: &str, map: &std::collections::BTreeMap<String, String>) -> AnyElement {
    let mut section = div().flex().flex_col().gap_1().child(
        Label::new(title.to_string())
            .text_size(rems(0.85))
            .text_color(hsla(0., 0., 0.7, 1.)),
    );

    if map.is_empty() {
        section = section.child(
            Label::new("  (none)")
                .text_size(rems(0.75))
                .text_color(hsla(0., 0., 0.45, 1.)),
        );
    } else {
        for (k, v) in map {
            section = section.child(
                Label::new(format!("  {k}={v}"))
                    .text_size(rems(0.75))
                    .text_color(hsla(0., 0., 0.55, 1.)),
            );
        }
    }

    section.into_any_element()
}

fn render_containers(detail: &PodDetail) -> AnyElement {
    let mut section = div().flex().flex_col().gap_1().child(
        Label::new("Containers")
            .text_size(rems(0.85))
            .text_color(hsla(0., 0., 0.7, 1.)),
    );

    for c in &detail.containers {
        let state_color = match c.state.as_str() {
            "Running" => status_color("Running"),
            _ => STATUS_FAILED,
        };

        section = section.child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .px_2()
                .py_1()
                .rounded(px(4.))
                .border_1()
                .border_color(hsla(0., 0., 0.2, 1.))
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            div()
                                .w(px(6.))
                                .h(px(6.))
                                .rounded_full()
                                .bg(state_color),
                        )
                        .child(Label::new(c.name.clone()).text_size(rems(0.8))),
                )
                .child(
                    Label::new(c.image.clone())
                        .text_size(rems(0.7))
                        .text_color(hsla(0., 0., 0.5, 1.)),
                )
                .child(
                    h_flex().gap_2().child(
                        Label::new(format!(
                            "ready: {} | restarts: {} | {}",
                            c.ready, c.restart_count, c.state
                        ))
                        .text_size(rems(0.7))
                        .text_color(hsla(0., 0., 0.5, 1.)),
                    ),
                ),
        );
    }

    section.into_any_element()
}

fn render_events(detail: &PodDetail) -> AnyElement {
    let mut section = div().flex().flex_col().gap_1().child(
        Label::new("Events")
            .text_size(rems(0.85))
            .text_color(hsla(0., 0., 0.7, 1.)),
    );

    if detail.events.is_empty() {
        section = section.child(
            Label::new("  No events")
                .text_size(rems(0.75))
                .text_color(hsla(0., 0., 0.45, 1.)),
        );
    } else {
        for ev in &detail.events {
            let color = if ev.event_type == "Warning" {
                STATUS_FAILED
            } else {
                hsla(0., 0., 0.55, 1.)
            };

            section = section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .px_2()
                    .py_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Label::new(ev.event_type.clone())
                                    .text_size(rems(0.7))
                                    .text_color(color),
                            )
                            .child(Label::new(ev.reason.clone()).text_size(rems(0.75)))
                            .child(
                                Label::new(format!("x{}", ev.count))
                                    .text_size(rems(0.7))
                                    .text_color(hsla(0., 0., 0.5, 1.)),
                            ),
                    )
                    .child(
                        Label::new(ev.message.clone())
                            .text_size(rems(0.7))
                            .text_color(hsla(0., 0., 0.5, 1.)),
                    ),
            );
        }
    }

    section.into_any_element()
}
