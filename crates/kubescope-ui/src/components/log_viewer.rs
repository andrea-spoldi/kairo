use gpui::*;
use gpui_component::button::Button;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;

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
}

impl Render for LogViewerPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paused = self.paused;
        let line_count = self.lines.len();
        // Clone is cheap — SharedString wraps an Arc.
        let lines = self.lines.clone();

        div().size_full().flex().flex_col()
            // ── Toolbar ───────────────────────────────────────────────────────
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .border_b_1()
                    .border_color(gpui::rgb(0x3a3a3a))
                    .flex_shrink_0()
                    .child(
                        Label::new(
                            self.pod_label
                                .clone()
                                .map(SharedString::from)
                                .unwrap_or_else(|| SharedString::from("No pod selected")),
                        )
                        .text_sm(),
                    )
                    .child(div().flex_1())
                    // Container tabs
                    .children(self.containers.iter().enumerate().map(|(ix, name)| {
                        let selected = ix == self.selected_container_ix;
                        div()
                            .id(("container-tab", ix))
                            .px_2()
                            .py_px()
                            .rounded_md()
                            .text_sm()
                            .cursor_pointer()
                            .bg(if selected {
                                gpui::rgb(0x3a3a8c)
                            } else {
                                gpui::rgb(0x2a2a2a)
                            })
                            .child(name.clone())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.select_container(ix, cx);
                            }))
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
            // ── Log lines ─────────────────────────────────────────────────────
            .child(
                uniform_list(
                    "log-lines",
                    line_count,
                    move |range, _window, _cx| {
                        range
                            .map(|ix| {
                                div()
                                    .font_family("monospace")
                                    .text_xs()
                                    .text_color(gpui::rgb(0xd4d4d4))
                                    .child(lines[ix].clone())
                            })
                            .collect()
                    },
                )
                .flex_1()
                .w_full()
                .track_scroll(&self.scroll_handle),
            )
    }
}
