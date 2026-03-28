use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};

/// Bottom panel — streaming log viewer (populated in Phase 8).
pub struct LogViewerPanel {
    focus_handle: FocusHandle,
}

impl LogViewerPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for LogViewerPanel {}

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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child("Log stream — Phase 8")
    }
}
