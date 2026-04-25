use gpui::*;
use gpui_component::Root;

use crate::components::log_viewer::LogViewerPanel;
use super::{fixtures, story_header, window_opts};

pub struct LogViewerStory {
    panel: Entity<LogViewerPanel>,
}

impl LogViewerStory {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let panel = cx.new(|cx| LogViewerPanel::new(cx));
        panel.update(cx, |p, cx| {
            p.set_pod(
                "api-server-7d9f8b-xk2p4".to_string(),
                "production".to_string(),
                vec!["api-server".to_string(), "envoy-proxy".to_string()],
                cx,
            );
        });
        for line in fixtures::log_lines() {
            panel.update(cx, |p, cx| p.push_line(line, cx));
        }
        Self { panel }
    }
}

impl Render for LogViewerStory {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(story_header("log-viewer — api-server-7d9f8b-xk2p4"))
            .child(div().flex_1().min_h_0().child(self.panel.clone()))
    }
}

pub fn open(cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.open_window(window_opts(), |window, cx| {
            let view: AnyView = cx.new(LogViewerStory::new).into();
            cx.new(|cx| Root::new(view, window, cx))
        })
        .ok();
    })
    .detach();
}
