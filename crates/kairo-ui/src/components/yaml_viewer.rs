use gpui::*;
use gpui_component::dock::{Panel, PanelEvent};
use gpui_component::h_flex;
use gpui_component::label::Label;
use gpui_component::scroll::ScrollableElement;

use crate::theme::{
    BORDER, SURFACE, TEXT_HEADING, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

// ── YAML syntax-highlight palette ────────────────────────────────────────────

/// YAML key color — blue-ish.
const YAML_KEY: Hsla = Hsla { h: 0.58, s: 0.70, l: 0.68, a: 1.0 };
/// YAML string value color — green-ish.
const YAML_STRING: Hsla = Hsla { h: 0.32, s: 0.60, l: 0.65, a: 1.0 };
/// YAML numeric / boolean value color — orange-ish.
const YAML_SCALAR: Hsla = Hsla { h: 0.07, s: 0.75, l: 0.68, a: 1.0 };
/// YAML comment color.
const YAML_COMMENT: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.45, a: 1.0 };
/// YAML document separator / list bullet.
const YAML_PUNCT: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.50, a: 1.0 };

// ── Panel ─────────────────────────────────────────────────────────────────────

/// Right-panel YAML viewer — shows the raw YAML of any selected resource.
pub struct YamlViewerPanel {
    focus_handle: FocusHandle,
    /// Display title, e.g. "Deployment / my-app".
    pub resource_title: String,
    /// Raw YAML string (None = nothing selected yet).
    pub yaml: Option<String>,
}

impl YamlViewerPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            resource_title: String::new(),
            yaml: None,
        }
    }

    pub fn set_yaml(&mut self, title: String, yaml: String) {
        self.resource_title = title;
        self.yaml = Some(yaml);
    }

    pub fn clear(&mut self) {
        self.resource_title.clear();
        self.yaml = None;
    }
}

impl EventEmitter<PanelEvent> for YamlViewerPanel {}

impl Focusable for YamlViewerPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Panel for YamlViewerPanel {
    fn panel_name(&self) -> &'static str { "YamlViewerPanel" }
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { "YAML" }
    fn zoomable(&self, _: &App) -> Option<gpui_component::dock::PanelControl> { None }
    fn closable(&self, _: &App) -> bool { false }
}

impl Render for YamlViewerPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(yaml) = self.yaml.clone() else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(TEXT_MUTED)
                .child("Select a resource to view its YAML")
                .into_any_element();
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            // Header
            .child(
                h_flex()
                    .px(px(12.))
                    .py(px(8.))
                    .border_b_1()
                    .border_color(BORDER)
                    .bg(SURFACE)
                    .gap(px(8.))
                    .child(
                        Label::new(self.resource_title.clone())
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(TEXT_HEADING),
                    ),
            )
            // Scrollable YAML body
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(render_yaml_body(&yaml)),
            )
            .into_any_element()
    }
}

// ── YAML renderer ─────────────────────────────────────────────────────────────

fn render_yaml_body(yaml: &str) -> impl IntoElement {
    let mut body = div()
        .flex_col()
        .px(px(12.))
        .py(px(8.))
        .font_family("Zed Mono")
        .text_xs();

    for line in yaml.lines() {
        body = body.child(render_yaml_line(line));
    }

    body.overflow_y_scrollbar()
}

/// Render one YAML line with token-level coloring.
fn render_yaml_line(line: &str) -> AnyElement {
    let trimmed = line.trim_start();

    // Document separator or empty
    if trimmed == "---" || trimmed == "..." {
        return div()
            .child(
                Label::new(line.to_string())
                    .text_xs()
                    .text_color(YAML_PUNCT),
            )
            .into_any_element();
    }

    // Comment
    if trimmed.starts_with('#') {
        return div()
            .child(
                Label::new(line.to_string())
                    .text_xs()
                    .text_color(YAML_COMMENT),
            )
            .into_any_element();
    }

    // List item bullet without key (bare `- value`)
    if trimmed.starts_with("- ") || trimmed == "-" {
        let indent_len = line.len() - trimmed.len();
        let indent: String = " ".repeat(indent_len);
        let rest = &trimmed[1..]; // everything after `-`
        return h_flex()
            .child(Label::new(format!("{indent}-")).text_xs().text_color(YAML_PUNCT))
            .child(Label::new(rest.to_string()).text_xs().text_color(TEXT_SECONDARY))
            .into_any_element();
    }

    // Key: value pair
    if let Some(colon_pos) = find_key_colon(trimmed) {
        let indent_len = line.len() - trimmed.len();
        let indent: String = " ".repeat(indent_len);
        let key = &trimmed[..colon_pos];
        let after_colon = &trimmed[colon_pos + 1..]; // includes leading space + value

        // Key-only line (value on next lines or empty)
        let value_str = after_colon.trim();
        if value_str.is_empty() || value_str == "|" || value_str == ">" || value_str == "|-" {
            return h_flex()
                .child(Label::new(format!("{indent}{key}:")).text_xs().text_color(YAML_KEY))
                .child(Label::new(after_colon.to_string()).text_xs().text_color(YAML_PUNCT))
                .into_any_element();
        }

        let value_color = value_color(value_str);
        return h_flex()
            .child(Label::new(format!("{indent}{key}: ")).text_xs().text_color(YAML_KEY))
            .child(Label::new(value_str.to_string()).text_xs().text_color(value_color))
            .into_any_element();
    }

    // Fallback — plain text (continuation lines, multi-line values, etc.)
    div()
        .child(Label::new(line.to_string()).text_xs().text_color(TEXT_PRIMARY))
        .into_any_element()
}

/// Find the position of `:` that separates a YAML key from its value.
/// Skips colons inside quoted strings.
fn find_key_colon(s: &str) -> Option<usize> {
    let mut in_quote = false;
    let mut quote_char = ' ';
    for (i, ch) in s.char_indices() {
        if in_quote {
            if ch == quote_char { in_quote = false; }
        } else {
            match ch {
                '"' | '\'' => { in_quote = true; quote_char = ch; }
                ':' => {
                    // must be followed by space or end of string to count as YAML key separator
                    let rest = &s[i + 1..];
                    if rest.is_empty() || rest.starts_with(' ') {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Choose text color based on the YAML value content.
fn value_color(v: &str) -> Hsla {
    // Null / boolean / numeric → scalar color
    if matches!(v, "null" | "~" | "true" | "false") {
        return YAML_SCALAR;
    }
    if v.starts_with('\'') || v.starts_with('"') {
        return YAML_STRING;
    }
    // Pure number
    if v.parse::<f64>().is_ok() {
        return YAML_SCALAR;
    }
    // Anchors / aliases
    if v.starts_with('&') || v.starts_with('*') {
        return YAML_PUNCT;
    }
    // Default: treat as string
    TEXT_SECONDARY
}

// ── Resource-kind label helper ─────────────────────────────────────────────────

/// Format a YAML viewer title from kind + optional namespace + name.
pub fn yaml_title(kind: &str, namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("{kind}  /  {ns}/{name}"),
        _ => format!("{kind}  /  {name}"),
    }
}
