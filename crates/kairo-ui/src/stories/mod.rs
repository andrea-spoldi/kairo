mod event_feed;
mod fixtures;
mod log_viewer;
mod pod_detail;
mod pod_list;

use gpui::{
    Bounds, WindowBounds, WindowKind, WindowOptions, div, point, px, size,
    IntoElement, ParentElement, Styled,
};
use gpui_component::label::Label;
use crate::theme::{ACCENT_FG, BG_RAISED, BORDER, TEXT_MUTED};

/// Open a story window by name.  Called from `main` when `--story <name>` is passed.
///
/// Available stories: `pod-list`, `event-feed`, `pod-detail`, `log-viewer`
pub fn run(name: &str, cx: &mut gpui::App) {
    match name {
        "pod-list"    => pod_list::open(cx),
        "event-feed"  => event_feed::open(cx),
        "pod-detail"  => pod_detail::open(cx),
        "log-viewer"  => log_viewer::open(cx),
        other => {
            eprintln!(
                "unknown story '{other}'. available: pod-list, event-feed, pod-detail, log-viewer"
            );
            std::process::exit(1);
        }
    }
}

// ── Shared helpers ─────────────────────────────────────────────────────────────

pub(super) fn window_opts() -> WindowOptions {
    WindowOptions {
        titlebar: Some(gpui_component::TitleBar::title_bar_options()),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(120.), px(120.)),
            size: size(px(1100.), px(720.)),
        })),
        kind: WindowKind::Normal,
        ..Default::default()
    }
}

pub(super) fn story_header(label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .px_3()
        .py_1()
        .bg(BG_RAISED)
        .border_b_1()
        .border_color(BORDER)
        .flex_shrink_0()
        .child(
            Label::new("STORY")
                .text_xs()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(ACCENT_FG),
        )
        .child(
            Label::new("  ·  ")
                .text_xs()
                .text_color(TEXT_MUTED),
        )
        .child(Label::new(label).text_xs().text_color(TEXT_MUTED))
}
