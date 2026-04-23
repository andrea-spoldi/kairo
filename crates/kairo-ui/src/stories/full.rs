use gpui::*;
use gpui_component::Root;

use crate::app::Workspace;
use super::fixtures;

/// Full Workspace rendered with mock data — no cluster connection needed.
///
/// Unlike the isolated component stories, this opens the complete dock layout
/// so you can work on cross-panel interactions, dock resize behaviour, and the
/// status bar without a live cluster.
pub fn open(cx: &mut gpui::App) {
    cx.spawn(async move |cx| {
        cx.open_window(
            gpui::WindowOptions {
                titlebar: Some(gpui_component::TitleBar::title_bar_options()),
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
                    origin: gpui::point(gpui::px(100.), gpui::px(100.)),
                    size: gpui::size(gpui::px(1280.), gpui::px(800.)),
                })),
                window_min_size: Some(gpui::Size {
                    width: gpui::px(640.),
                    height: gpui::px(480.),
                }),
                kind: gpui::WindowKind::Normal,
                ..Default::default()
            },
            |window, cx| {
                let view: AnyView = cx
                    .new(|cx| {
                        Workspace::new_story(
                            fixtures::pods(),
                            fixtures::cluster_events(),
                            vec![
                                "cert-manager".to_string(),
                                "ingress-nginx".to_string(),
                                "jobs".to_string(),
                                "monitoring".to_string(),
                                "production".to_string(),
                            ],
                            window,
                            cx,
                        )
                    })
                    .into();
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .ok();
    })
    .detach();
}
