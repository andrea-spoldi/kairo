use gpui::*;
use gpui_component::button::Button;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;

use crate::theme::{ACCENT, BORDER, HOVER_BG, SELECTED_BG, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY};

const MAX_LINES: usize = 5_000;

/// Emitted when the user selects a different container tab.
#[derive(Clone)]
pub struct ContainerSelected {
    pub name: String,
}

/// Bottom panel — streaming log viewer.
pub struct LogViewerPanel {
    focus_handle: FocusHandle,
    /// Buffered log lines (capped at [`MAX_LINES`]).
    lines: Vec<SharedString>,
    /// When paused, incoming lines are dropped.
    paused: bool,
    /// Container names for the currently displayed pod.
    containers: Vec<String>,
    /// Index into `containers` for the active container.
    selected_container_ix: usize,
    /// Drives auto-scroll to the last line.
    scroll_handle: UniformListScrollHandle,
    /// "namespace/pod-name" shown in the toolbar.
    pod_label: Option<String>,
}

impl LogViewerPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            lines: Vec::new(),
            paused: false,
            containers: Vec::new(),
            selected_container_ix: 0,
            scroll_handle: UniformListScrollHandle::new(),
            pod_label: None,
        }
    }

    /// Called by `Workspace` when a pod is selected and detail has loaded.
    pub fn set_pod(
        &mut self,
        pod_name: String,
        namespace: String,
        containers: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.pod_label = Some(format!("{namespace}/{pod_name}"));
        self.containers = containers;
        self.selected_container_ix = 0;
        self.lines.clear();
        self.paused = false;
        cx.notify();
    }

    /// Append a new log line; evicts oldest lines when at capacity.
    pub fn push_line(&mut self, line: String, cx: &mut Context<Self>) {
        if self.paused {
            return;
        }
        if self.lines.len() >= MAX_LINES {
            self.lines.drain(..256);
        }
        self.lines.push(SharedString::from(line));
        if !self.lines.is_empty() {
            self.scroll_handle
                .scroll_to_item(self.lines.len() - 1, ScrollStrategy::Bottom);
        }
        cx.notify();
    }

    /// Clear all buffered lines.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.lines.clear();
        cx.notify();
    }

    /// Reset to a "no pod selected" empty state — used when the inspector is
    /// showing a non-Pod resource (Deployment, Service, …).
    pub fn set_no_pod(&mut self, cx: &mut Context<Self>) {
        self.pod_label = None;
        self.containers.clear();
        self.selected_container_ix = 0;
        self.lines.clear();
        self.paused = false;
        cx.notify();
    }

    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        self.paused = !self.paused;
        if !self.paused && !self.lines.is_empty() {
            self.scroll_handle
                .scroll_to_item(self.lines.len() - 1, ScrollStrategy::Bottom);
        }
        cx.notify();
    }

    fn select_container(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix == self.selected_container_ix {
            return;
        }
        self.selected_container_ix = ix;
        self.lines.clear();
        self.paused = false;
        let name = self.containers[ix].clone();
        cx.emit(ContainerSelected { name });
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for LogViewerPanel {}
impl EventEmitter<ContainerSelected> for LogViewerPanel {}

impl Focusable for LogViewerPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for LogViewerPanel {
    fn panel_name(&self) -> &'static str {
        "LogViewerPanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Logs"
    }

    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> {
        None
    }
}

impl Render for LogViewerPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paused = self.paused;
        let line_count = self.lines.len();
        // Clone is cheap — SharedString wraps an Arc.
        let lines = self.lines.clone();

        if self.pod_label.is_none() {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(TEXT_MUTED)
                .child("Logs available when a Pod is selected")
                .into_any_element();
        }

        div().size_full().flex().flex_col()
            // ── Toolbar ───────────────────────────────────────────────────────
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(BORDER)
                    .flex_shrink_0()
                    // Pod label
                    .child(
                        Label::new(
                            self.pod_label
                                .clone()
                                .map(SharedString::from)
                                .unwrap_or_else(|| SharedString::from("No pod selected")),
                        )
                        .text_sm()
                        .text_color(TEXT_SECONDARY),
                    )
                    .child(div().flex_1())
                    // Container tabs — color + font-weight + bottom accent on selected; hover on unselected
                    .children(self.containers.iter().enumerate().map(|(ix, name)| {
                        let selected = ix == self.selected_container_ix;
                        let listener = cx.listener(move |this, _: &ClickEvent, _window, cx| {
                            this.select_container(ix, cx);
                        });
                        let name_child = name.clone();
                        if selected {
                            div()
                                .id(("container-tab", ix))
                                .px_3()
                                .py(px(3.))
                                .rounded_md()
                                .text_sm()
                                .cursor_pointer()
                                .bg(SELECTED_BG)
                                .text_color(TEXT_PRIMARY)
                                .font_weight(FontWeight::MEDIUM)
                                .border_b_2()
                                .border_color(ACCENT)
                                .child(name_child)
                                .on_click(listener)
                                .into_any_element()
                        } else {
                            div()
                                .id(("container-tab", ix))
                                .px_3()
                                .py(px(3.))
                                .rounded_md()
                                .text_sm()
                                .cursor_pointer()
                                .text_color(TEXT_SECONDARY)
                                .font_weight(FontWeight::NORMAL)
                                .hover(|s| s.bg(HOVER_BG))
                                .child(name_child)
                                .on_click(listener)
                                .into_any_element()
                        }
                    }))
                    // Pause / Resume
                    .child(
                        Button::new("pause-toggle")
                            .label(if paused { "Resume" } else { "Pause" })
                            .compact()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.toggle_pause(cx);
                            })),
                    )
                    // Clear
                    .child(
                        Button::new("clear-logs")
                            .label("Clear")
                            .compact()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.clear(cx);
                            })),
                    ),
            )
            // ── Log lines — monospace text_sm for comfortable reading ─────────
            .child(
                uniform_list(
                    "log-lines",
                    line_count,
                    move |range, _window, _cx| {
                        range
                            .map(|ix| {
                                div()
                                    .font_family("monospace")
                                    .text_sm()
                                    .text_color(TEXT_PRIMARY)
                                    .px_3()
                                    .whitespace_nowrap()
                                    .child(lines[ix].clone())
                            })
                            .collect()
                    },
                )
                .flex_1()
                .w_full()
                .track_scroll(&self.scroll_handle),
            )
            .into_any_element()
    }
}
