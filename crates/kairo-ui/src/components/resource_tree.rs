use std::collections::HashSet;

use gpui::{prelude::FluentBuilder, *};
use gpui_component::dock::{Panel, PanelControl, PanelEvent};
use gpui_component::h_flex;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::label::Label;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use gpui_component::scroll::ScrollableElement;
use kairo_core::models::{RelationType, ResourceRelationship, ResourceScope, ResourceTreeNode, TreeNodeKind};

use crate::theme::{
    BORDER, HOVER_BG, SELECTED_BG, STATUS_FAILED, STATUS_PENDING, STATUS_RUNNING,
    TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
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
    scope: ResourceScope,
    /// Up to 2 relationship badges to show inline.
    relationships: Vec<ResourceRelationship>,
    depth: usize,
    has_children: bool,
    is_expanded: bool,
}

// ── Panel ─────────────────────────────────────────────────────────────────────

/// KindGroup label strings (as produced by `build_resource_tree`) that can be
/// toggled on/off in the dropdown filter. The label must match exactly.
const FILTERABLE_KINDS: &[&str] = &[
    "Pods",
    "Deployments",
    "Services",
    "Ingresses",
    "ConfigMaps",
    "Nodes",
    "Cluster Resources",
];

pub struct ResourceTreePanel {
    root: Option<ResourceTreeNode>,
    /// Set of node IDs that are currently expanded.
    expanded: HashSet<String>,
    /// IDs seen at least once — prevents re-expanding nodes the user has collapsed.
    seen_ids: HashSet<String>,
    selected: Option<String>,
    /// Flattened, visible rows rebuilt on every expand/collapse or tree update.
    rows: Vec<FlatRow>,
    focus_handle: FocusHandle,
    filter_input: Entity<InputState>,
    filter: String,
    /// Kind labels (matching FILTERABLE_KINDS labels) that are hidden from the tree.
    hidden_kinds: HashSet<String>,
}

impl ResourceTreePanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter resources…"));
        cx.subscribe(&filter_input, |this, state, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                this.filter = state.read(cx).value().to_string();
                this.rebuild_rows();
                cx.notify();
            }
        })
        .detach();
        Self {
            root: None,
            expanded: HashSet::new(),
            seen_ids: HashSet::new(),
            selected: None,
            rows: vec![],
            focus_handle: cx.focus_handle(),
            filter_input,
            filter: String::new(),
            hidden_kinds: HashSet::new(),
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
        let first_time = self.seen_ids.insert(node.id.clone());
        if first_time {
            match node.kind {
                TreeNodeKind::ClusterRoot | TreeNodeKind::Namespace | TreeNodeKind::KindGroup(_) => {
                    self.expanded.insert(node.id.clone());
                }
                _ => {}
            }
        }
        for child in &node.children {
            self.auto_expand_defaults(child);
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        if let Some(root) = &self.root {
            let expanded = &self.expanded;
            let hidden = &self.hidden_kinds;
            let mut rows = Vec::new();
            Self::collect_rows(root, 0, expanded, hidden, &mut rows);
            if !self.filter.is_empty() {
                let q = self.filter.to_lowercase();
                rows.retain(|r| r.name.to_lowercase().contains(&q));
            }
            self.rows = rows;
        }
    }

    fn collect_rows(
        node: &ResourceTreeNode,
        depth: usize,
        expanded: &HashSet<String>,
        hidden: &HashSet<String>,
        rows: &mut Vec<FlatRow>,
    ) {
        // Skip entire KindGroup subtrees whose kind label is hidden.
        if let TreeNodeKind::KindGroup(ref label) = node.kind {
            if hidden.contains(label.as_str()) {
                return;
            }
        }
        let is_expanded = expanded.contains(&node.id);
        let has_children = !node.children.is_empty();
        // Cap relationship badges at 2 to avoid overflow
        let relationships = node.relationships.iter().take(2).cloned().collect();
        rows.push(FlatRow {
            id: node.id.clone(),
            kind: node.kind.clone(),
            name: node.name.clone(),
            namespace: node.namespace.clone(),
            status: node.status.clone(),
            scope: node.scope.clone(),
            relationships,
            depth,
            has_children,
            is_expanded,
        });
        if is_expanded {
            for child in &node.children {
                Self::collect_rows(child, depth + 1, expanded, hidden, rows);
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

    fn dropdown_menu(
        &mut self,
        menu: PopupMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PopupMenu {
        let weak = cx.entity().downgrade();
        let menu = menu.separator().item(PopupMenuItem::label("Show / Hide"));
        FILTERABLE_KINDS.iter().fold(menu, |menu, &label| {
            let is_hidden = self.hidden_kinds.contains(label);
            let w = weak.clone();
            let label_str = label.to_string();
            menu.item(
                PopupMenuItem::new(label)
                    .checked(!is_hidden)
                    .on_click(move |_, _, cx| {
                        w.update(cx, |this, cx| {
                            if this.hidden_kinds.contains(&label_str) {
                                this.hidden_kinds.remove(&label_str);
                            } else {
                                this.hidden_kinds.insert(label_str.clone());
                            }
                            this.rebuild_rows();
                            cx.notify();
                        })
                        .ok();
                    }),
            )
        })
    }
}

// ── Render helpers ────────────────────────────────────────────────────────────

fn kind_icon(kind: &TreeNodeKind) -> &'static str {
    match kind {
        TreeNodeKind::ClusterRoot => "◈",
        TreeNodeKind::Namespace => "⬚",
        TreeNodeKind::KindGroup(_) => "▤",
        TreeNodeKind::Deployment => "▣",
        TreeNodeKind::ReplicaSet => "◫",
        TreeNodeKind::StatefulSet => "▥",
        TreeNodeKind::DaemonSet => "▦",
        TreeNodeKind::Job => "◷",
        TreeNodeKind::Pod => "◉",
        TreeNodeKind::Container => "▸",
        TreeNodeKind::Service => "⬡",
        TreeNodeKind::Ingress => "⊶",
        TreeNodeKind::ConfigMap => "≡",
        TreeNodeKind::Node => "◈",
        TreeNodeKind::HorizontalPodAutoscaler => "⇅",
        TreeNodeKind::StorageClass => "⊞",
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

/// Return the Kubernetes kind string for selectable node kinds.
/// Container (Embedded) and structural nodes are non-selectable.
fn selectable_kind(kind: &TreeNodeKind, scope: &ResourceScope) -> Option<&'static str> {
    if *scope == ResourceScope::Embedded {
        return None;
    }
    match kind {
        TreeNodeKind::Pod => Some("Pod"),
        TreeNodeKind::Deployment => Some("Deployment"),
        TreeNodeKind::ReplicaSet => Some("ReplicaSet"),
        TreeNodeKind::Service => Some("Service"),
        TreeNodeKind::Ingress => Some("Ingress"),
        TreeNodeKind::ConfigMap => Some("ConfigMap"),
        TreeNodeKind::Node => Some("Node"),
        TreeNodeKind::StorageClass => Some("StorageClass"),
        TreeNodeKind::HorizontalPodAutoscaler => Some("HorizontalPodAutoscaler"),
        _ => None,
    }
}

fn rel_badge_text(rel: &ResourceRelationship) -> String {
    let arrow = match rel.rel_type {
        RelationType::References => "→",
        RelationType::TargetedBy => "←",
        RelationType::Selects => "→",
        RelationType::RoutesTo => "→",
        RelationType::BindsTo => "↔",
        RelationType::UsesStorageClass => "→",
        RelationType::Targets => "→",
    };
    let kind_abbr = match rel.kind.as_str() {
        "ServiceAccount" => "SA",
        "ConfigMap" => "CM",
        "Secret" => "Sec",
        "PersistentVolumeClaim" => "PVC",
        "HorizontalPodAutoscaler" => "HPA",
        "Service" => "Svc",
        "Pod" => "Pod",
        other => other,
    };
    format!("{arrow}{kind_abbr}:{}", rel.name)
}

// ── Render ────────────────────────────────────────────────────────────────────

impl Render for ResourceTreePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        let selected = self.selected.clone();

        let mut list = div().flex().flex_col().p(px(4.)).gap(px(1.));

        if rows.is_empty() {
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
            let scope = row.scope.clone();
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
            let is_container = row.scope == ResourceScope::Embedded;
            let relationships = row.relationships.clone();

            let row_el = h_flex()
                .id(("tree-row", ix))
                .pl(indent)
                .pr(px(8.))
                .py(px(3.))
                .rounded_md()
                .when(!is_container, |s| s.cursor_pointer())
                .when(is_selected, |s| s.bg(SELECTED_BG))
                .when(!is_selected, |s| s.hover(|s| s.bg(HOVER_BG)))
                .gap(px(4.))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    if is_container { return; }
                    this.selected = Some(id.clone());
                    if has_children {
                        this.toggle(id.clone(), cx);
                    } else {
                        cx.notify();
                    }
                    if let Some(kind_str) = selectable_kind(&kind, &scope) {
                        let namespace = if kind_str == "Node" || kind_str == "StorageClass" {
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
                        .text_color(if is_container { TEXT_MUTED } else { TEXT_SECONDARY })
                        .child(icon),
                )
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .text_color(if is_container { TEXT_MUTED } else { TEXT_PRIMARY })
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(name_display),
                )
                .when(!relationships.is_empty(), |el| {
                    let badges: Vec<_> = relationships.iter().map(|rel| {
                        div()
                            .text_xs()
                            .text_color(TEXT_MUTED)
                            .flex_shrink_0()
                            .child(SharedString::from(rel_badge_text(rel)))
                    }).collect();
                    el.children(badges)
                })
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

            // Namespace nodes get a subtle glass background wrapper.
            if matches!(row.kind, TreeNodeKind::Namespace) {
                list = list.child(
                    div()
                        .rounded(px(12.))
                        .bg(Hsla { h: 0.0, s: 0.0, l: 1.0, a: 0.02 })
                        .border_1()
                        .border_color(BORDER)
                        .child(row_el),
                );
            } else {
                list = list.child(row_el);
            }
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .flex_shrink_0()
                    .child(Input::new(&self.filter_input)),
            )
            .child(div().flex_1().overflow_y_scrollbar().child(list))
    }
}
