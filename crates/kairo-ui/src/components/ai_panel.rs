use gpui::*;
use gpui_component::{
    dock::{Panel, PanelEvent},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    scroll::ScrollableElement,
};

use crate::ai_client::ChatMessage;
use crate::theme::{
    ACCENT, BORDER, HOVER_BG, STATUS_FAILED, SURFACE, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

// ── Events ─────────────────────────────────────────────────────────────────────

/// Emitted when the user submits a message.
#[derive(Clone, Debug)]
pub struct AiSendMessage(pub String);

impl EventEmitter<AiSendMessage> for AiPanel {}
impl EventEmitter<PanelEvent> for AiPanel {}

// ── Message model ──────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub enum AiRole {
    User,
    Assistant,
    Error,
}

#[derive(Clone, Debug)]
pub struct ChatEntry {
    pub role: AiRole,
    /// Display text shown in the chat bubble.
    pub content: String,
    /// If set, this is what gets sent to the API instead of `content`.
    /// Used when a structured prompt is too verbose to display in the UI.
    pub api_content: Option<String>,
}

// ── Component ──────────────────────────────────────────────────────────────────

pub struct AiPanel {
    focus_handle: FocusHandle,
    input: Entity<InputState>,
    messages: Vec<ChatEntry>,
    /// Accumulates tokens during a streaming response.
    streaming_buffer: Option<String>,
    /// Human-readable description of the currently selected K8s resource.
    context_text: Option<String>,
}

const MAX_AI_MESSAGES: usize = 100;

impl AiPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Ask about your cluster…")
        });

        cx.subscribe(&input, |this, state, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let text = state.read(cx).value().to_string();
                this.emit_send(text, cx);
            }
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            input,
            messages: Vec::new(),
            streaming_buffer: None,
            context_text: None,
        }
    }

    // ── Public API (called from Workspace) ────────────────────────────────────

    /// Append the user's message to history and clear the input.
    pub fn push_user_message(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.messages.push(ChatEntry {
            role: AiRole::User,
            content: text.to_string(),
            api_content: None,
        });
        if self.messages.len() > MAX_AI_MESSAGES {
            self.messages.drain(..self.messages.len() - MAX_AI_MESSAGES);
        }
        self.streaming_buffer = Some(String::new());
        self.input.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// Append an AI token to the in-progress streaming buffer.
    pub fn push_token(&mut self, token: &str, cx: &mut Context<Self>) {
        if let Some(buf) = &mut self.streaming_buffer {
            buf.push_str(token);
        } else {
            self.streaming_buffer = Some(token.to_string());
        }
        cx.notify();
    }

    /// Commit the accumulated streaming buffer as a finished assistant message.
    pub fn finish_streaming(&mut self, cx: &mut Context<Self>) {
        if let Some(buf) = self.streaming_buffer.take() {
            if !buf.is_empty() {
                self.messages.push(ChatEntry {
                    role: AiRole::Assistant,
                    content: buf,
                    api_content: None,
                });
                if self.messages.len() > MAX_AI_MESSAGES {
                    self.messages.drain(..self.messages.len() - MAX_AI_MESSAGES);
                }
            }
        }
        cx.notify();
    }

    /// Append an error entry to the chat history.
    pub fn set_error(&mut self, msg: &str, cx: &mut Context<Self>) {
        self.streaming_buffer = None;
        self.messages.push(ChatEntry {
            role: AiRole::Error,
            content: msg.to_string(),
            api_content: None,
        });
        cx.notify();
    }

    /// Update the K8s resource context shown in the header and injected into prompts.
    pub fn set_context(&mut self, desc: String) {
        self.context_text = Some(desc);
    }

    pub fn context_text(&self) -> Option<&str> {
        self.context_text.as_deref()
    }

    pub fn is_streaming(&self) -> bool {
        self.streaming_buffer.is_some()
    }

    /// Push a user message with separate display text and API content.
    ///
    /// The chat bubble shows `display`; the full `api_content` is sent to the LLM.
    pub fn push_analysis_message(
        &mut self,
        display: &str,
        api_content: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.messages.push(ChatEntry {
            role: AiRole::User,
            content: display.to_string(),
            api_content: Some(api_content.to_string()),
        });
        if self.messages.len() > MAX_AI_MESSAGES {
            self.messages.drain(..self.messages.len() - MAX_AI_MESSAGES);
        }
        self.streaming_buffer = Some(String::new());
        self.input.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// Build the messages list for the API call (converts ChatEntry → ChatMessage).
    pub fn build_api_messages(&self) -> Vec<ChatMessage> {
        self.messages
            .iter()
            .filter_map(|e| match e.role {
                AiRole::User => Some(ChatMessage {
                    role: "user".into(),
                    content: e.api_content.as_deref().unwrap_or(&e.content).to_string(),
                }),
                AiRole::Assistant => Some(ChatMessage {
                    role: "assistant".into(),
                    content: e.content.clone(),
                }),
                AiRole::Error => None,
            })
            .collect()
    }

    // ── Private ────────────────────────────────────────────────────────────────

    fn emit_send(&mut self, text: String, cx: &mut Context<Self>) {
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() || self.is_streaming() {
            return;
        }
        cx.emit(AiSendMessage(trimmed));
    }

    fn clear_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.messages.clear();
        self.streaming_buffer = None;
        self.input.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }
}

// ── Focusable + Panel ──────────────────────────────────────────────────────────

impl Focusable for AiPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for AiPanel {
    fn panel_name(&self) -> &'static str {
        "AiPanel"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "AI Agent"
    }

    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> {
        None
    }
}

// ── Render ─────────────────────────────────────────────────────────────────────

impl Render for AiPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let streaming = self.streaming_buffer.clone();
        let context_label = self.context_text.as_deref().map(|c| SharedString::from(format!("⬡ {c}")))
            .unwrap_or_else(|| SharedString::from("No resource selected"));
        let is_streaming = self.is_streaming();
        let messages_empty = self.messages.is_empty();
        let streaming_none = streaming.is_none();
        let messages = self.messages.to_vec();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(SURFACE)
            // ── Toolbar ───────────────────────────────────────────────────────
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(BORDER)
                    .flex_shrink_0()
                    .child(
                        Label::new(context_label)
                            .text_xs()
                            .text_color(TEXT_MUTED),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .hover(|s| s.bg(HOVER_BG))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| this.clear_history(window, cx)),
                            )
                            .child(Label::new("Clear").text_xs().text_color(TEXT_MUTED)),
                    ),
            )
            // ── Message list ──────────────────────────────────────────────────
            .child({
                let mut inner = div()
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .children(messages.into_iter().map(render_entry))
                    .children(streaming.map(render_streaming_entry));
                if messages_empty && streaming_none {
                    inner = inner.child(render_empty_state());
                }
                div().flex_1().min_h_0().child(inner.overflow_y_scrollbar())
            })
            // ── Input footer ──────────────────────────────────────────────────
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(BORDER)
                    .flex_shrink_0()
                    .items_end()
                    .child(div().flex_1().child(Input::new(&self.input).appearance(false)))
                    .child(
                        div()
                            .cursor_pointer()
                            .px(px(10.))
                            .py(px(6.))
                            .rounded(px(6.))
                            .bg(if is_streaming { HOVER_BG } else { ACCENT })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    let text = this.input.read(cx).value().to_string();
                                    this.emit_send(text, cx);
                                }),
                            )
                            .child(
                                Label::new(if is_streaming { "…" } else { "▶" })
                                    .text_xs()
                                    .text_color(TEXT_PRIMARY),
                            ),
                    ),
            )
    }
}

// ── Entry renderers ────────────────────────────────────────────────────────────

fn render_entry(entry: ChatEntry) -> impl IntoElement {
    let (role_label, role_color, bg, align_right) = match entry.role {
        AiRole::User => ("You", ACCENT, rgba(0x313244AA), true),
        AiRole::Assistant => ("AI", TEXT_SECONDARY, rgba(0x1E1E2E00), false),
        AiRole::Error => ("Error", STATUS_FAILED, rgba(0x3D1515AA), false),
    };

    let content = entry.content.clone();

    if align_right {
        // User messages: right-aligned bubble.
        div()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(3.))
            .child(Label::new(role_label).text_xs().text_color(role_color))
            .child(
                div()
                    .max_w(px(320.))
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(8.))
                    .rounded_tr(px(2.))
                    .bg(bg)
                    .text_sm()
                    .text_color(TEXT_PRIMARY)
                    .child(SharedString::from(content)),
            )
    } else {
        // AI / Error messages: left-aligned.
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(3.))
            .child(Label::new(role_label).text_xs().text_color(role_color))
            .child(
                div()
                    .w_full()
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(8.))
                    .rounded_tl(px(2.))
                    .bg(bg)
                    .text_sm()
                    .text_color(TEXT_PRIMARY)
                    .child(SharedString::from(content)),
            )
    }
}

fn render_streaming_entry(buf: String) -> impl IntoElement {
    let display = if buf.is_empty() {
        "▊".to_string()
    } else {
        format!("{buf}▊")
    };

    div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(3.))
        .child(Label::new("AI").text_xs().text_color(TEXT_SECONDARY))
        .child(
            div()
                .w_full()
                .px(px(10.))
                .py(px(7.))
                .rounded(px(8.))
                .rounded_tl(px(2.))
                .text_sm()
                .text_color(TEXT_PRIMARY)
                .child(SharedString::from(display)),
        )
}

fn render_empty_state() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.))
        .mt(px(40.))
        .child(
            Label::new("AI Agent")
                .text_sm()
                .text_color(TEXT_SECONDARY),
        )
        .child(
            Label::new("Ask questions about your cluster, diagnose\npod issues, or get YAML help.")
                .text_xs()
                .text_color(TEXT_MUTED),
        )
        .child(
            div()
                .mt(px(8.))
                .px(px(12.))
                .py(px(6.))
                .rounded(px(6.))
                .border_1()
                .border_color(BORDER)
                .child(
                    Label::new("Configure a provider in Settings (⚙ or Ctrl+,)")
                        .text_xs()
                        .text_color(TEXT_MUTED),
                ),
        )
}
