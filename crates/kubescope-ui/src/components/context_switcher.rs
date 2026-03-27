use gpui::*;

/// Dropdown for switching Kubernetes contexts.
#[allow(dead_code)]
pub struct ContextSwitcher;

impl Render for ContextSwitcher {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Context")
    }
}
