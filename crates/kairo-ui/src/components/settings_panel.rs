use gpui::*;
use gpui_component::{
    h_flex,
    input::{Input, InputState},
    label::Label,
    scroll::ScrollableElement,
};
use kairo_config::{KairoConfig, ActiveProvider, AiConfig, AnthropicConfig, OpenAiConfig, OllamaConfig, McpConfig};

fn parse_u32(s: &str, default: u32) -> u32 {
    s.parse::<u32>().unwrap_or(default)
}

use tracing::error;

use crate::theme::{ACCENT, BORDER, HOVER_BG, SURFACE, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY};

// ── Events ─────────────────────────────────────────────────────────────────────

/// Emitted after the user saves settings successfully.
#[derive(Clone, Debug)]
pub struct SettingsSaved(pub KairoConfig);

impl EventEmitter<SettingsSaved> for SettingsPanel {}

/// Emitted when the user clicks "Test Connection" on the MCP tab.
/// `app.rs` handles this by spawning a tokio task (reqwest can't run on smol).
#[derive(Clone, Debug)]
pub struct McpTestRequest(pub String);

impl EventEmitter<McpTestRequest> for SettingsPanel {}

// ── Internal types ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
enum ProviderTab {
    Anthropic,
    OpenAi,
    Ollama,
    Mcp,
}

#[derive(Clone)]
enum SaveStatus {
    Idle,
    Error(String),
}

// ── Component ──────────────────────────────────────────────────────────────────

pub struct SettingsPanel {
    visible: bool,
    active_tab: ProviderTab,
    active_provider: ActiveProvider,
    save_status: SaveStatus,
    focus_handle: FocusHandle,

    // ── Anthropic fields ───────────────────────────────────────────────────────
    anthropic_key: Entity<InputState>,
    anthropic_model: Entity<InputState>,
    anthropic_max_tokens: Entity<InputState>,

    // ── OpenAI fields ──────────────────────────────────────────────────────────
    openai_key: Entity<InputState>,
    openai_base_url: Entity<InputState>,
    openai_model: Entity<InputState>,
    openai_max_tokens: Entity<InputState>,

    // ── Ollama fields ──────────────────────────────────────────────────────────
    ollama_base_url: Entity<InputState>,
    ollama_model: Entity<InputState>,
    ollama_max_tokens: Entity<InputState>,

    // ── MCP fields ─────────────────────────────────────────────────────────────
    mcp_server_url: Entity<InputState>,
    mcp_enabled: bool,
    test_status: String,
    test_tools: Vec<String>,
}

impl SettingsPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mk = |ph: &'static str| {
            move |cx: &mut Context<SettingsPanel>, window: &mut Window| {
                cx.new(|cx| InputState::new(window, cx).placeholder(ph))
            }
        };

        Self {
            visible: false,
            active_tab: ProviderTab::Anthropic,
            active_provider: ActiveProvider::None,
            save_status: SaveStatus::Idle,
            focus_handle: cx.focus_handle(),

            anthropic_key: mk("sk-ant-...")(cx, window),
            anthropic_model: mk("claude-sonnet-4-6")(cx, window),
            anthropic_max_tokens: mk("1024")(cx, window),

            openai_key: mk("sk-...")(cx, window),
            openai_base_url: mk("https://api.openai.com/v1")(cx, window),
            openai_model: mk("gpt-4o")(cx, window),
            openai_max_tokens: mk("1024")(cx, window),

            ollama_base_url: mk("http://localhost:11434")(cx, window),
            ollama_model: mk("llama3.2")(cx, window),
            ollama_max_tokens: mk("2048")(cx, window),

            mcp_server_url: mk("http://localhost:8811")(cx, window),
            mcp_enabled: false,
            test_status: String::new(),
            test_tools: Vec::new(),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Open the panel, loading the latest config from disk into the fields.
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = KairoConfig::load().unwrap_or_default();
        self.active_provider = cfg.ai.active_provider.clone();
        self.mcp_enabled = cfg.mcp.enabled;
        self.populate_from_config(&cfg, window, cx);
        self.save_status = SaveStatus::Idle;
        self.visible = true;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        cx.notify();
    }

    /// Called by `app.rs` with the result of the "Test Connection" attempt.
    pub fn set_mcp_test_result(
        &mut self,
        status: String,
        tools: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.test_status = status;
        self.test_tools = tools;
        cx.notify();
    }

    // ── Private ────────────────────────────────────────────────────────────────

    fn populate_from_config(&self, cfg: &KairoConfig, window: &mut Window, cx: &mut Context<Self>) {
        let a = &cfg.ai.anthropic;
        let o = &cfg.ai.openai;
        let ol = &cfg.ai.ollama;
        let ak = a.api_key.clone();
        let am = a.model.clone();
        let at = a.max_tokens.to_string();
        let ok = o.api_key.clone();
        let ob = o.base_url.clone();
        let om = o.model.clone();
        let ot = o.max_tokens.to_string();
        let olb = ol.base_url.clone();
        let olm = ol.model.clone();
        let olt = ol.max_tokens.to_string();

        self.anthropic_key.update(cx, |s, cx| s.set_value(ak, window, cx));
        self.anthropic_model.update(cx, |s, cx| s.set_value(am, window, cx));
        self.anthropic_max_tokens.update(cx, |s, cx| s.set_value(at, window, cx));
        self.openai_key.update(cx, |s, cx| s.set_value(ok, window, cx));
        self.openai_base_url.update(cx, |s, cx| s.set_value(ob, window, cx));
        self.openai_model.update(cx, |s, cx| s.set_value(om, window, cx));
        self.openai_max_tokens.update(cx, |s, cx| s.set_value(ot, window, cx));
        self.ollama_base_url.update(cx, |s, cx| s.set_value(olb, window, cx));
        self.ollama_model.update(cx, |s, cx| s.set_value(olm, window, cx));
        self.ollama_max_tokens.update(cx, |s, cx| s.set_value(olt, window, cx));

        let mcp_url = cfg.mcp.server_url.clone();
        self.mcp_server_url.update(cx, |s, cx| s.set_value(mcp_url, window, cx));
    }

    fn build_config(&self, cx: &mut Context<Self>) -> KairoConfig {
        KairoConfig {
            ai: AiConfig {
                active_provider: self.active_provider.clone(),
                anthropic: AnthropicConfig {
                    api_key: self.anthropic_key.read(cx).value().to_string(),
                    model: self.anthropic_model.read(cx).value().to_string(),
                    max_tokens: parse_u32(
                        self.anthropic_max_tokens.read(cx).value().as_ref(),
                        1024,
                    ),
                },
                openai: OpenAiConfig {
                    api_key: self.openai_key.read(cx).value().to_string(),
                    base_url: self.openai_base_url.read(cx).value().to_string(),
                    model: self.openai_model.read(cx).value().to_string(),
                    max_tokens: parse_u32(
                        self.openai_max_tokens.read(cx).value().as_ref(),
                        1024,
                    ),
                },
                ollama: OllamaConfig {
                    base_url: self.ollama_base_url.read(cx).value().to_string(),
                    model: self.ollama_model.read(cx).value().to_string(),
                    max_tokens: parse_u32(
                        self.ollama_max_tokens.read(cx).value().as_ref(),
                        2048,
                    ),
                },
            },
            mcp: McpConfig {
                enabled: self.mcp_enabled,
                server_url: self.mcp_server_url.read(cx).value().to_string(),
            },
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let cfg = self.build_config(cx);
        match cfg.save() {
            Ok(()) => {
                cx.emit(SettingsSaved(cfg));
                self.hide(cx);
            }
            Err(e) => {
                error!("failed to save settings: {e}");
                self.save_status = SaveStatus::Error(e.to_string());
                cx.notify();
            }
        }
    }
}

// ── Render ─────────────────────────────────────────────────────────────────────

impl Render for SettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().into_any_element();
        }

        let active_tab = self.active_tab.clone();
        let active_provider = self.active_provider.clone();
        let mcp_enabled = self.mcp_enabled;
        let save_status = self.save_status.clone();

        // Full-screen backdrop.
        div()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.hide(cx)))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .flex()
            .justify_center()
            .pt(px(60.))
            .child(
                div()
                    .w(px(600.))
                    .max_h(px(580.))
                    .bg(rgba(0x1E1E2EFF))
                    .border_1()
                    .border_color(BORDER)
                    .rounded(px(10.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .track_focus(&self.focus_handle)
                    // ── Header ────────────────────────────────────────────────
                    .child(
                        h_flex()
                            .px(px(16.))
                            .py(px(12.))
                            .border_b_1()
                            .border_color(BORDER)
                            .gap(px(8.))
                            .child(
                                Label::new("Settings")
                                    .text_sm()
                                    .text_color(TEXT_PRIMARY),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .cursor_pointer()
                                    .px_2()
                                    .rounded(px(4.))
                                    .hover(|s| s.bg(HOVER_BG))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.hide(cx)),
                                    )
                                    .child(Label::new("✕").text_xs().text_color(TEXT_MUTED)),
                            ),
                    )
                    // ── Provider tabs ─────────────────────────────────────────
                    .child(render_provider_tabs(active_tab.clone(), active_provider.clone(), mcp_enabled, cx))
                    // Divider below tabs.
                    .child(div().h_px().bg(BORDER))
                    // ── Tab content ───────────────────────────────────────────
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .px(px(16.))
                            .py(px(12.))
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .child(match active_tab {
                                ProviderTab::Anthropic => render_anthropic_fields(self, cx),
                                ProviderTab::OpenAi => render_openai_fields(self, cx),
                                ProviderTab::Ollama => render_ollama_fields(self, cx),
                                ProviderTab::Mcp => render_mcp_fields(self, cx),
                            }),
                    )
                    // ── Footer ────────────────────────────────────────────────
                    .child(div().h_px().bg(BORDER))
                    .child(render_footer(save_status, cx)),
            )
            .into_any_element()
    }
}

// ── Sub-renderers ──────────────────────────────────────────────────────────────

fn render_provider_tabs(
    active_tab: ProviderTab,
    active_provider: ActiveProvider,
    mcp_enabled: bool,
    cx: &mut Context<SettingsPanel>,
) -> impl IntoElement {
    h_flex()
        .px(px(16.))
        .py(px(8.))
        .gap(px(4.))
        .child(tab_button(
            "Anthropic",
            ProviderTab::Anthropic,
            &active_tab,
            active_provider == ActiveProvider::Anthropic,
            cx,
        ))
        .child(tab_button(
            "OpenAI",
            ProviderTab::OpenAi,
            &active_tab,
            active_provider == ActiveProvider::OpenAi,
            cx,
        ))
        .child(tab_button(
            "Ollama",
            ProviderTab::Ollama,
            &active_tab,
            active_provider == ActiveProvider::Ollama,
            cx,
        ))
        .child(tab_button(
            "MCP",
            ProviderTab::Mcp,
            &active_tab,
            mcp_enabled,
            cx,
        ))
}

fn tab_button(
    label: &'static str,
    tab: ProviderTab,
    active_tab: &ProviderTab,
    is_active_provider: bool,
    cx: &mut Context<SettingsPanel>,
) -> impl IntoElement {
    let is_selected = tab == *active_tab;
    let tab_clone = tab.clone();

    let mut el = div()
        .cursor_pointer()
        .px(px(12.))
        .py(px(5.))
        .rounded(px(6.))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.active_tab = tab_clone.clone();
                cx.notify();
            }),
        );

    if is_selected {
        el = el.bg(HOVER_BG).border_1().border_color(BORDER);
    }

    let label_color = if is_selected {
        TEXT_PRIMARY
    } else {
        TEXT_MUTED
    };

    let indicator = if is_active_provider { " ●" } else { "" };

    el.child(
        h_flex()
            .gap(px(4.))
            .child(Label::new(label).text_xs().text_color(label_color))
            .child(Label::new(indicator).text_xs().text_color(ACCENT)),
    )
}

fn field_row(label: &'static str, input: &Entity<InputState>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(Label::new(label).text_xs().text_color(TEXT_MUTED))
        .child(Input::new(input))
}

fn section_header(title: &'static str, provider: ActiveProvider, is_active: bool, cx: &mut Context<SettingsPanel>) -> impl IntoElement {
    h_flex()
        .gap(px(8.))
        .mb(px(4.))
        .child(Label::new(title).text_sm().text_color(TEXT_SECONDARY))
        .child(div().flex_1())
        .child(
            div()
                .cursor_pointer()
                .px(px(8.))
                .py(px(3.))
                .rounded(px(4.))
                .border_1()
                .border_color(if is_active { ACCENT } else { BORDER })
                .hover(|s| s.bg(HOVER_BG))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.active_provider = provider.clone();
                        cx.notify();
                    }),
                )
                .child(
                    Label::new(if is_active { "● Active" } else { "Set active" })
                        .text_xs()
                        .text_color(if is_active { ACCENT } else { TEXT_MUTED }),
                ),
        )
}

fn render_anthropic_fields(panel: &SettingsPanel, cx: &mut Context<SettingsPanel>) -> AnyElement {
    let is_active = panel.active_provider == ActiveProvider::Anthropic;
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(section_header("Anthropic", ActiveProvider::Anthropic, is_active, cx))
        .child(field_row("API Key", &panel.anthropic_key))
        .child(field_row("Model", &panel.anthropic_model))
        .child(field_row("Max Tokens", &panel.anthropic_max_tokens))
        .into_any_element()
}

fn render_openai_fields(panel: &SettingsPanel, cx: &mut Context<SettingsPanel>) -> AnyElement {
    let is_active = panel.active_provider == ActiveProvider::OpenAi;
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(section_header("OpenAI", ActiveProvider::OpenAi, is_active, cx))
        .child(field_row("API Key", &panel.openai_key))
        .child(field_row("Base URL", &panel.openai_base_url))
        .child(field_row("Model", &panel.openai_model))
        .child(field_row("Max Tokens", &panel.openai_max_tokens))
        .into_any_element()
}

fn render_ollama_fields(panel: &SettingsPanel, cx: &mut Context<SettingsPanel>) -> AnyElement {
    let is_active = panel.active_provider == ActiveProvider::Ollama;
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(section_header("Ollama", ActiveProvider::Ollama, is_active, cx))
        .child(field_row("Base URL", &panel.ollama_base_url))
        .child(field_row("Model", &panel.ollama_model))
        .child(field_row("Max Tokens", &panel.ollama_max_tokens))
        .into_any_element()
}

fn render_mcp_fields(panel: &SettingsPanel, cx: &mut Context<SettingsPanel>) -> AnyElement {
    let is_enabled = panel.mcp_enabled;
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            h_flex()
                .gap(px(8.))
                .mb(px(4.))
                .child(
                    Label::new("Kubernetes MCP Server")
                        .text_sm()
                        .text_color(TEXT_SECONDARY),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .cursor_pointer()
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(if is_enabled { ACCENT } else { BORDER })
                        .hover(|s| s.bg(HOVER_BG))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.mcp_enabled = !this.mcp_enabled;
                                cx.notify();
                            }),
                        )
                        .child(
                            Label::new(if is_enabled { "● Enabled" } else { "Enable" })
                                .text_xs()
                                .text_color(if is_enabled { ACCENT } else { TEXT_MUTED }),
                        ),
                ),
        )
        .child(field_row("Server URL", &panel.mcp_server_url))
        .child(
            div()
                .cursor_pointer()
                .px(px(8.))
                .py(px(3.))
                .rounded(px(4.))
                .border_1()
                .border_color(HOVER_BG)
                .hover(|s| s.bg(HOVER_BG))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        let url = this.mcp_server_url.read(cx).value().to_string();
                        this.test_status = "Connecting…".to_string();
                        this.test_tools.clear();
                        cx.notify();
                        // Delegate to app.rs which runs this on the tokio runtime
                        // (reqwest cannot run on GPUI's smol executor).
                        cx.emit(McpTestRequest(url));
                    }),
                )
                .child(Label::new("Test Connection").text_xs()),
        )
        .child(Label::new(&panel.test_status).text_xs().text_color(TEXT_MUTED))
        .child(Label::new(format!("Tools: {}", panel.test_tools.join(", "))).text_xs().text_color(TEXT_MUTED))
        .into_any_element()



}

fn render_footer(save_status: SaveStatus, cx: &mut Context<SettingsPanel>) -> impl IntoElement {
    h_flex()
        .px(px(16.))
        .py(px(10.))
        .gap(px(8.))
        .child(match &save_status {
            SaveStatus::Error(msg) => div()
                .text_xs()
                .text_color(crate::theme::STATUS_FAILED)
                .child(SharedString::from(format!("Error: {msg}")))
                .into_any_element(),
            SaveStatus::Idle => div().flex_1().into_any_element(),
        })
        .child(div().flex_1())
        // Cancel button
        .child(
            div()
                .cursor_pointer()
                .px(px(12.))
                .py(px(6.))
                .rounded(px(6.))
                .border_1()
                .border_color(BORDER)
                .hover(|s| s.bg(HOVER_BG))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.hide(cx)),
                )
                .child(Label::new("Cancel").text_xs().text_color(TEXT_MUTED)),
        )
        // Save button
        .child(
            div()
                .cursor_pointer()
                .px(px(12.))
                .py(px(6.))
                .rounded(px(6.))
                .bg(ACCENT)
                .hover(|s| s.bg(SURFACE))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.save(cx)),
                )
                .child(Label::new("Save Settings").text_xs().text_color(TEXT_PRIMARY)),
        )
}

// ── Helpers ────────────────────────────────────────────────────────────────────

// parse_u32 already defined above
