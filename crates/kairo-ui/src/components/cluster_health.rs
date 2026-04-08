use std::collections::{BTreeMap, VecDeque};

use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kairo_core::models::{ClusterEvent, PodSummary};

use crate::theme::{
    BORDER, HOVER_BG, SELECTED_BG, STATUS_FAILED, STATUS_PENDING, STATUS_RUNNING,
    SURFACE, TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

const MAX_WARNINGS: usize = 12;

/// Emitted when the user clicks a namespace row in the sidebar.
#[derive(Clone)]
pub struct SidebarNamespaceSelected {
    /// `None` means "All namespaces".
    pub namespace: Option<String>,
}

/// Aggregated pod counts for a scope (cluster or namespace).
#[derive(Default, Clone)]
struct PodCounts {
    running: usize,
    pending: usize,
    failed: usize,
    total: usize,
}

/// Left sidebar panel — cluster health overview (pod counts + namespace tree + recent warnings).
pub struct ClusterHealthPanel {
    focus_handle: FocusHandle,
    cluster: PodCounts,
    namespaces: BTreeMap<String, PodCounts>,
    warnings: VecDeque<ClusterEvent>,
    active_namespace: SharedString,
}

impl ClusterHealthPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            cluster: PodCounts::default(),
            namespaces: BTreeMap::new(),
            warnings: VecDeque::new(),
            active_namespace: SharedString::from("All"),
        }
    }

    /// Recompute all pod counts from the current pod list.
    pub fn update_pods(&mut self, pods: &[PodSummary], cx: &mut Context<Self>) {
        self.cluster = PodCounts::default();
        self.namespaces.clear();

        for pod in pods {
            self.cluster.total += 1;
            let ns = self.namespaces.entry(pod.namespace.clone()).or_default();
            ns.total += 1;

            match pod.status.as_str() {
                "Running" => {
                    self.cluster.running += 1;
                    ns.running += 1;
                }
                "Pending" | "ContainerCreating" | "Initializing" => {
                    self.cluster.pending += 1;
                    ns.pending += 1;
                }
                _ => {
                    self.cluster.failed += 1;
                    ns.failed += 1;
                }
            }
        }

        cx.notify();
    }

    /// Add a warning event to the recent list (newest first, capped at MAX_WARNINGS).
    pub fn push_warning(&mut self, ev: ClusterEvent, cx: &mut Context<Self>) {
        self.warnings.push_front(ev);
        if self.warnings.len() > MAX_WARNINGS {
            self.warnings.pop_back();
        }
        cx.notify();
    }

    /// Update the highlighted active namespace (driven by external namespace selection).
    pub fn set_active_namespace(&mut self, ns: SharedString, cx: &mut Context<Self>) {
        self.active_namespace = ns;
        cx.notify();
    }

    /// Reset all counts and warnings (e.g. on context switch).
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.cluster = PodCounts::default();
        self.namespaces.clear();
        self.warnings.clear();
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for ClusterHealthPanel {}
impl EventEmitter<SidebarNamespaceSelected> for ClusterHealthPanel {}

impl Focusable for ClusterHealthPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ClusterHealthPanel {
    fn panel_name(&self) -> &'static str {
        "ClusterHealthPanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Cluster"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Render for ClusterHealthPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_ns = self.active_namespace.clone();

        // ── Health counts strip ───────────────────────────────────────────────
        let counts = render_health_counts(&self.cluster);

        // ── Namespace tree ────────────────────────────────────────────────────
        let mut ns_section = div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(
                Label::new("Namespaces")
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(TEXT_HEADING),
            );

        // "All" row
        let all_selected = active_ns.as_ref() == "All";
        let all_listener = cx.listener(|this, _: &ClickEvent, _window, cx| {
            this.active_namespace = SharedString::from("All");
            cx.emit(SidebarNamespaceSelected { namespace: None });
            cx.notify();
        });
        let all_counts = self.cluster.clone();
        let all_row = if all_selected {
            div()
                .id("ns-all")
                .px_2()
                .py(px(2.))
                .rounded_md()
                .cursor_pointer()
                .bg(SELECTED_BG)
                .on_click(all_listener)
                .child(render_ns_row_content("All", &all_counts, true))
                .into_any_element()
        } else {
            div()
                .id("ns-all")
                .px_2()
                .py(px(2.))
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(HOVER_BG))
                .on_click(all_listener)
                .child(render_ns_row_content("All", &all_counts, false))
                .into_any_element()
        };
        ns_section = ns_section.child(all_row);

        // Per-namespace rows
        let ns_entries: Vec<(String, PodCounts)> = self
            .namespaces
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        for (ix, (ns_name, counts)) in ns_entries.iter().enumerate() {
            let selected = active_ns.as_ref() == ns_name.as_str();
            let ns_for_listener = ns_name.clone();
            let listener = cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.active_namespace = SharedString::from(ns_for_listener.clone());
                cx.emit(SidebarNamespaceSelected {
                    namespace: Some(ns_for_listener.clone()),
                });
                cx.notify();
            });
            let row_content = render_ns_row_content(ns_name, counts, selected);
            let row = if selected {
                div()
                    .id(("ns-row", ix))
                    .px_2()
                    .py(px(2.))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(SELECTED_BG)
                    .on_click(listener)
                    .child(row_content)
                    .into_any_element()
            } else {
                div()
                    .id(("ns-row", ix))
                    .px_2()
                    .py(px(2.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(HOVER_BG))
                    .on_click(listener)
                    .child(row_content)
                    .into_any_element()
            };
            ns_section = ns_section.child(row);
        }

        // ── Recent warnings ───────────────────────────────────────────────────
        let warnings_section = render_warnings_section(&self.warnings);

        // ── Assemble ──────────────────────────────────────────────────────────
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scrollbar()
            .p_3()
            .gap_4()
            .child(counts)
            .child(ns_section)
            .child(warnings_section)
    }
}

// ── Section helpers ───────────────────────────────────────────────────────────

fn render_health_counts(c: &PodCounts) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            Label::new("Pod Health")
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT_HEADING),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .px_2()
                .py_2()
                .rounded(px(6.))
                .border_1()
                .border_color(BORDER)
                .bg(SURFACE)
                .child(count_row("● Running", c.running, STATUS_RUNNING))
                .child(count_row("◐ Pending", c.pending, STATUS_PENDING))
                .child(count_row("✖ Failed", c.failed, STATUS_FAILED))
                .child(
                    h_flex()
                        .justify_between()
                        .child(
                            Label::new("Total")
                                .text_sm()
                                .text_color(TEXT_MUTED),
                        )
                        .child(
                            Label::new(c.total.to_string())
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(TEXT_SECONDARY),
                        ),
                ),
        )
        .into_any_element()
}

fn count_row(label: &str, n: usize, color: gpui::Hsla) -> impl IntoElement {
    h_flex()
        .justify_between()
        .child(Label::new(label.to_string()).text_sm().text_color(color))
        .child(
            Label::new(n.to_string())
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(color),
        )
}

fn render_ns_row_content(name: &str, counts: &PodCounts, selected: bool) -> impl IntoElement {
    let text_color = if selected { TEXT_PRIMARY } else { TEXT_SECONDARY };
    h_flex()
        .justify_between()
        .child(
            Label::new(name.to_string())
                .text_sm()
                .text_color(text_color),
        )
        .child(
            h_flex()
                .gap_1()
                .child(
                    Label::new(counts.running.to_string())
                        .text_sm()
                        .text_color(STATUS_RUNNING),
                )
                .child(Label::new("/").text_sm().text_color(TEXT_MUTED))
                .child(
                    Label::new(counts.total.to_string())
                        .text_sm()
                        .text_color(TEXT_MUTED),
                ),
        )
}

fn render_warnings_section(warnings: &VecDeque<ClusterEvent>) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            Label::new("Recent Warnings")
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT_HEADING),
        );

    if warnings.is_empty() {
        section = section.child(
            Label::new("  No warnings")
                .text_sm()
                .text_color(TEXT_MUTED),
        );
    } else {
        for ev in warnings {
            section = section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .border_l_2()
                    .border_color(STATUS_FAILED)
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Label::new(ev.reason.clone())
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(STATUS_FAILED),
                            )
                            .child(
                                Label::new(ev.object_name.clone())
                                    .text_sm()
                                    .text_color(TEXT_SECONDARY),
                            ),
                    )
                    .child(
                        Label::new(ev.message.clone())
                            .text_sm()
                            .text_color(TEXT_MUTED),
                    ),
            );
        }
    }

    section.into_any_element()
}

