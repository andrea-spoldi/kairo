use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::{
    dock::{Panel, PanelEvent},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    scroll::ScrollableElement,
    text::TextView,
};
use serde::Deserialize;

use crate::ai_client::ChatMessage;
use crate::scope::AgentScope;
use crate::actions::OpenSettings;
use crate::theme::{
    ACCENT, ACCENT_BG, ACCENT_BORDER, ACCENT_FG, BORDER, HEALTH_ERR, HEALTH_OK, HEALTH_WARN,
    HOVER_BG, SURFACE, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
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
    ToolCall,
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
    /// Number of MCP tools currently available (0 = MCP not connected).
    mcp_tool_count: usize,
    /// Active provider + model shown in the toolbar (e.g. "Anthropic · claude-sonnet-4-6").
    provider_label: Option<String>,
    /// Current investigation scope bound to this AI session.
    scope: AgentScope,
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
            mcp_tool_count: 0,
            provider_label: None,
            scope: AgentScope::None,
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

    /// Append an inline "⬡ Calling <name>…" card when the agent invokes a tool.
    pub fn push_tool_call(&mut self, tool_name: &str, cx: &mut Context<Self>) {
        self.messages.push(ChatEntry {
            role: AiRole::ToolCall,
            content: tool_name.to_string(),
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

    /// Update the MCP tool count shown in the panel header.
    pub fn set_mcp_tool_count(&mut self, n: usize, cx: &mut Context<Self>) {
        self.mcp_tool_count = n;
        cx.notify();
    }

    /// Update the provider + model label shown in the toolbar.
    pub fn set_provider_label(&mut self, label: Option<String>, cx: &mut Context<Self>) {
        self.provider_label = label;
        cx.notify();
    }

    /// Bind this session to an investigation scope (shows scope chip in header).
    pub fn set_scope(&mut self, scope: AgentScope, cx: &mut Context<Self>) {
        self.scope = scope;
        cx.notify();
    }

    /// Inject a system-context block as a hidden user message (visible in chat as small label).
    pub fn push_system_context(&mut self, context_block: String, cx: &mut Context<Self>) {
        if context_block.is_empty() { return; }
        self.messages.push(ChatEntry {
            role: AiRole::User,
            content: "[Context attached — ready for your question]".to_string(),
            api_content: Some(context_block),
        });
        cx.notify();
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
                AiRole::ToolCall | AiRole::Error => None,
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
        let is_streaming = self.is_streaming();
        let messages_empty = self.messages.is_empty();
        let streaming_none = streaming.is_none();
        let messages = self.messages.to_vec();
        let mcp_tool_count = self.mcp_tool_count;
        let provider_label = self.provider_label.clone();
        let scope_label = self.scope.label();
        let quick_actions: Vec<&'static str> = self.scope.quick_actions().to_vec();

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
                    // AI chip — shown when MCP tools are connected
                    .when(mcp_tool_count > 0, |el| {
                        let tools_text = format!("{mcp_tool_count} tools");
                        el.child(
                            h_flex()
                                .rounded(px(12.))
                                .border_1()
                                .border_color(ACCENT_BORDER)
                                .bg(ACCENT_BG)
                                .px(px(8.))
                                .py(px(2.))
                                .gap(px(4.))
                                .items_center()
                                .child(Label::new("⬡").text_xs().text_color(HEALTH_OK))
                                .child(
                                    Label::new(SharedString::from(tools_text))
                                        .text_xs()
                                        .text_color(ACCENT_FG),
                                )
                                .when_some(provider_label, |el, lbl| {
                                    el.child(Label::new("·").text_xs().text_color(TEXT_MUTED))
                                        .child(
                                            Label::new(SharedString::from(lbl))
                                                .text_xs()
                                                .text_color(TEXT_MUTED),
                                        )
                                }),
                        )
                    })
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
                    )
                    // Gear — opens AI/MCP settings
                    .child(
                        div()
                            .cursor_pointer()
                            .px_1()
                            .rounded(px(8.))
                            .hover(|s| s.bg(HOVER_BG))
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                cx.stop_propagation();
                                window.dispatch_action(Box::new(OpenSettings), cx);
                            })
                            .child(Label::new("⚙").text_xl().text_color(TEXT_MUTED)),
                    ),
            )
            // ── Scope chip + quick actions ────────────────────────────────────
            .when_some(scope_label, |el, label| {
                el.child(
                    div()
                        .px_3()
                        .py_1()
                        .border_b_1()
                        .border_color(BORDER)
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .px_2()
                                        .py(px(2.))
                                        .rounded(px(10.))
                                        .border_1()
                                        .border_color(ACCENT_BORDER)
                                        .bg(ACCENT_BG)
                                        .text_xs()
                                        .text_color(TEXT_SECONDARY)
                                        .child(SharedString::from(label)),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(TEXT_MUTED)
                                        .hover(|s| s.text_color(TEXT_PRIMARY))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.scope = AgentScope::None;
                                                cx.notify();
                                            }),
                                        )
                                        .child("×"),
                                ),
                        )
                        .when(!quick_actions.is_empty(), |el| {
                            let weak = cx.entity().downgrade();
                            el.child(
                                h_flex()
                                    .gap(px(4.))
                                    .flex_wrap()
                                    .children(quick_actions.into_iter().enumerate().map(|(i, action)| {
                                        let w = weak.clone();
                                        div()
                                            .id(("quick-action", i))
                                            .px_2()
                                            .py(px(2.))
                                            .rounded(px(8.))
                                            .border_1()
                                            .border_color(BORDER)
                                            .cursor_pointer()
                                            .hover(|s| s.bg(HOVER_BG))
                                            .text_xs()
                                            .text_color(TEXT_MUTED)
                                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                                w.update(cx, |this, cx| {
                                                    this.emit_send(action.to_string(), cx);
                                                }).ok();
                                            })
                                            .child(action)
                                    })),
                            )
                        }),
                )
            })
            // ── Message list ──────────────────────────────────────────────────
            .child({
                let mut inner = div()
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .children(messages.into_iter().enumerate().map(|(i, e)| render_entry(i, e)))
                    .children(streaming.map(render_streaming_entry));
                if messages_empty && streaming_none {
                    inner = inner.child(render_empty_state(self.provider_label.is_some()));
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

// ── Text-based structured analysis (G2) ───────────────────────────────────────

#[derive(Clone)]
enum Confidence { High, Medium, Low }

#[derive(Clone)]
struct TextHypothesis {
    title: String,
    confidence: Confidence,
    evidence: Vec<String>,
}

#[derive(Clone)]
struct ParsedAnalysis {
    hypotheses: Vec<TextHypothesis>,
    next_checks: Vec<String>,
}

fn parse_analysis(text: &str) -> Option<ParsedAnalysis> {
    let trimmed = text.trim();
    if !trimmed.starts_with("HYPOTHESIS") {
        return None;
    }

    let mut hypotheses = Vec::new();
    let mut next_checks = Vec::new();
    let mut in_next_checks = false;
    let mut current_title = String::new();
    let mut current_conf = Confidence::Low;
    let mut current_evidence: Vec<String> = Vec::new();
    let mut in_hypothesis = false;

    for line in trimmed.lines() {
        let line = line.trim();
        if line.starts_with("HYPOTHESIS") {
            if in_hypothesis && !current_title.is_empty() {
                hypotheses.push(TextHypothesis {
                    title: current_title.clone(),
                    confidence: current_conf.clone(),
                    evidence: current_evidence.clone(),
                });
            }
            current_title.clear();
            current_evidence.clear();
            current_conf = Confidence::Low;
            in_hypothesis = true;
            in_next_checks = false;
        } else if line == "NEXT CHECKS" {
            if in_hypothesis && !current_title.is_empty() {
                hypotheses.push(TextHypothesis {
                    title: current_title.clone(),
                    confidence: current_conf.clone(),
                    evidence: current_evidence.clone(),
                });
                in_hypothesis = false;
            }
            in_next_checks = true;
        } else if in_hypothesis {
            if let Some(title) = line.strip_prefix("Title: ") {
                current_title = title.to_string();
            } else if let Some(conf) = line.strip_prefix("Confidence: ") {
                current_conf = match conf.trim() {
                    "High" => Confidence::High,
                    "Medium" => Confidence::Medium,
                    _ => Confidence::Low,
                };
            } else if let Some(bullet) = line.strip_prefix("- ") {
                current_evidence.push(bullet.to_string());
            }
        } else if in_next_checks {
            if let Some(check) = line.strip_prefix("- ") {
                next_checks.push(check.to_string());
            }
        }
    }

    if in_hypothesis && !current_title.is_empty() {
        hypotheses.push(TextHypothesis {
            title: current_title,
            confidence: current_conf,
            evidence: current_evidence,
        });
    }

    if hypotheses.is_empty() { None } else { Some(ParsedAnalysis { hypotheses, next_checks }) }
}

// ── JSON-based analysis response model ────────────────────────────────────────

#[derive(Deserialize)]
struct AnalysisResponse {
    #[serde(default)]
    hypotheses: Vec<Hypothesis>,
    #[serde(default)]
    next_steps: Vec<String>,
}

#[derive(Deserialize)]
struct Hypothesis {
    cause: String,
    #[serde(default = "default_confidence")]
    confidence: String,
    #[serde(default)]
    evidence: Vec<String>,
}

fn default_confidence() -> String {
    "medium".into()
}

/// Try to extract and parse analysis JSON from assistant content.
///
/// Handles raw JSON, markdown-fenced JSON (` ```json ... ``` `), and content
/// that has preamble text before the JSON block.
fn try_parse_analysis(content: &str) -> Option<AnalysisResponse> {
    // Try: markdown code fence
    if let Some(start) = content.find("```json") {
        let json_start = start + 7; // skip ```json
        if let Some(end) = content[json_start..].find("```") {
            let json_str = content[json_start..json_start + end].trim();
            if let Ok(r) = serde_json::from_str::<AnalysisResponse>(json_str) {
                if !r.hypotheses.is_empty() {
                    return Some(r);
                }
            }
        }
    }
    // Try: raw JSON containing "hypotheses"
    if let Some(start) = content.find("{\"hypotheses\"") {
        // Find the matching closing brace
        let slice = &content[start..];
        if let Ok(r) = serde_json::from_str::<AnalysisResponse>(slice) {
            if !r.hypotheses.is_empty() {
                return Some(r);
            }
        }
        // Fallback: braces may not align at end of string, try up to each '}'
        for (i, c) in slice.char_indices().rev() {
            if c == '}' {
                if let Ok(r) = serde_json::from_str::<AnalysisResponse>(&slice[..=i]) {
                    if !r.hypotheses.is_empty() {
                        return Some(r);
                    }
                }
                break;
            }
        }
    }
    None
}

fn confidence_color(level: &str) -> Hsla {
    match level.to_lowercase().as_str() {
        "high" => HEALTH_OK,
        "medium" => HEALTH_WARN,
        _ => HEALTH_ERR,
    }
}

// ── Entry renderers ────────────────────────────────────────────────────────────

fn render_entry(ix: usize, entry: ChatEntry) -> impl IntoElement {
    // Tool call cards are rendered separately before the generic bubble logic.
    if entry.role == AiRole::ToolCall {
        return h_flex()
            .gap(px(4.))
            .items_center()
            .py(px(2.))
            .child(Label::new("⬡").text_xs().text_color(HEALTH_WARN))
            .child(
                Label::new(SharedString::from(format!("Calling {}…", entry.content)))
                    .text_xs()
                    .text_color(TEXT_MUTED),
            )
            .into_any_element();
    }

    let (role_label, role_color, bg, align_right) = match entry.role {
        AiRole::User => ("You", ACCENT, rgba(0x313244AA), true),
        AiRole::Assistant => ("Kairo AI", TEXT_SECONDARY, rgba(0x1E1E2E00), false),
        AiRole::ToolCall => unreachable!(),
        AiRole::Error => ("Error", HEALTH_ERR, rgba(0x3D1515AA), false),
    };

    if align_right {
        let content_copy = entry.content.clone();
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
                    .child(SharedString::from(entry.content)),
            )
            .child(copy_chip_button(content_copy))
            .into_any_element()
    } else if entry.role == AiRole::Assistant {
        // Text-based HYPOTHESIS format takes priority (G2/G3).
        if let Some(parsed) = parse_analysis(&entry.content) {
            return render_parsed_analysis(parsed);
        }
        // Fall back to JSON-based analysis, then plain markdown.
        if let Some(analysis) = try_parse_analysis(&entry.content) {
            return render_analysis_entry(analysis);
        }
        render_plain_assistant(ix, &entry.content)
    } else {
        render_plain_assistant(ix, &entry.content)
    }
}

/// Small inline copy chip used on message bubbles — plain closure, no cx.listener needed.
fn copy_chip_button(text: String) -> impl IntoElement {
    div()
        .cursor_pointer()
        .px(px(5.))
        .py(px(1.))
        .rounded(px(3.))
        .text_xs()
        .text_color(TEXT_MUTED)
        .hover(|s| s.text_color(TEXT_PRIMARY).bg(HOVER_BG))
        .child("⎘")
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        })
}

fn render_plain_assistant(ix: usize, content: &str) -> AnyElement {
    let content_copy = content.to_string();
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
                .child(
                    TextView::markdown(
                        SharedString::from(format!("ai-msg-{ix}")),
                        SharedString::from(content.to_string()),
                    )
                    .selectable(true),
                ),
        )
        .child(copy_chip_button(content_copy))
        .into_any_element()
}

fn render_analysis_entry(analysis: AnalysisResponse) -> AnyElement {
    let mut root = div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(3.))
        .child(Label::new("AI").text_xs().text_color(TEXT_SECONDARY));

    let mut card = div()
        .w_full()
        .px(px(10.))
        .py(px(7.))
        .rounded(px(8.))
        .rounded_tl(px(2.))
        .flex()
        .flex_col()
        .gap(px(10.));

    // ── Hypotheses ────────────────────────────────────────────────────────────
    for h in &analysis.hypotheses {
        let conf_color = confidence_color(&h.confidence);
        let conf_label = h.confidence.to_uppercase();

        let mut hypothesis = div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .px(px(8.))
            .py(px(8.))
            .rounded(px(6.))
            .border_1()
            .border_color(BORDER)
            // Header: confidence badge + cause
            .child(
                h_flex()
                    .gap(px(6.))
                    .child(
                        div()
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(px(3.))
                            .bg(conf_color.opacity(0.15))
                            .child(
                                Label::new(SharedString::from(conf_label))
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(conf_color),
                            ),
                    )
                    .child(
                        Label::new(SharedString::from(h.cause.clone()))
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(TEXT_PRIMARY),
                    ),
            );

        // Evidence bullets
        if !h.evidence.is_empty() {
            let mut evidence_col = div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .pl(px(4.));

            for item in &h.evidence {
                evidence_col = evidence_col.child(
                    Label::new(SharedString::from(format!("  • {item}")))
                        .text_xs()
                        .text_color(TEXT_SECONDARY),
                );
            }
            hypothesis = hypothesis.child(evidence_col);
        }

        card = card.child(hypothesis);
    }

    // ── Next steps ────────────────────────────────────────────────────────────
    if !analysis.next_steps.is_empty() {
        let mut steps = div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(
                Label::new("Next Steps")
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT_MUTED),
            );

        for (i, step) in analysis.next_steps.iter().enumerate() {
            steps = steps.child(
                Label::new(SharedString::from(format!("  {}. {step}", i + 1)))
                    .text_sm()
                    .text_color(TEXT_PRIMARY),
            );
        }
        card = card.child(steps);
    }

    root = root.child(card);
    root.into_any_element()
}

fn render_parsed_analysis(analysis: ParsedAnalysis) -> AnyElement {
    let mut root = div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(8.))
        .child(Label::new("AI").text_xs().text_color(TEXT_SECONDARY));

    // ── Hypothesis cards ──────────────────────────────────────────────────────
    for h in &analysis.hypotheses {
        let (conf_label, conf_color) = match h.confidence {
            Confidence::High   => ("HIGH",   HEALTH_OK),
            Confidence::Medium => ("MED",    HEALTH_WARN),
            Confidence::Low    => ("LOW",    TEXT_MUTED),
        };

        let mut card = div()
            .w_full()
            .rounded(px(16.))
            .border_1()
            .border_color(ACCENT_BORDER)
            .bg(ACCENT_BG)
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(6.))
            // ── Title + confidence badge ───────────────────────────────────────
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_start()
                    .child(
                        Label::new(SharedString::from(h.title.clone()))
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(TEXT_PRIMARY),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(px(4.))
                            .bg(conf_color.opacity(0.15))
                            .flex_shrink_0()
                            .child(
                                Label::new(conf_label)
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(conf_color),
                            ),
                    ),
            );

        // Evidence bullets
        for item in &h.evidence {
            card = card.child(
                Label::new(SharedString::from(format!("• {item}")))
                    .text_xs()
                    .text_color(TEXT_SECONDARY),
            );
        }

        root = root.child(card);
    }

    // ── Next checks card ──────────────────────────────────────────────────────
    if !analysis.next_checks.is_empty() {
        let mut checks_card = div()
            .w_full()
            .rounded(px(16.))
            .border_1()
            .border_color(BORDER)
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                Label::new("NEXT CHECKS")
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT_MUTED),
            );

        for check in &analysis.next_checks {
            checks_card = checks_card.child(
                Label::new(SharedString::from(format!("• {check}")))
                    .text_sm()
                    .text_color(TEXT_PRIMARY),
            );
        }

        root = root.child(checks_card);
    }

    root.into_any_element()
}

/// Returns `true` when the streaming buffer looks like structured analysis
/// (HYPOTHESIS text format or JSON). Hides raw tokens and shows a placeholder.
fn looks_like_analysis(buf: &str) -> bool {
    let trimmed = buf.trim_start();
    if trimmed.starts_with("HYPOTHESIS") {
        return true;
    }
    if trimmed.starts_with("```json") || trimmed.starts_with("```\n{") {
        return true;
    }
    if trimmed.starts_with('{') && trimmed.contains("\"hypothes") {
        return true;
    }
    false
}

fn render_streaming_entry(buf: String) -> impl IntoElement {
    if looks_like_analysis(&buf) {
        return div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(3.))
            .child(Label::new("Kairo AI").text_xs().text_color(TEXT_SECONDARY))
            .child(
                div()
                    .w_full()
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(8.))
                    .rounded_tl(px(2.))
                    .text_sm()
                    .text_color(HEALTH_WARN)
                    .child(SharedString::from("Analyzing situation… ▊")),
            )
            .into_any_element();
    }

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
        .child(Label::new("Kairo AI").text_xs().text_color(TEXT_SECONDARY))
        .child(
            div()
                .w_full()
                .px(px(10.))
                .py(px(7.))
                .rounded(px(8.))
                .rounded_tl(px(2.))
                .child(
                    TextView::markdown("ai-streaming", SharedString::from(display))
                        .selectable(true),
                ),
        )
        .into_any_element()
}

fn render_empty_state(has_provider: bool) -> impl IntoElement {
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
        .when(!has_provider, |el| {
            el.child(
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
        })
}
