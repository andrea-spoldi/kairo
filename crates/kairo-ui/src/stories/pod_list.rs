use gpui::*;
use gpui_component::Root;

use crate::components::pod_list::PodListPanel;
use super::{fixtures, story_header, window_opts};

pub struct PodListStory {
    panel: Entity<PodListPanel>,
}

impl PodListStory {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let panel = cx.new(|cx| PodListPanel::new(window, cx));
        panel.update(cx, |p, cx| p.set_pods(fixtures::pods(), cx));
        Self { panel }
    }
}

impl Render for PodListStory {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(story_header("pod-list — 10 pods across 4 namespaces"))
            .child(div().flex_1().min_h_0().child(self.panel.clone()))
    }
}

pub fn open(cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.open_window(window_opts(), |window, cx| {
            let view: AnyView = cx.new(|cx| PodListStory::new(window, cx)).into();
            cx.new(|cx| Root::new(view, window, cx))
        })
        .ok();
    })
    .detach();
}
