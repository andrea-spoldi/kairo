use gpui::*;

/// Detail panel for a selected pod.
#[allow(dead_code)]
pub struct PodDetail;

impl Render for PodDetail {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Pod Detail")
    }
}
