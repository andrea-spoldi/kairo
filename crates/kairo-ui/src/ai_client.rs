//! Streaming AI client — supports Anthropic, OpenAI-compatible, and Ollama.
//!
//! All three providers are called from the dedicated tokio runtime thread
//! (see `kube_runtime`). The caller receives incremental tokens via an mpsc
//! channel and is responsible for pushing them into the GPUI event queue.
//!
//! When an `McpClient` is supplied with a non-empty tool list, Anthropic and
//! OpenAI providers run an agentic loop: if the model returns tool-use blocks
//! we call the MCP server, inject the results, and re-query the model until it
//! produces a plain text response.

use std::{collections::HashMap, sync::Arc};

use anyhow::{bail, Context as _, Result};
use futures_util::StreamExt;
use kairo_config::{ActiveProvider, AnthropicConfig, KairoConfig, OllamaConfig, OpenAiConfig};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::mcp_client::{McpClient, McpTool};

// ── Curated prompts ────────────────────────────────────────────────────────────

/// System prompt for the Kubernetes SRE AI agent.
pub const SYSTEM_PROMPT: &str = r#"You are an expert Kubernetes SRE assistant.

Your job is to:
- analyze cluster situations
- propose likely causes
- explain reasoning based on evidence
- suggest next investigative steps

Rules:
- do not hallucinate missing data
- always explain why
- provide multiple hypotheses when possible
- include confidence levels (low, medium, high)
- prioritize actionable insights"#;

/// Build the structured user prompt for event analysis.
///
/// `context_json` is a JSON string describing the event or resource to analyze.
/// The prompt instructs the model to return a structured JSON response.
pub fn build_event_analysis_prompt(context_json: &str) -> String {
    format!(
        r#"Analyze the following Kubernetes situation.

Return:
1. Likely causes
2. Supporting evidence
3. What to check next
4. Confidence level

Context:
<JSON>
{context_json}
</JSON>

Respond in JSON format:
```json
{{
  "hypotheses": [
    {{
      "cause": "string",
      "confidence": "low|medium|high",
      "evidence": ["string"]
    }}
  ],
  "next_steps": ["string"]
}}
```"#
    )
}

// ── Public types ───────────────────────────────────────────────────────────────

/// A single turn in the conversation sent to the API.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// `"user"` or `"assistant"`.
    pub role: String,
    pub content: String,
}

/// Incremental output from the streaming call.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    Token(String),
    /// The model is about to call the named MCP tool.
    ToolCallStart(String),
    Done,
}

// ── Entry point ────────────────────────────────────────────────────────────────

/// Stream a completion from the active provider.
///
/// `system_prompt` is injected as a system-level instruction.  `messages`
/// contains the full conversation history.  When `mcp_client` and `mcp_tools`
/// are provided, the Anthropic and OpenAI providers run an agentic loop so
/// the model can call MCP tools before producing its final answer.
pub async fn stream_completion(
    config: KairoConfig,
    system_prompt: String,
    messages: Vec<ChatMessage>,
    mcp_client: Option<Arc<McpClient>>,
    mcp_tools: Vec<McpTool>,
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()> {
    match config.ai.active_provider {
        ActiveProvider::Anthropic => {
            stream_anthropic(config.ai.anthropic, system_prompt, messages, mcp_client, &mcp_tools, tx).await
        }
        ActiveProvider::OpenAi => {
            stream_openai(config.ai.openai, system_prompt, messages, mcp_client, &mcp_tools, tx).await
        }
        ActiveProvider::Ollama => {
            stream_ollama(config.ai.ollama, system_prompt, messages, mcp_client, &mcp_tools, tx).await
        }
        ActiveProvider::None => {
            bail!(
                "No AI provider configured. Click ⚙ in the title bar \
                 (or press Ctrl+,) to open Settings and set an active provider."
            );
        }
    }
}

// ── Tool schema helpers ────────────────────────────────────────────────────────

fn tools_to_anthropic(tools: &[McpTool]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect(),
    )
}

fn tools_to_openai(tools: &[McpTool]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect(),
    )
}

// ── Anthropic ──────────────────────────────────────────────────────────────────

/// Tracks a tool_use block being assembled from streaming deltas.
struct PendingAnthropicTool {
    id: String,
    name: String,
    input_buf: String,
}

async fn stream_anthropic(
    cfg: AnthropicConfig,
    system_prompt: String,
    messages: Vec<ChatMessage>,
    mcp_client: Option<Arc<McpClient>>,
    tools: &[McpTool],
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()> {
    if cfg.api_key.trim().is_empty() {
        bail!("Anthropic API key is not set. Open Settings (⚙) to add it.");
    }

    // Convert initial chat history to Anthropic's wire format.
    let mut api_messages: Vec<Value> = messages
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();

    let tool_defs = if tools.is_empty() {
        None
    } else {
        Some(tools_to_anthropic(tools))
    };

    let client = reqwest::Client::new();
    const MAX_TOOL_ITERATIONS: usize = 10;

    for _ in 0..MAX_TOOL_ITERATIONS {
        let mut body = json!({
            "model": cfg.model,
            "max_tokens": cfg.max_tokens,
            "stream": true,
            "system": system_prompt,
            "messages": api_messages,
        });
        if let Some(ref td) = tool_defs {
            body["tools"] = td.clone();
            body["tool_choice"] = json!({"type": "auto"});
        }

        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", cfg.api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Anthropic: failed to connect")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            bail!("Anthropic API {status}: {text}");
        }

        let mut buf = String::new();
        let mut stream = response.bytes_stream();

        let mut current_tool: Option<PendingAnthropicTool> = None;
        let mut pending_tools: Vec<PendingAnthropicTool> = Vec::new();
        let mut stop_reason: Option<String> = None;
        let mut current_text = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("Anthropic: stream read error")?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);

                let Some(data) = line.strip_prefix("data: ") else {
                    continue;
                };
                let Ok(v) = serde_json::from_str::<Value>(data) else {
                    continue;
                };

                match v.get("type").and_then(Value::as_str).unwrap_or("") {
                    "content_block_start" => {
                        if v.pointer("/content_block/type").and_then(Value::as_str)
                            == Some("tool_use")
                        {
                            current_tool = Some(PendingAnthropicTool {
                                id: v
                                    .pointer("/content_block/id")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                name: v
                                    .pointer("/content_block/name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                input_buf: String::new(),
                            });
                        }
                    }
                    "content_block_delta" => {
                        match v.pointer("/delta/type").and_then(Value::as_str).unwrap_or("") {
                            "text_delta" => {
                                if let Some(text) =
                                    v.pointer("/delta/text").and_then(Value::as_str)
                                {
                                    if !text.is_empty() {
                                        current_text.push_str(text);
                                        let _ = tx
                                            .send(StreamChunk::Token(text.to_string()))
                                            .await;
                                    }
                                }
                            }
                            "input_json_delta" => {
                                if let Some(partial) =
                                    v.pointer("/delta/partial_json").and_then(Value::as_str)
                                {
                                    if let Some(ref mut t) = current_tool {
                                        t.input_buf.push_str(partial);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    "content_block_stop" => {
                        if let Some(tool) = current_tool.take() {
                            pending_tools.push(tool);
                        }
                    }
                    "message_delta" => {
                        if let Some(reason) =
                            v.pointer("/delta/stop_reason").and_then(Value::as_str)
                        {
                            stop_reason = Some(reason.to_string());
                        }
                    }
                    "message_stop" => break,
                    _ => {}
                }
            }
        }

        if stop_reason.as_deref() == Some("tool_use") && !pending_tools.is_empty() {
            // Build the assistant content block (text + tool_use entries).
            let mut assistant_content: Vec<Value> = Vec::new();
            if !current_text.is_empty() {
                assistant_content.push(json!({"type": "text", "text": current_text}));
            }
            for tool in &pending_tools {
                let input: Value =
                    serde_json::from_str(&tool.input_buf).unwrap_or(json!({}));
                assistant_content.push(json!({
                    "type": "tool_use",
                    "id": tool.id,
                    "name": tool.name,
                    "input": input,
                }));
            }
            api_messages.push(json!({"role": "assistant", "content": assistant_content}));

            let mcp = mcp_client
                .as_ref()
                .context("MCP tool call requested but no MCP client is connected")?;

            let mut tool_results: Vec<Value> = Vec::new();
            for tool in &pending_tools {
                let _ = tx.send(StreamChunk::ToolCallStart(tool.name.clone())).await;
                let args: Value =
                    serde_json::from_str(&tool.input_buf).unwrap_or(json!({}));
                let result = mcp
                    .call_tool(&tool.name, args)
                    .await
                    .unwrap_or_else(|e| format!("Tool call error: {e}"));
                tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tool.id,
                    "content": result,
                }));
            }
            api_messages.push(json!({"role": "user", "content": tool_results}));

            // Loop back to query the model with the tool results.
            continue;
        }

        // No tool use — streaming is complete.
        let _ = tx.send(StreamChunk::Done).await;
        return Ok(());
    }

    bail!("Anthropic: exceeded maximum tool-use iterations ({MAX_TOOL_ITERATIONS})");
}

// ── OpenAI (and compatible endpoints) ─────────────────────────────────────────

struct PendingOpenAiTool {
    id: String,
    name: String,
    args_buf: String,
}

async fn stream_openai(
    cfg: OpenAiConfig,
    system_prompt: String,
    messages: Vec<ChatMessage>,
    mcp_client: Option<Arc<McpClient>>,
    tools: &[McpTool],
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()> {
    if cfg.api_key.trim().is_empty() {
        bail!("OpenAI API key is not set. Open Settings (⚙) to add it.");
    }

    let mut api_messages: Vec<Value> = vec![json!({"role": "system", "content": system_prompt})];
    for m in &messages {
        api_messages.push(json!({"role": m.role, "content": m.content}));
    }

    let tool_defs = if tools.is_empty() {
        None
    } else {
        Some(tools_to_openai(tools))
    };

    let base_url = cfg.base_url.trim_end_matches('/');
    let url = format!("{base_url}/chat/completions");
    let client = reqwest::Client::new();

    const MAX_TOOL_ITERATIONS: usize = 10;

    for _ in 0..MAX_TOOL_ITERATIONS {
        let mut body = json!({
            "model": cfg.model,
            "max_tokens": cfg.max_tokens,
            "stream": true,
            "messages": api_messages,
        });
        if let Some(ref td) = tool_defs {
            body["tools"] = td.clone();
        }

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", cfg.api_key.trim()))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("OpenAI: failed to connect")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            bail!("OpenAI API {status}: {text}");
        }

        let mut buf = String::new();
        let mut stream = response.bytes_stream();

        let mut pending_tools: HashMap<usize, PendingOpenAiTool> = HashMap::new();
        let mut finish_reason = String::new();
        let mut current_text = String::new();

        'stream: while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("OpenAI: stream read error")?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);

                let Some(data) = line.strip_prefix("data: ") else {
                    continue;
                };
                if data == "[DONE]" {
                    break 'stream;
                }
                let Ok(v) = serde_json::from_str::<Value>(data) else {
                    continue;
                };

                // Capture finish_reason.
                if let Some(reason) = v
                    .pointer("/choices/0/finish_reason")
                    .and_then(Value::as_str)
                {
                    if !reason.is_empty() {
                        finish_reason = reason.to_string();
                    }
                }

                // Accumulate tool_calls by index.
                if let Some(tcs) = v
                    .pointer("/choices/0/delta/tool_calls")
                    .and_then(Value::as_array)
                {
                    for tc in tcs {
                        let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                        let entry = pending_tools.entry(idx).or_insert_with(|| {
                            PendingOpenAiTool {
                                id: String::new(),
                                name: String::new(),
                                args_buf: String::new(),
                            }
                        });
                        if let Some(id) = tc["id"].as_str() {
                            entry.id = id.to_string();
                        }
                        if let Some(name) =
                            tc.pointer("/function/name").and_then(Value::as_str)
                        {
                            entry.name = name.to_string();
                        }
                        if let Some(args) =
                            tc.pointer("/function/arguments").and_then(Value::as_str)
                        {
                            entry.args_buf.push_str(args);
                        }
                    }
                }

                // Accumulate text content.
                if let Some(content) = v
                    .pointer("/choices/0/delta/content")
                    .and_then(Value::as_str)
                {
                    if !content.is_empty() {
                        current_text.push_str(content);
                        let _ = tx.send(StreamChunk::Token(content.to_string())).await;
                    }
                }
            }
        }

        if finish_reason == "tool_calls" && !pending_tools.is_empty() {
            // Build the assistant message with tool_calls.
            let mut tool_calls_json: Vec<Value> = pending_tools
                .values()
                .map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.args_buf,
                        }
                    })
                })
                .collect();
            tool_calls_json.sort_by_key(|v| v["id"].as_str().unwrap_or("").to_string());
            api_messages.push(json!({
                "role": "assistant",
                "content": current_text,
                "tool_calls": tool_calls_json,
            }));

            let mcp = mcp_client
                .as_ref()
                .context("MCP tool call requested but no MCP client is connected")?;

            let mut sorted_tools: Vec<(&usize, &PendingOpenAiTool)> =
                pending_tools.iter().collect();
            sorted_tools.sort_by_key(|(idx, _)| *idx);

            for (_, tc) in &sorted_tools {
                let _ = tx.send(StreamChunk::ToolCallStart(tc.name.clone())).await;
                let args: Value =
                    serde_json::from_str(&tc.args_buf).unwrap_or(json!({}));
                let result = mcp
                    .call_tool(&tc.name, args)
                    .await
                    .unwrap_or_else(|e| format!("Tool call error: {e}"));
                api_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tc.id,
                    "content": result,
                }));
            }

            continue;
        }

        let _ = tx.send(StreamChunk::Done).await;
        return Ok(());
    }

    bail!("OpenAI: exceeded maximum tool-use iterations ({MAX_TOOL_ITERATIONS})");
}

// ── Ollama ─────────────────────────────────────────────────────────────────────

async fn stream_ollama(
    cfg: OllamaConfig,
    system_prompt: String,
    messages: Vec<ChatMessage>,
    mcp_client: Option<Arc<McpClient>>,
    tools: &[McpTool],
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()> {
    let mut api_messages: Vec<Value> = vec![json!({"role": "system", "content": system_prompt})];
    for m in &messages {
        api_messages.push(json!({"role": m.role, "content": m.content}));
    }

    let tool_defs = if tools.is_empty() {
        None
    } else {
        Some(tools_to_openai(tools)) // Ollama uses the OpenAI tool format
    };

    let base_url = cfg.base_url.trim_end_matches('/');
    let url = format!("{base_url}/api/chat");
    let client = reqwest::Client::new();

    const MAX_TOOL_ITERATIONS: usize = 10;

    for _ in 0..MAX_TOOL_ITERATIONS {
        let mut body = json!({
            "model": cfg.model,
            "stream": true,
            "messages": api_messages,
        });
        if let Some(ref td) = tool_defs {
            body["tools"] = td.clone();
        }

        let response = client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Ollama: failed to connect — is Ollama running?")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            bail!("Ollama API {status}: {text}");
        }

        let mut buf = String::new();
        let mut stream = response.bytes_stream();
        let mut final_message: Option<Value> = None;

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("Ollama: stream read error")?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf.drain(..=nl);

                if line.is_empty() {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };

                if let Some(content) =
                    v.pointer("/message/content").and_then(Value::as_str)
                {
                    if !content.is_empty() {
                        let _ = tx.send(StreamChunk::Token(content.to_string())).await;
                    }
                }

                if v.get("done").and_then(Value::as_bool) == Some(true) {
                    final_message = Some(v);
                }
            }
        }

        // Check if the final message contains tool_calls (some Ollama models).
        let tool_calls = final_message
            .as_ref()
            .and_then(|v| v.pointer("/message/tool_calls"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if !tool_calls.is_empty() {
            if let Some(mcp) = mcp_client.as_ref() {
                // Record what the assistant said.
                api_messages.push(
                    final_message
                        .as_ref()
                        .and_then(|v| v.get("message").cloned())
                        .unwrap_or(json!({"role": "assistant", "content": ""})),
                );

                for tc in &tool_calls {
                    let name = tc
                        .pointer("/function/name")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let args = tc
                        .pointer("/function/arguments")
                        .cloned()
                        .unwrap_or(json!({}));

                    let _ = tx.send(StreamChunk::ToolCallStart(name.to_string())).await;
                    let result = mcp
                        .call_tool(name, args)
                        .await
                        .unwrap_or_else(|e| format!("Tool call error: {e}"));
                    api_messages.push(json!({"role": "tool", "content": result}));
                }

                continue;
            }
        }

        let _ = tx.send(StreamChunk::Done).await;
        return Ok(());
    }

    bail!("Ollama: exceeded maximum tool-use iterations ({MAX_TOOL_ITERATIONS})");
}
