mod actions;
mod ai_client;
mod analyze;
mod app;
mod components;
mod kube_runtime;
mod mcp_client;
mod theme;

use app::Workspace;
use gpui::*;
use gpui_component::{Root, TitleBar};
use gpui_component_assets::Assets;

use actions::{
    CloseCommandPalette, ConfirmSelection, FocusSearch, NavigateDown, NavigateUp,
    OpenCommandPalette, OpenSettings, ToggleGrouping, YankName,
};

/// macOS app bundles (and Linux .desktop launchers) don't inherit the user's
/// login-shell PATH. Kubeconfig exec-credential plugins (aws-iam-authenticator,
/// gke-gcloud-auth-plugin, kubelogin, …) live in Homebrew/user-local dirs that
/// are absent from the system PATH. Spawn a login shell once at startup to
/// capture its PATH and apply it to the process environment.
fn fix_exec_path() {
    if let Ok(shell) = std::env::var("SHELL") {
        if let Ok(out) = std::process::Command::new(&shell)
            .args(["-l", "-c", "echo $PATH"])
            .output()
        {
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout);
                let path = path.trim();
                if !path.is_empty() {
                    std::env::set_var("PATH", path);
                    return;
                }
            }
        }
    }
    // Fallback: prepend the most common Homebrew / user-local locations.
    let cur = std::env::var("PATH").unwrap_or_default();
    std::env::set_var(
        "PATH",
        format!("/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:{cur}"),
    );
}

fn main() {
    // Must run before kube_runtime::init() so exec credential plugins are
    // resolvable inside the tokio runtime's spawned tasks.
    fix_exec_path();

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

            // Global shortcuts (no context — fire anywhere).
            cx.bind_keys([
                KeyBinding::new("cmd-k",     OpenCommandPalette, None),
                KeyBinding::new("ctrl-k",    OpenCommandPalette, None),
                KeyBinding::new("cmd-,",     OpenSettings,       None),
                KeyBinding::new("ctrl-,",    OpenSettings,       None),
            ]);

            // Pod list navigation (fires only when PodList key context is focused).
            // "&& !Input" prevents these from firing when a text field inside
            // the PodList panel has focus (Input declares key_context("Input")).
            cx.bind_keys([
                KeyBinding::new("j",      NavigateDown,      Some("PodList && !Input")),
                KeyBinding::new("k",      NavigateUp,        Some("PodList && !Input")),
                KeyBinding::new("down",   NavigateDown,      Some("PodList && !Input")),
                KeyBinding::new("up",     NavigateUp,        Some("PodList && !Input")),
                KeyBinding::new("return", ConfirmSelection,  Some("PodList && !Input")),
                KeyBinding::new("/",      FocusSearch,       Some("PodList && !Input")),
                KeyBinding::new("g",      ToggleGrouping,    Some("PodList && !Input")),
                KeyBinding::new("y",      YankName,          Some("PodList && !Input")),
            ]);

            // Palette navigation (fires only when Palette key context is focused).
            cx.bind_keys([
                KeyBinding::new("down",   NavigateDown,       Some("Palette")),
                KeyBinding::new("up",     NavigateUp,         Some("Palette")),
                KeyBinding::new("return", ConfirmSelection,   Some("Palette")),
                KeyBinding::new("escape", CloseCommandPalette, Some("Palette")),
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
