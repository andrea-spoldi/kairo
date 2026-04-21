use std::collections::HashSet;

use gpui::{prelude::FluentBuilder, *};
use gpui_component::dock::{Panel, PanelControl, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;
use kairo_core::models::{ResourceTreeNode, TreeNodeKind};

use crate::theme::{
    HOVER_BG, SELECTED_BG, STATUS_FAILED, STATUS_PENDING, STATUS_RUNNING, TEXT_MUTED,
    TEXT_PRIMARY, TEXT_SECONDARY,
};

// ── Public event ──────────────────────────────────────────────────────────────

/// Emitted when the user clicks a selectable resource node (Pod, Deployment, …).
#[derive(Clone)]
pub struct TreeNodeSelected {
    /// Kubernetes kind string, e.g. `"Pod"`, `"Deployment"`.
    pub kind: String,
    pub name: String,
    /// Empty for cluster-scoped resources (Nodes).
    pub namespace: String,
}

// ── Internal flat-list row ────────────────────────────────────────────────────

#[derive(Clone)]
struct FlatRow {
    id: String,
    kind: TreeNodeKind,
    name: String,
    namespace: Option<String>,
    status: Option<String>,
    depth: usize,
    has_children: bool,
    is_expanded: bool,
}

// ── Panel ─────────────────────────────────────────────────────────────────────

pub struct ResourceTreePanel {
    root: Option<ResourceTreeNode>,
    /// Set of node IDs that are currently expanded.
    expanded: HashSet<String>,
    selected: Option<String>,
    /// Flattened, visible rows rebuilt on every expand/collapse or tree update.
    rows: Vec<FlatRow>,
    focus_handle: FocusHandle,
}

impl ResourceTreePanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            root: None,
            expanded: HashSet::new(),
            selected: None,
            rows: vec![],
            focus_handle: cx.focus_handle(),
        }
    }

    /// Replace the tree root and rebuild the visible row list.
    /// Expansion state is preserved for nodes whose IDs still exist; new
    /// ClusterRoot and Namespace nodes are auto-expanded on first appearance.
    pub fn set_tree(&mut self, root: ResourceTreeNode, cx: &mut Context<Self>) {
        self.auto_expand_defaults(&root);
        self.root = Some(root);
        self.rebuild_rows();
        cx.notify();
    }

    fn auto_expand_defaults(&mut self, node: &ResourceTreeNode) {
        match node.kind {
            TreeNodeKind::ClusterRoot | TreeNodeKind::Namespace => {
                self.expanded.insert(node.id.clone());
            }
            _ => {}
        }
        for child in &node.children {
            self.auto_expand_defaults(child);
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        if let Some(root) = &self.root {
            let expanded = &self.expanded;
            let mut rows = Vec::new();
            Self::collect_rows(root, 0, expanded, &mut rows);
            self.rows = rows;
        }
    }

    fn collect_rows(
        node: &ResourceTreeNode,
        depth: usize,
        expanded: &HashSet<String>,
        rows: &mut Vec<FlatRow>,
    ) {
        let is_expanded = expanded.contains(&node.id);
        let has_children = !node.children.is_empty();
        rows.push(FlatRow {
            id: node.id.clone(),
            kind: node.kind.clone(),
            name: node.name.clone(),
            namespace: node.namespace.clone(),
            status: node.status.clone(),
            depth,
            has_children,
            is_expanded,
        });
        if is_expanded {
            for child in &node.children {
                Self::collect_rows(child, depth + 1, expanded, rows);
            }
        }
    }

    fn toggle(&mut self, id: String, cx: &mut Context<Self>) {
        if self.expanded.contains(&id) {
            self.expanded.remove(&id);
        } else {
            self.expanded.insert(id);
        }
        self.rebuild_rows();
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for ResourceTreePanel {}
impl EventEmitter<TreeNodeSelected> for ResourceTreePanel {}

impl Focusable for ResourceTreePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ResourceTreePanel {
    fn panel_name(&self) -> &'static str {
        "ResourceTreePanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Resources"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

// ── Render helpers ────────────────────────────────────────────────────────────

fn kind_icon(kind: &TreeNodeKind) -> &'static str {
    match kind {
        TreeNodeKind::ClusterRoot => "◈",
        TreeNodeKind::Namespace => "⬚",
        TreeNodeKind::KindGroup(_) => "▤",
        TreeNodeKind::Deployment => "▣",
        TreeNodeKind::StatefulSet => "▥",
        TreeNodeKind::DaemonSet => "▦",
        TreeNodeKind::Job => "◷",
        TreeNodeKind::Pod => "◉",
        TreeNodeKind::Service => "⬡",
        TreeNodeKind::ConfigMap => "≡",
        TreeNodeKind::Node => "◈",
    }
}

fn status_color(status: &str) -> Hsla {
    match status {
        "Running" | "Ready" => STATUS_RUNNING,
        s if s == "Pending" || s.contains("Creating") || s.contains("Init") => STATUS_PENDING,
        s if s.contains("Fail") || s.contains("Error") || s.contains("Crash") => STATUS_FAILED,
        _ => TEXT_MUTED,
    }
}

/// Return the Kubernetes kind string for selectable node kinds, or `None` for
/// structural nodes (ClusterRoot, Namespace, KindGroup) that are not selectable.
fn selectable_kind(kind: &TreeNodeKind) -> Option<&'static str> {
    match kind {
        TreeNodeKind::Pod => Some("Pod"),
        TreeNodeKind::Deployment => Some("Deployment"),
        TreeNodeKind::Service => Some("Service"),
        TreeNodeKind::ConfigMap => Some("ConfigMap"),
        TreeNodeKind::Node => Some("Node"),
        _ => None,
    }
}

// ── Render ────────────────────────────────────────────────────────────────────

impl Render for ResourceTreePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        let selected = self.selected.clone();

        // Single return path: build list content then wrap in scrollable outer div.
        let mut list = div().flex().flex_col().p(px(4.)).gap(px(1.));

        if rows.is_empty() {
            // Inline empty state — no early return avoids type-mismatch with Scrollable<Div>.
            list = list
                .flex_1()
                .items_center()
                .justify_center()
                .child(Label::new("Connecting…").text_sm().text_color(TEXT_MUTED));
        }

        for (ix, row) in rows.iter().enumerate() {
            let is_selected = selected.as_deref() == Some(row.id.as_str());
            let id = row.id.clone();
            let has_children = row.has_children;
            let kind = row.kind.clone();
            let name_ev = row.name.clone();
            let ns_ev = row.namespace.clone().unwrap_or_default();

            let toggle_icon: &'static str = if row.has_children {
                if row.is_expanded { "▾" } else { "▸" }
            } else {
                " "
            };
            let icon = kind_icon(&row.kind);
            let name_display = SharedString::from(row.name.clone());
            let status = row.status.clone();
            let indent = px(row.depth as f32 * 14.0);

            let row_el = h_flex()
                .id(("tree-row", ix))
                .pl(indent)
                .pr(px(8.))
                .py(px(3.))
                .rounded_md()
                .cursor_pointer()
                .when(is_selected, |s| s.bg(SELECTED_BG))
                .when(!is_selected, |s| s.hover(|s| s.bg(HOVER_BG)))
                .gap(px(4.))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.selected = Some(id.clone());
                    if has_children {
                        this.toggle(id.clone(), cx);
                    } else {
                        cx.notify();
                    }
                    if let Some(kind_str) = selectable_kind(&kind) {
                        let namespace = if kind_str == "Node" {
                            String::new()
                        } else {
                            ns_ev.clone()
                        };
                        cx.emit(TreeNodeSelected {
                            kind: kind_str.to_string(),
                            name: name_ev.clone(),
                            namespace,
                        });
                    }
                }))
                .child(
                    div()
                        .w(px(10.))
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(TEXT_MUTED)
                        .child(toggle_icon),
                )
                .child(
                    div()
                        .w(px(14.))
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(TEXT_SECONDARY)
                        .child(icon),
                )
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .text_color(TEXT_PRIMARY)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(name_display),
                )
                .when_some(status, |el, st| {
                    let color = status_color(&st);
                    el.child(
                        div()
                            .text_xs()
                            .text_color(color)
                            .flex_shrink_0()
                            .max_w(px(56.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(SharedString::from(st)),
                    )
                });

            list = list.child(row_el);
        }

        div()
            .size_full()
            .overflow_y_scrollbar()
            .child(list)
    }
}
