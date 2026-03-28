mod actions;
mod app;
mod components;
mod theme;

use app::AppRoot;
use gpui::*;
use gpui_component::Root;
use gpui_component_assets::Assets;

fn main() {
    tracing_subscriber::fmt::init();

    gpui_platform::application()
        .with_assets(Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            cx.activate(true);

            cx.spawn(async move |cx| {
                cx.open_window(
                    WindowOptions {
                        titlebar: Some(TitlebarOptions {
                            title: Some("KubeScope".into()),
                            appears_transparent: true,
                            ..Default::default()
                        }),
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(100.0), px(100.0)),
                            size: size(px(1280.0), px(800.0)),
                        })),
                        ..Default::default()
                    },
                    |window, cx| {
                        let view = cx.new(|cx| AppRoot::new(window, cx));
                        let view: AnyView = view.into();
                        cx.new(|cx| Root::new(view, window, cx))
                    },
                )
                .expect("Failed to open window");
            })
            .detach();
        });
}
