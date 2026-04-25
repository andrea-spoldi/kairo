use gpui::*;
use gpui_component::Root;

use crate::components::event_feed::EventFeedPanel;
use super::{fixtures, story_header, window_opts};

pub struct EventFeedStory {
    panel: Entity<EventFeedPanel>,
}

impl EventFeedStory {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let panel = cx.new(|cx| EventFeedPanel::new(cx));
        for ev in fixtures::cluster_events() {
            panel.update(cx, |p, cx| p.push_event(ev, cx));
        }
        Self { panel }
    }
}

impl Render for EventFeedStory {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(story_header("event-feed — 5 cluster events"))
            .child(div().flex_1().min_h_0().child(self.panel.clone()))
    }
}

pub fn open(cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.open_window(window_opts(), |window, cx| {
            let view: AnyView = cx.new(EventFeedStory::new).into();
            cx.new(|cx| Root::new(view, window, cx))
        })
        .ok();
    })
    .detach();
}
