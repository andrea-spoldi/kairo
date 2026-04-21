use std::collections::HashSet;
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;

use crate::scope::LogRef;
use crate::theme::{ACCENT, BORDER, HOVER_BG, SELECTED_BG, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY};

const MAX_LINES: usize = 5_000;

/// Emitted when the user clicks "Send N lines to Agent" in the log toolbar.
#[derive(Clone, Debug)]
pub struct SendLogToAgent;

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
    /// True for 1.5 s after the user clicks "Copy Logs".
    copied_flash: bool,
    /// Indices of lines the user has selected for "Send to Agent".
    selected_lines: HashSet<usize>,
    /// Last clicked line index — used for shift+click range selection.
    last_clicked: Option<usize>,
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
            copied_flash: false,
            selected_lines: HashSet::new(),
            last_clicked: None,
        }
    }

    fn trigger_copy_flash(&mut self, cx: &mut Context<Self>) {
        self.copied_flash = true;
        cx.notify();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this: WeakEntity<LogViewerPanel>, cx| {
            executor.timer(Duration::from_millis(1500)).await;
            this.update(cx, |panel: &mut LogViewerPanel, cx| {
                panel.copied_flash = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
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
        self.selected_lines.clear();
        self.last_clicked = None;
        cx.notify();
    }

    /// Toggle selection of a single line (or a range on shift+click).
    fn toggle_line(&mut self, ix: usize, shift: bool, cx: &mut Context<Self>) {
        if shift {
            let anchor = self.last_clicked.unwrap_or(ix);
            let (lo, hi) = if anchor <= ix { (anchor, ix) } else { (ix, anchor) };
            let all_selected = (lo..=hi).all(|i| self.selected_lines.contains(&i));
            if all_selected {
                for i in lo..=hi { self.selected_lines.remove(&i); }
            } else {
                for i in lo..=hi { self.selected_lines.insert(i); }
            }
        } else if self.selected_lines.contains(&ix) {
            self.selected_lines.remove(&ix);
        } else {
            self.selected_lines.insert(ix);
        }
        self.last_clicked = Some(ix);
        cx.notify();
    }

    /// Build a log context snapshot for the AI agent from the currently selected lines.
    pub fn build_log_context(&self) -> LogRef {
        let mut sorted: Vec<usize> = self.selected_lines.iter().cloned().collect();
        sorted.sort_unstable();

        let selected: Vec<String> = sorted.iter()
            .take(100)
            .filter_map(|&i| self.lines.get(i).map(|s| s.as_ref().to_string()))
            .collect();

        let (lo, hi) = sorted.iter().fold(
            (usize::MAX, 0usize),
            |(lo, hi), &i| (lo.min(i), hi.max(i)),
        );
        let window_start = lo.saturating_sub(25).min(self.lines.len());
        let window_end = (hi + 25).min(self.lines.len().saturating_sub(1));
        let window: Vec<String> = if !selected.is_empty() {
            (window_start..=window_end)
                .filter_map(|i| self.lines.get(i).map(|s| s.as_ref().to_string()))
                .collect()
        } else {
            vec![]
        };

        let (pod, namespace) = if let Some(label) = &self.pod_label {
            let mut parts = label.splitn(2, '/');
            match (parts.next(), parts.next()) {
                (Some(ns), Some(p)) => (Some(p.to_string()), Some(ns.to_string())),
                (Some(p), None)     => (Some(p.to_string()), None),
                _                    => (None, None),
            }
        } else {
            (None, None)
        };
        let container = self.containers.get(self.selected_container_ix).cloned();

        LogRef { selected, window, pod, container, namespace }
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
        self.selected_lines.clear();
        self.last_clicked = None;
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
        self.selected_lines.clear();
        self.last_clicked = None;
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
impl EventEmitter<SendLogToAgent> for LogViewerPanel {}

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
        let copied = self.copied_flash;
        let has_lines = !self.lines.is_empty();
        let line_count = self.lines.len();
        let selected_count = self.selected_lines.len();
        // Clone is cheap — SharedString wraps an Arc.
        let lines = self.lines.clone();
        let selected_lines = self.selected_lines.clone();

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

        // Weak handle used inside the uniform_list callback for line click handling.
        let weak = cx.entity().downgrade();

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
                    // "Send N lines to Agent" — only visible when lines are selected.
                    .when(selected_count > 0, |row| {
                        row.child(
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
                                        this.selected_lines.clear();
                                        this.last_clicked = None;
                                        cx.emit(SendLogToAgent);
                                        cx.notify();
                                    }),
                                )
                                .child(
                                    Label::new(SharedString::from(format!(
                                        "⬡ Send {selected_count} lines to Agent"
                                    )))
                                    .text_xs()
                                    .text_color(ACCENT),
                                ),
                        )
                    })
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
                    )
                    // Copy Logs
                    .when(has_lines, |row| {
                        row.child(
                            Button::new("copy-logs")
                                .label(if copied { "Copied!" } else { "Copy Logs" })
                                .compact()
                                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                                    let text = this
                                        .lines
                                        .iter()
                                        .map(|s| s.as_ref())
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                                    this.trigger_copy_flash(cx);
                                })),
                        )
                    }),
            )
            // ── Log lines — monospace text_sm, click to select for Agent ──────
            .child(
                uniform_list(
                    "log-lines",
                    line_count,
                    move |range, _window, _cx| {
                        range
                            .map(|ix| {
                                let is_selected = selected_lines.contains(&ix);
                                let w = weak.clone();
                                div()
                                    .id(("log-line", ix))
                                    .font_family("monospace")
                                    .text_sm()
                                    .text_color(TEXT_PRIMARY)
                                    .px_3()
                                    .whitespace_nowrap()
                                    .cursor_pointer()
                                    .when(is_selected, |el| el.bg(SELECTED_BG))
                                    .when(!is_selected, |el| el.hover(|s| s.bg(HOVER_BG)))
                                    .on_click(move |ev, _, cx| {
                                        let shift = ev.modifiers().shift;
                                        w.update(cx, |panel, cx| {
                                            panel.toggle_line(ix, shift, cx);
                                        })
                                        .ok();
                                    })
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
