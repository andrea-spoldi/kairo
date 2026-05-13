mod actions;
mod ai_client;
mod analyze;
mod app;
mod components;
mod kube_runtime;
mod mcp_client;
mod scope;
mod stories;
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
    // Check for story mode before any heavy initialisation.
    let story_name = std::env::args()
        .skip_while(|a| a != "--story")
        .nth(1);

    // Must run before kube_runtime::init() so exec credential plugins are
    // resolvable inside the tokio runtime's spawned tasks.
    fix_exec_path();

    tracing_subscriber::fmt::init();

    // The full story renders a real Workspace so it needs the kube runtime
    // (even though no cluster is connected).  Isolated component stories don't.
    let needs_kube_runtime = story_name.is_none() || story_name.as_deref() == Some("full");
    if needs_kube_runtime {
        kube_runtime::init();
    }

    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            gpui_component::theme::Theme::change(
                gpui_component::theme::ThemeMode::Dark,
                None,
                cx,
            );
            cx.activate(true);

            // Register all keybindings up front — full story mode renders a real
            // Workspace so it needs them; isolated component stories ignore them.
            cx.bind_keys([
                KeyBinding::new("cmd-k",     OpenCommandPalette, None),
                KeyBinding::new("ctrl-k",    OpenCommandPalette, None),
                KeyBinding::new("cmd-,",     OpenSettings,       None),
                KeyBinding::new("ctrl-,",    OpenSettings,       None),
            ]);
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
            cx.bind_keys([
                KeyBinding::new("down",   NavigateDown,       Some("Palette")),
                KeyBinding::new("up",     NavigateUp,         Some("Palette")),
                KeyBinding::new("return", ConfirmSelection,   Some("Palette")),
                KeyBinding::new("escape", CloseCommandPalette, Some("Palette")),
            ]);

            if let Some(name) = &story_name {
                stories::run(name, cx);
                return;
            }

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
                        // Lets the compositor (X11 WM_CLASS / Wayland app_id) look up
                        // the icon from the system icon theme or the installed .desktop file.
                        app_id: Some("kairo".to_string()),
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
