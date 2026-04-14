//! Streaming AI client — supports Anthropic, OpenAI-compatible, and Ollama.
//!
//! All three providers are called from the dedicated tokio runtime thread
//! (see `kube_runtime`). The caller receives incremental tokens via an mpsc
//! channel and is responsible for pushing them into the GPUI event queue.

use anyhow::{bail, Context as _, Result};
use futures_util::StreamExt;
use kairo_config::{ActiveProvider, AnthropicConfig, KairoConfig, OllamaConfig, OpenAiConfig};
use serde_json::{json, Value};
use tokio::sync::mpsc;

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
    Done,
}

// ── Entry point ────────────────────────────────────────────────────────────────

/// Stream a completion from the active provider.
///
/// `system_prompt` is injected as a system-level instruction (not in
/// `messages`).  `messages` should contain the full conversation history
/// (user + assistant turns), with the latest user message last.
pub async fn stream_completion(
    config: KairoConfig,
    system_prompt: String,
    messages: Vec<ChatMessage>,
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()> {
    match config.ai.active_provider {
        ActiveProvider::Anthropic => {
            stream_anthropic(config.ai.anthropic, system_prompt, messages, tx).await
        }
        ActiveProvider::OpenAi => {
            stream_openai(config.ai.openai, system_prompt, messages, tx).await
        }
        ActiveProvider::Ollama => {
            stream_ollama(config.ai.ollama, system_prompt, messages, tx).await
        }
        ActiveProvider::None => {
            bail!(
                "No AI provider configured. Click ⚙ in the title bar \
                 (or press Ctrl+,) to open Settings and set an active provider."
            );
        }
    }
}

// ── Anthropic ──────────────────────────────────────────────────────────────────

async fn stream_anthropic(
    cfg: AnthropicConfig,
    system_prompt: String,
    messages: Vec<ChatMessage>,
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()> {
    if cfg.api_key.trim().is_empty() {
        bail!("Anthropic API key is not set. Open Settings (⚙) to add it.");
    }

    let api_messages: Vec<Value> = messages
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();

    let body = json!({
        "model": cfg.model,
        "max_tokens": cfg.max_tokens,
        "stream": true,
        "system": system_prompt,
        "messages": api_messages,
    });

    let client = reqwest::Client::new();
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

    // Parse SSE stream: lines starting with "data: " carry JSON payloads.
    let mut buf = String::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("Anthropic: stream read error")?;
        buf.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim().to_string();
            buf = buf[nl + 1..].to_string();

            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    let event_type = v.get("type").and_then(Value::as_str).unwrap_or("");
                    match event_type {
                        "content_block_delta" => {
                            if let Some(text) = v.pointer("/delta/text").and_then(Value::as_str) {
                                if !text.is_empty() {
                                    let _ = tx.send(StreamChunk::Token(text.to_string())).await;
                                }
                            }
                        }
                        "message_stop" => {
                            let _ = tx.send(StreamChunk::Done).await;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let _ = tx.send(StreamChunk::Done).await;
    Ok(())
}

// ── OpenAI (and compatible endpoints) ─────────────────────────────────────────

async fn stream_openai(
    cfg: OpenAiConfig,
    system_prompt: String,
    messages: Vec<ChatMessage>,
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()> {
    if cfg.api_key.trim().is_empty() {
        bail!("OpenAI API key is not set. Open Settings (⚙) to add it.");
    }

    let mut api_messages: Vec<Value> = vec![json!({"role": "system", "content": system_prompt})];
    for m in &messages {
        api_messages.push(json!({"role": m.role, "content": m.content}));
    }

    let body = json!({
        "model": cfg.model,
        "max_tokens": cfg.max_tokens,
        "stream": true,
        "messages": api_messages,
    });

    let base_url = cfg.base_url.trim_end_matches('/');
    let url = format!("{base_url}/chat/completions");

    let client = reqwest::Client::new();
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

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("OpenAI: stream read error")?;
        buf.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim().to_string();
            buf = buf[nl + 1..].to_string();

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    let _ = tx.send(StreamChunk::Done).await;
                    return Ok(());
                }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if let Some(content) = v
                        .pointer("/choices/0/delta/content")
                        .and_then(Value::as_str)
                    {
                        if !content.is_empty() {
                            let _ = tx.send(StreamChunk::Token(content.to_string())).await;
                        }
                    }
                }
            }
        }
    }

    let _ = tx.send(StreamChunk::Done).await;
    Ok(())
}

// ── Ollama ─────────────────────────────────────────────────────────────────────

async fn stream_ollama(
    cfg: OllamaConfig,
    system_prompt: String,
    messages: Vec<ChatMessage>,
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()> {
    // Ollama accepts a system message as the first entry in the messages array.
    let mut api_messages: Vec<Value> = vec![json!({"role": "system", "content": system_prompt})];
    for m in &messages {
        api_messages.push(json!({"role": m.role, "content": m.content}));
    }

    let base_url = cfg.base_url.trim_end_matches('/');
    let url = format!("{base_url}/api/chat");

    let body = json!({
        "model": cfg.model,
        "stream": true,
        "messages": api_messages,
    });

    let client = reqwest::Client::new();
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

    // Ollama streams newline-delimited JSON objects.
    let mut buf = String::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("Ollama: stream read error")?;
        buf.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim().to_string();
            buf = buf[nl + 1..].to_string();

            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if let Some(content) = v
                    .pointer("/message/content")
                    .and_then(Value::as_str)
                {
                    if !content.is_empty() {
                        let _ = tx.send(StreamChunk::Token(content.to_string())).await;
                    }
                }
                if v.get("done").and_then(Value::as_bool) == Some(true) {
                    let _ = tx.send(StreamChunk::Done).await;
                    return Ok(());
                }
            }
        }
    }

    let _ = tx.send(StreamChunk::Done).await;
    Ok(())
}
