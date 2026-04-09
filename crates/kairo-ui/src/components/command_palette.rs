use gpui::*;
use gpui_component::{
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    scroll::ScrollableElement,
};
use kairo_core::models::PodSummary;

use crate::actions::{CloseCommandPalette, ConfirmSelection, NavigateDown, NavigateUp};
use crate::theme::{BORDER, STATUS_PENDING, STATUS_RUNNING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY};

// ── Item model ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum ItemKind {
    Pod,
    Context,
    Namespace,
}

#[derive(Clone, Debug)]
struct PaletteItem {
    kind: ItemKind,
    /// Primary display text (pod name, context name, namespace name).
    label: String,
    /// Secondary text shown on the right (namespace for pods, empty otherwise).
    detail: String,
}

// ── Events emitted to Workspace ────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum PaletteAction {
    SelectPod { name: String, namespace: String },
    SwitchContext(String),
    SwitchNamespace(String),
}

impl EventEmitter<PaletteAction> for CommandPalette {}

// ── Component ──────────────────────────────────────────────────────────────────

pub struct CommandPalette {
    visible: bool,
    all_items: Vec<PaletteItem>,
    filtered: Vec<usize>,
    selected: usize,
    input: Entity<InputState>,
    focus_handle: FocusHandle,
}

impl CommandPalette {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search pods, contexts, namespaces…")
        });

        cx.subscribe(&input, |this, state, event: &InputEvent, cx| {
            match event {
                InputEvent::Change => {
                    let text = state.read(cx).value().to_string();
                    this.refilter(&text, cx);
                }
                InputEvent::PressEnter { .. } => {
                    this.confirm(cx);
                }
                _ => {}
            }
        })
        .detach();

        Self {
            visible: false,
            all_items: vec![],
            filtered: vec![],
            selected: 0,
            input,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Open the palette and focus its text input.
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        self.selected = 0;
        self.input.update(cx, |s, cx| s.set_value("", window, cx));
        self.refilter("", cx);
        // Focus the text input so the user can type immediately.
        window.focus(&self.input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        cx.notify();
    }

    /// Replace the pod entries in the item list.
    pub fn set_pods(&mut self, pods: &[PodSummary], cx: &mut Context<Self>) {
        self.all_items.retain(|i| i.kind != ItemKind::Pod);
        for pod in pods {
            self.all_items.push(PaletteItem {
                kind: ItemKind::Pod,
                label: pod.name.clone(),
                detail: pod.namespace.clone(),
            });
        }
        self.refilter_current(cx);
    }

    /// Replace the context entries.
    pub fn set_contexts(&mut self, contexts: &[SharedString], cx: &mut Context<Self>) {
        self.all_items.retain(|i| i.kind != ItemKind::Context);
        for ctx in contexts {
            self.all_items.push(PaletteItem {
                kind: ItemKind::Context,
                label: ctx.to_string(),
                detail: String::new(),
            });
        }
        self.refilter_current(cx);
    }

    /// Replace the namespace entries (skips the "All" sentinel).
    pub fn set_namespaces(&mut self, namespaces: &[SharedString], cx: &mut Context<Self>) {
        self.all_items.retain(|i| i.kind != ItemKind::Namespace);
        for ns in namespaces {
            if ns.as_ref() == "All" {
                continue;
            }
            self.all_items.push(PaletteItem {
                kind: ItemKind::Namespace,
                label: ns.to_string(),
                detail: String::new(),
            });
        }
        self.refilter_current(cx);
    }

    // ── Private ────────────────────────────────────────────────────────────────

    fn refilter_current(&mut self, cx: &mut Context<Self>) {
        let q = self.input.read(cx).value().to_string();
        self.refilter(&q, cx);
    }

    fn refilter(&mut self, query: &str, cx: &mut Context<Self>) {
        let q = query.to_lowercase();
        self.filtered = self
            .all_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                q.is_empty()
                    || item.label.to_lowercase().contains(&q)
                    || item.detail.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        self.selected = 0;
        cx.notify();
    }

    fn navigate(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.filtered.len();
        if n == 0 {
            return;
        }
        self.selected = ((self.selected as isize + delta).rem_euclid(n as isize)) as usize;
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(&item_ix) = self.filtered.get(self.selected) {
            let item = &self.all_items[item_ix];
            let action = match item.kind {
                ItemKind::Pod => PaletteAction::SelectPod {
                    name: item.label.clone(),
                    namespace: item.detail.clone(),
                },
                ItemKind::Context => PaletteAction::SwitchContext(item.label.clone()),
                ItemKind::Namespace => PaletteAction::SwitchNamespace(item.label.clone()),
            };
            cx.emit(action);
        }
        self.hide(cx);
    }
}

// ── Render ─────────────────────────────────────────────────────────────────────

impl Render for CommandPalette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().into_any_element();
        }

        let selected = self.selected;
        let filtered = self.filtered.clone();
        let items = self.all_items.clone();

        // Full-screen backdrop that dismisses on click.
        // on_scroll_wheel stops wheel events from reaching panels behind the overlay.
        div()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000099))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.hide(cx)),
            )
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            // Center the modal box horizontally.
            .flex()
            .justify_center()
            .pt(px(100.))
            .child(
                div()
                    .w(px(560.))
                    .max_h(px(440.))
                    .bg(rgba(0x1E1E2EFF))
                    .border_1()
                    .border_color(BORDER)
                    .rounded(px(10.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Stop backdrop click-through.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    // Key bindings scoped to this overlay.
                    .key_context("Palette")
                    .track_focus(&self.focus_handle)
                    .on_action(cx.listener(|this, _: &NavigateDown, _, cx| this.navigate(1, cx)))
                    .on_action(cx.listener(|this, _: &NavigateUp, _, cx| this.navigate(-1, cx)))
                    .on_action(cx.listener(|this, _: &ConfirmSelection, _, cx| this.confirm(cx)))
                    .on_action(cx.listener(|this, _: &CloseCommandPalette, _, cx| this.hide(cx)))
                    // ── Input ──────────────────────────────────────────────────
                    .child(
                        div()
                            .px(px(12.))
                            .pt(px(10.))
                            .pb(px(8.))
                            .child(Input::new(&self.input).appearance(false)),
                    )
                    // Divider
                    .child(div().h_px().bg(BORDER))
                    // ── Results ────────────────────────────────────────────────
                    // flex_1 + min_h_0 gives the Scrollable wrapper a definite
                    // height so its inner overflow_y_scroll actually activates.
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .child(render_results(&filtered, &items, selected)),
                    )
                    // ── Footer hint ────────────────────────────────────────────
                    .child(
                        div()
                            .h_px()
                            .bg(BORDER),
                    )
                    .child(
                        h_flex()
                            .px(px(12.))
                            .py(px(6.))
                            .gap(px(12.))
                            .child(hint("↑↓", "navigate"))
                            .child(hint("↩", "select"))
                            .child(hint("esc", "close")),
                    ),
            )
            .into_any_element()
    }
}

fn render_results(
    filtered: &[usize],
    items: &[PaletteItem],
    selected: usize,
) -> impl IntoElement {
    let mut list = div().py(px(4.));

    if filtered.is_empty() {
        list = list.child(
            div()
                .px(px(12.))
                .py(px(12.))
                .text_xs()
                .text_color(TEXT_MUTED)
                .child("No results"),
        );
    } else {
        for (list_ix, &item_ix) in filtered.iter().enumerate() {
            let item = &items[item_ix];
            let is_sel = list_ix == selected;

            let (kind_tag, tag_color) = match item.kind {
                ItemKind::Pod => ("pod", TEXT_SECONDARY),
                ItemKind::Context => ("ctx", STATUS_RUNNING),
                ItemKind::Namespace => ("ns", STATUS_PENDING),
            };

            let mut row = div()
                .px(px(12.))
                .py(px(6.))
                .mx(px(4.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(4.));

            if is_sel {
                row = row.bg(rgba(0x313244FF));
            }

            row = row
                .child(
                    div()
                        .w(px(28.))
                        .text_xs()
                        .text_color(tag_color)
                        .child(kind_tag),
                )
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .text_color(TEXT_PRIMARY)
                        .truncate()
                        .child(SharedString::from(item.label.clone())),
                );

            if !item.detail.is_empty() {
                row = row.child(
                    div()
                        .text_xs()
                        .text_color(TEXT_MUTED)
                        .child(SharedString::from(item.detail.clone())),
                );
            }

            list = list.child(row);
        }
    }

    list.overflow_y_scrollbar()
}

fn hint(key: &'static str, label: &'static str) -> impl IntoElement {
    h_flex()
        .gap(px(4.))
        .child(
            Label::new(key)
                .text_xs()
                .text_color(rgba(0xCDD6F4AA)),
        )
        .child(
            Label::new(label)
                .text_xs()
                .text_color(TEXT_MUTED),
        )
}
