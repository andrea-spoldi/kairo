use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::table::{Column, DataTable, TableDelegate, TableState};
use kubescope_core::models::PodSummary;

use crate::theme::status_color;

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
            "name" => div().child(pod.name.clone()).into_any_element(),
            "namespace" => div().child(pod.namespace.clone()).into_any_element(),
            "status" => {
                let color = status_color(&pod.status);
                h_flex()
                    .gap_1()
                    .child(div().w_2().h_2().rounded_full().bg(color))
                    .child(pod.status.clone())
                    .into_any_element()
            }
            "ready" => div().child(pod.ready.clone()).into_any_element(),
            "restarts" => div().child(pod.restarts.to_string()).into_any_element(),
            "age" => div().child(pod.age.clone()).into_any_element(),
            "node" => div().child(pod.node.clone()).into_any_element(),
            _ => div().into_any_element(),
        }
    }
}

/// Center panel — virtualized pod list table.
pub struct PodListPanel {
    pub table: Entity<TableState<PodTableDelegate>>,
    focus_handle: FocusHandle,
}

impl PodListPanel {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let delegate = PodTableDelegate::new();
        let table = cx.new(|cx| TableState::new(delegate, window, cx));
        Self {
            table,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for PodListPanel {}

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
        div().size_full().child(DataTable::new(&self.table).stripe(true))
    }
}
