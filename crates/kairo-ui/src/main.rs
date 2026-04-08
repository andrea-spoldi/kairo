mod actions;
mod app;
mod components;
mod kube_runtime;
mod theme;

use app::Workspace;
use gpui::*;
use gpui_component::{Root, TitleBar};
use gpui_component_assets::Assets;

use actions::{ConfirmSelection, FocusSearch, NavigateDown, NavigateUp, ToggleGrouping};

fn main() {
    tracing_subscriber::fmt::init();

    // Initialise the dedicated tokio runtime for kube-rs operations.
    // GPUI on macOS uses Grand Central Dispatch, not tokio, so kube-rs (which
    // depends on tower/hyper) must run on a real tokio executor.
    kube_runtime::init();

    gpui_platform::application()
        .with_assets(Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            // Switch the component library to dark mode so DataTable, Button,
            // Input, Select, and all other widgets use colours calibrated for
            // a dark background instead of the default light-mode palette.
            gpui_component::theme::Theme::change(
                gpui_component::theme::ThemeMode::Dark,
                None,
                cx,
            );
            cx.activate(true);

            // Pod list keyboard navigation (only fires when PodList has key context focus).
            cx.bind_keys([
                KeyBinding::new("j",      NavigateDown,      Some("PodList")),
                KeyBinding::new("k",      NavigateUp,        Some("PodList")),
                KeyBinding::new("down",   NavigateDown,      Some("PodList")),
                KeyBinding::new("up",     NavigateUp,        Some("PodList")),
                KeyBinding::new("return", ConfirmSelection,  Some("PodList")),
                KeyBinding::new("/",      FocusSearch,       Some("PodList")),
                KeyBinding::new("g",      ToggleGrouping,    Some("PodList")),
            ]);

            cx.spawn(async move |cx| {
                cx.open_window(
                    WindowOptions {
                        titlebar: Some(TitleBar::title_bar_options()),
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(100.), px(100.)),
                            size: size(px(1280.), px(800.)),
                        })),
                        window_min_size: Some(Size {
                            width: px(640.),
                            height: px(480.),
                        }),
                        kind: WindowKind::Normal,
                        ..Default::default()
                    },
                    |window, cx| {
                        let view = cx.new(|cx| Workspace::new(window, cx));
                        let view: AnyView = view.into();
                        cx.new(|cx| Root::new(view, window, cx))
                    },
                )
                .expect("failed to open window");
            })
            .detach();
        });
}
