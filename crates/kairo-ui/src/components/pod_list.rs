use std::collections::BTreeMap;

use gpui::*;
use gpui_component::button::Button;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::label::Label;
use gpui_component::select::{Select, SelectEvent, SelectState};
use gpui_component::table::{Column, DataTable, TableDelegate, TableEvent, TableState};
use kairo_core::models::PodSummary;

use crate::actions::{ConfirmSelection, FocusSearch, NavigateDown, NavigateUp, ToggleGrouping};
use crate::theme::{
    status_color, status_symbol, BORDER, SELECTED_BG, SURFACE, TEXT_HEADING, TEXT_MUTED,
    TEXT_PRIMARY, TEXT_SECONDARY,
};

/// Emitted when the user selects a pod row (click or keyboard Enter).
#[derive(Clone)]
pub struct PodSelected {
    pub name: String,
    pub namespace: String,
}

// ── Row type ──────────────────────────────────────────────────────────────────

/// A visual row in the table: either a controller group header or a pod row.
#[derive(Clone, Debug)]
enum RowType {
    /// Collapsible group header for a controller (kind + name).
    GroupHeader {
        kind: String,
        name: String,
        running: usize,
        total: usize,
    },
    /// Pod row — carries the index into `PodTableDelegate::pods`.
    PodRow(usize),
}

// ── Column definitions ────────────────────────────────────────────────────────

const COLUMNS: &[(&str, &str, f32)] = &[
    ("name",      "Name",      200.),
    ("namespace", "Namespace", 120.),
    ("status",    "Status",    160.),
    ("ready",     "Ready",      70.),
    ("restarts",  "Restarts",   80.),
    ("age",       "Age",        70.),
    ("node",      "Node",      180.),
];

fn status_filter_items() -> Vec<SharedString> {
    ["All", "Running", "Pending", "Failed", "Succeeded", "CrashLoopBackOff", "Terminating"]
        .iter()
        .map(|s| SharedString::from(*s))
        .collect()
}

// ── TableDelegate ─────────────────────────────────────────────────────────────

pub struct PodTableDelegate {
    /// Filtered pod list (flat, used for pod indexing).
    pub pods: Vec<PodSummary>,
    columns: Vec<Column>,
    /// Visual row sequence — may include GroupHeader entries when grouped.
    rows: Vec<RowType>,
    /// Visual row index of the keyboard cursor (`None` = no cursor).
    cursor_row: Option<usize>,
}

impl PodTableDelegate {
    pub fn new() -> Self {
        let columns = COLUMNS
            .iter()
            .map(|(k, n, w)| Column::new(*k, *n).width(*w))
            .collect();
        Self {
            pods: vec![],
            columns,
            rows: vec![],
            cursor_row: None,
        }
    }

    /// Recompute `rows` from the current `pods` list.
    fn rebuild_rows(&mut self, grouped: bool) {
        if !grouped || self.pods.is_empty() {
            self.rows = (0..self.pods.len()).map(RowType::PodRow).collect();
            return;
        }

        // Group pods by (owner_kind, owner_name). Empty owner → "Standalone".
        let mut groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
        for (i, pod) in self.pods.iter().enumerate() {
            let key = if pod.owner_name.is_empty() {
                ("".to_string(), "Standalone".to_string())
            } else {
                (pod.owner_kind.clone(), pod.owner_name.clone())
            };
            groups.entry(key).or_default().push(i);
        }

        let mut rows = Vec::new();
        for ((kind, name), indices) in &groups {
            let running = indices
                .iter()
                .filter(|&&i| self.pods[i].status == "Running")
                .count();
            rows.push(RowType::GroupHeader {
                kind: kind.clone(),
                name: name.clone(),
                running,
                total: indices.len(),
            });
            for &pod_ix in indices {
                rows.push(RowType::PodRow(pod_ix));
            }
        }
        self.rows = rows;
    }

    /// Find the visual row index for a given pod index.
    fn row_for_pod(&self, pod_ix: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| matches!(r, RowType::PodRow(i) if *i == pod_ix))
    }

    /// The pod index stored at a visual row, if it's a PodRow.
    fn pod_ix_at_row(&self, row_ix: usize) -> Option<usize> {
        match self.rows.get(row_ix)? {
            RowType::PodRow(pod_ix) => Some(*pod_ix),
            RowType::GroupHeader { .. } => None,
        }
    }
}

impl TableDelegate for PodTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, ix: usize, _: &App) -> Column {
        self.columns[ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let is_cursor = self.cursor_row == Some(row_ix);

        match self.rows.get(row_ix) {
            // ── Group header ──────────────────────────────────────────────────
            Some(RowType::GroupHeader { kind, name, running, total }) => {
                let (kind, name, running, total) =
                    (kind.clone(), name.clone(), *running, *total);
                if col_ix == 0 {
                    h_flex()
                        .size_full()
                        .bg(SURFACE)
                        .px_2()
                        .gap_2()
                        .child(
                            Label::new(format!("▶  {kind} · {name}"))
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(TEXT_HEADING),
                        )
                        .child(div().flex_1())
                        .child(
                            Label::new(format!("{running}/{total}"))
                                .text_sm()
                                .text_color(TEXT_MUTED),
                        )
                        .into_any_element()
                } else {
                    div().size_full().bg(SURFACE).into_any_element()
                }
            }

            // ── Pod row ───────────────────────────────────────────────────────
            Some(RowType::PodRow(pod_ix)) => {
                let Some(pod) = self.pods.get(*pod_ix) else {
                    return div().into_any_element();
                };
                let bg = if is_cursor { SELECTED_BG } else { gpui::transparent_black() };
                let key = self.columns[col_ix].key.as_ref();
                match key {
                    "name" => div()
                        .size_full()
                        .bg(bg)
                        .px(px(4.))
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(TEXT_PRIMARY)
                        .child(pod.name.clone())
                        .into_any_element(),
                    "namespace" => div()
                        .size_full()
                        .bg(bg)
                        .px(px(4.))
                        .text_color(TEXT_SECONDARY)
                        .child(pod.namespace.clone())
                        .into_any_element(),
                    "status" => {
                        let color = status_color(&pod.status);
                        let symbol = status_symbol(&pod.status);
                        h_flex()
                            .size_full()
                            .bg(bg)
                            .px(px(4.))
                            .gap(px(6.))
                            .child(
                                Label::new(format!("{symbol} {}", pod.status))
                                    .text_sm()
                                    .text_color(color)
                                    .font_weight(FontWeight::MEDIUM),
                            )
                            .into_any_element()
                    }
                    "ready" => div()
                        .size_full()
                        .bg(bg)
                        .px(px(4.))
                        .text_color(TEXT_SECONDARY)
                        .child(pod.ready.clone())
                        .into_any_element(),
                    "restarts" => div()
                        .size_full()
                        .bg(bg)
                        .px(px(4.))
                        .text_color(TEXT_SECONDARY)
                        .child(pod.restarts.to_string())
                        .into_any_element(),
                    "age" => div()
                        .size_full()
                        .bg(bg)
                        .px(px(4.))
                        .text_color(TEXT_MUTED)
                        .child(pod.age.clone())
                        .into_any_element(),
                    "node" => div()
                        .size_full()
                        .bg(bg)
                        .px(px(4.))
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(TEXT_SECONDARY)
                        .child(pod.node.clone())
                        .into_any_element(),
                    _ => div().size_full().bg(bg).into_any_element(),
                }
            }

            None => div().into_any_element(),
        }
    }
}

// ── PodListPanel ──────────────────────────────────────────────────────────────

/// Center panel — virtualized pod list with search toolbar, grouping, and keyboard nav.
pub struct PodListPanel {
    pub table: Entity<TableState<PodTableDelegate>>,
    focus_handle: FocusHandle,
    /// Namespace-filtered pods from `Workspace`; search filters apply on top.
    source_pods: Vec<PodSummary>,
    name_input: Entity<InputState>,
    label_input: Entity<InputState>,
    status_select: Entity<SelectState<Vec<SharedString>>>,
    /// Keyboard cursor into the flat `pods` list of the delegate.
    cursor: Option<usize>,
    /// Whether to group pods by their owning controller.
    grouped: bool,
}

impl PodListPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = PodTableDelegate::new();
        let table = cx.new(|cx| TableState::new(delegate, window, cx));

        cx.subscribe_in(
            &table,
            window,
            |this, _, event: &TableEvent, _window, cx| {
                if let TableEvent::SelectRow(row_ix) = event {
                    let result = {
                        let t = this.table.read(cx);
                        let d = t.delegate();
                        d.pod_ix_at_row(*row_ix)
                            .and_then(|pod_ix| {
                                d.pods.get(pod_ix).map(|p| (pod_ix, p.name.clone(), p.namespace.clone()))
                            })
                    };
                    if let Some((pod_ix, name, namespace)) = result {
                        this.cursor = Some(pod_ix);
                        this.table.update(cx, |t, _| {
                            t.delegate_mut().cursor_row = Some(*row_ix);
                        });
                        cx.emit(PodSelected { name, namespace });
                    }
                }
            },
        )
        .detach();

        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter by name…"));
        let label_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("app=nginx"));
        let status_select = cx.new(|cx| {
            SelectState::new(
                status_filter_items(),
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            )
        });

        cx.subscribe_in(&name_input, window, |this, _, _: &InputEvent, _window, cx| {
            this.apply_filters(cx);
        })
        .detach();

        cx.subscribe_in(&label_input, window, |this, _, _: &InputEvent, _window, cx| {
            this.apply_filters(cx);
        })
        .detach();

        cx.subscribe_in(
            &status_select,
            window,
            |this, _, _: &SelectEvent<Vec<SharedString>>, _window, cx| {
                this.apply_filters(cx);
            },
        )
        .detach();

        Self {
            table,
            focus_handle: cx.focus_handle(),
            source_pods: Vec::new(),
            name_input,
            label_input,
            status_select,
            cursor: None,
            grouped: false,
        }
    }

    /// Replace the source pod list and re-apply current filters.
    pub fn set_pods(&mut self, pods: Vec<PodSummary>, cx: &mut Context<Self>) {
        self.source_pods = pods;
        self.apply_filters(cx);
    }

    fn apply_filters(&mut self, cx: &mut Context<Self>) {
        let name_query = self.name_input.read(cx).value().to_lowercase();
        let label_query = self.label_input.read(cx).value().to_string();
        let status_value = self
            .status_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| SharedString::from("All"));

        let filtered: Vec<PodSummary> = self
            .source_pods
            .iter()
            .filter(|p| {
                if !name_query.is_empty() && !p.name.to_lowercase().contains(&name_query) {
                    return false;
                }
                if status_value.as_ref() != "All" && p.status != status_value.as_ref() {
                    return false;
                }
                if !label_query.is_empty() {
                    if let Some((k, v)) = label_query.split_once('=') {
                        if p.labels.get(k).map(String::as_str) != Some(v) {
                            return false;
                        }
                    } else if !p.labels.contains_key(label_query.as_str()) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Reset cursor if it's out of range.
        if self.cursor.is_some_and(|c| c >= filtered.len()) {
            self.cursor = None;
        }

        let grouped = self.grouped;
        let cursor = self.cursor;

        self.table.update(cx, |t, _| {
            let d = t.delegate_mut();
            d.pods = filtered;
            d.rebuild_rows(grouped);
            // Restore visual cursor after rebuild.
            d.cursor_row = cursor.and_then(|pod_ix| d.row_for_pod(pod_ix));
        });
    }

    // ── Keyboard navigation ───────────────────────────────────────────────────

    fn navigate_down(&mut self, _: &NavigateDown, _window: &mut Window, cx: &mut Context<Self>) {
        let pod_count = self.table.read(cx).delegate().pods.len();
        if pod_count == 0 {
            return;
        }
        let new_cursor = match self.cursor {
            None => 0,
            Some(c) => (c + 1).min(pod_count - 1),
        };
        self.set_cursor(new_cursor, cx);
    }

    fn navigate_up(&mut self, _: &NavigateUp, _window: &mut Window, cx: &mut Context<Self>) {
        let pod_count = self.table.read(cx).delegate().pods.len();
        if pod_count == 0 {
            return;
        }
        let new_cursor = match self.cursor {
            None => 0,
            Some(c) => c.saturating_sub(1),
        };
        self.set_cursor(new_cursor, cx);
    }

    fn confirm_selection(
        &mut self,
        _: &ConfirmSelection,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Re-emit the current cursor pod so callers react even without new navigation.
        if let Some(pod_ix) = self.cursor {
            let pod = self.table.read(cx).delegate().pods.get(pod_ix).cloned();
            if let Some(pod) = pod {
                cx.emit(PodSelected { name: pod.name, namespace: pod.namespace });
            }
        }
    }

    fn focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.name_input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    fn toggle_grouping(
        &mut self,
        _: &ToggleGrouping,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.grouped = !self.grouped;
        self.apply_filters(cx);
    }

    fn set_cursor(&mut self, pod_ix: usize, cx: &mut Context<Self>) {
        self.cursor = Some(pod_ix);

        let row_ix = self.table.read(cx).delegate().row_for_pod(pod_ix);
        self.table.update(cx, |t, _| {
            t.delegate_mut().cursor_row = row_ix;
        });

        let pod = self.table.read(cx).delegate().pods.get(pod_ix).cloned();
        if let Some(pod) = pod {
            cx.emit(PodSelected { name: pod.name, namespace: pod.namespace });
        }
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for PodListPanel {}
impl EventEmitter<PodSelected> for PodListPanel {}

impl Focusable for PodListPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PodListPanel {
    fn panel_name(&self) -> &'static str {
        "PodListPanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Pods"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> {
        None
    }
}

impl Render for PodListPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let grouped = self.grouped;
        div()
            .key_context("PodList")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::navigate_down))
            .on_action(cx.listener(Self::navigate_up))
            .on_action(cx.listener(Self::confirm_selection))
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::toggle_grouping))
            .size_full()
            .flex()
            .flex_col()
            // ── Filter toolbar ────────────────────────────────────────────────
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(BORDER)
                    .flex_shrink_0()
                    .child(Input::new(&self.name_input).w(px(200.)))
                    .child(Input::new(&self.label_input).w(px(150.)))
                    .child(Select::new(&self.status_select).menu_width(gpui::rems(10.)))
                    .child(div().flex_1())
                    .child(
                        Button::new("group-toggle")
                            .label(if grouped { "Flat" } else { "Group" })
                            .compact()
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.grouped = !this.grouped;
                                this.apply_filters(cx);
                            })),
                    ),
            )
            // ── Pod table ─────────────────────────────────────────────────────
            .child(div().flex_1().child(DataTable::new(&self.table).stripe(true)))
    }
}
