use gpui::*;

/// The root view of the application — placeholder for Phase 1.
pub struct AppRoot;

impl Render for AppRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0xCDD6F4))
            .bg(rgb(0x1E1E2E))
            .child("KubeScope")
    }
}
