use gpui::*;
use gpui_component::Root;

use crate::components::pod_detail::{DetailPanel, ResourceDetail};
use super::{fixtures, story_header, window_opts};

pub struct PodDetailStory {
    panel: Entity<DetailPanel>,
}

impl PodDetailStory {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let panel = cx.new(|cx| DetailPanel::new(cx));
        panel.update(cx, |p, _| {
            p.set_detail(ResourceDetail::Pod(fixtures::pod_detail()));
        });
        Self { panel }
    }
}

impl Render for PodDetailStory {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(story_header("pod-detail — api-server-7d9f8b-xk2p4"))
            .child(div().flex_1().min_h_0().child(self.panel.clone()))
    }
}

pub fn open(cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.open_window(window_opts(), |window, cx| {
            let view: AnyView = cx.new(PodDetailStory::new).into();
            cx.new(|cx| Root::new(view, window, cx))
        })
        .ok();
    })
    .detach();
}
