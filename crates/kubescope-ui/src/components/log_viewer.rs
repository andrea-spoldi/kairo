use gpui::*;

/// Streaming log viewer for pod containers.
#[allow(dead_code)]
pub struct LogViewer;

impl Render for LogViewer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Log Viewer")
    }
}
