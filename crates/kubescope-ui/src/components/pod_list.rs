use gpui::*;

/// Virtualized table showing the list of pods.
#[allow(dead_code)]
pub struct PodList;

impl Render for PodList {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Pod List")
    }
}
