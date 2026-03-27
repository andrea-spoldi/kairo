use gpui::*;

/// Search and filter bar above the pod list.
#[allow(dead_code)]
pub struct SearchBar;

impl Render for SearchBar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Search")
    }
}
