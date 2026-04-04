use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::select::{Select, SelectEvent, SelectState};
use gpui_component::table::{Column, DataTable, TableDelegate, TableEvent, TableState};
use kubescope_core::models::PodSummary;

use crate::theme::status_color;

/// Emitted when the user clicks a pod row.
#[derive(Clone)]
pub struct PodSelected {
    pub name: String,
    pub namespace: String,
}

// Column definitions: (key, display name, default width px)
const COLUMNS: &[(&str, &str, f32)] = &[
    ("name",      "Name",      200.),
    ("namespace", "Namespace", 120.),
    ("status",    "Status",    150.),
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

/// `TableDelegate` implementation backed by a `Vec<PodSummary>`.
pub struct PodTableDelegate {
    pub pods: Vec<PodSummary>,
    columns: Vec<Column>,
}

impl PodTableDelegate {
    pub fn new() -> Self {
        let columns = COLUMNS
            .iter()
            .map(|(k, n, w)| Column::new(*k, *n).width(*w))
            .collect();
        Self { pods: vec![], columns }
    }
}

impl TableDelegate for PodTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.pods.len()
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
        let pod = &self.pods[row_ix];
        let key = self.columns[col_ix].key.as_ref();
        match key {
            "name"      => div().child(pod.name.clone()).into_any_element(),
            "namespace" => div().child(pod.namespace.clone()).into_any_element(),
            "status"    => {
                let color = status_color(&pod.status);
                h_flex()
                    .gap_1()
                    .child(div().w_2().h_2().rounded_full().bg(color))
                    .child(pod.status.clone())
                    .into_any_element()
            }
            "ready"    => div().child(pod.ready.clone()).into_any_element(),
            "restarts" => div().child(pod.restarts.to_string()).into_any_element(),
            "age"      => div().child(pod.age.clone()).into_any_element(),
            "node"     => div().child(pod.node.clone()).into_any_element(),
            _          => div().into_any_element(),
        }
    }
}

/// Center panel — virtualized pod list table with search and filter toolbar.
pub struct PodListPanel {
    pub table: Entity<TableState<PodTableDelegate>>,
    focus_handle: FocusHandle,
    /// Namespace-filtered pods received from `Workspace`; search filters apply on top.
    source_pods: Vec<PodSummary>,
    /// Input for name substring filter.
    name_input: Entity<InputState>,
    /// Input for label selector (`key=value` or bare `key`).
    label_input: Entity<InputState>,
    /// Dropdown for status filter.
    status_select: Entity<SelectState<Vec<SharedString>>>,
}

impl PodListPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = PodTableDelegate::new();
        let table = cx.new(|cx| TableState::new(delegate, window, cx));

        cx.subscribe_in(
            &table,
            window,
            |this, _table, event: &TableEvent, _window, cx| {
                if let TableEvent::SelectRow(row_ix) = event {
                    if let Some(pod) = this.table.read(cx).delegate().pods.get(*row_ix) {
                        cx.emit(PodSelected {
                            name: pod.name.clone(),
                            namespace: pod.namespace.clone(),
                        });
                    }
                }
            },
        )
        .detach();

        let name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Filter by name…")
        });
        let label_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("app=nginx")
        });
        let status_select = cx.new(|cx| {
            SelectState::new(
                status_filter_items(),
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            )
        });

        // Re-filter whenever any input changes.
        cx.subscribe_in(
            &name_input,
            window,
            |this, _, _: &InputEvent, _window, cx| {
                this.apply_filters(cx);
            },
        )
        .detach();

        cx.subscribe_in(
            &label_input,
            window,
            |this, _, _: &InputEvent, _window, cx| {
                this.apply_filters(cx);
            },
        )
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
        }
    }

    /// Replace the source pod list and re-apply current filters.
    pub fn set_pods(&mut self, pods: Vec<PodSummary>, cx: &mut Context<Self>) {
        self.source_pods = pods;
        self.apply_filters(cx);
    }

    /// Apply all active filters and push the result to the table delegate.
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
                // Name substring (case-insensitive).
                if !name_query.is_empty() && !p.name.to_lowercase().contains(&name_query) {
                    return false;
                }
                // Status dropdown.
                if status_value.as_ref() != "All" && p.status != status_value.as_ref() {
                    return false;
                }
                // Label selector: `key=value` or bare `key`.
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

        self.table.update(cx, |table, _| {
            table.delegate_mut().pods = filtered;
        });
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
}

impl Render for PodListPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            // ── Filter toolbar ────────────────────────────────────────────────
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .border_b_1()
                    .border_color(gpui::rgb(0x3a3a3a))
                    .flex_shrink_0()
                    .child(Input::new(&self.name_input).w(px(180.)))
                    .child(Input::new(&self.label_input).w(px(140.)))
                    .child(
                        Select::new(&self.status_select).menu_width(gpui::rems(9.)),
                    ),
            )
            // ── Pod table ─────────────────────────────────────────────────────
            .child(div().flex_1().child(DataTable::new(&self.table).stripe(true)))
    }
}
