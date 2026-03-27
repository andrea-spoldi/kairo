use gpui::*;

/// Dropdown for selecting the active Kubernetes namespace.
#[allow(dead_code)]
pub struct NamespaceSelector;

impl Render for NamespaceSelector {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Namespace")
    }
}
