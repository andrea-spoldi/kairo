use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};

/// Right panel — pod detail view (populated in Phase 7).
pub struct PodDetailPanel {
    focus_handle: FocusHandle,
}

impl PodDetailPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for PodDetailPanel {}

impl Focusable for PodDetailPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for PodDetailPanel {
    fn panel_name(&self) -> &'static str {
        "PodDetailPanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Pod Detail"
    }
}

impl Render for PodDetailPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("Select a pod to view details — Phase 7")
    }
}
