use serde::{Deserialize, Serialize};

/// Which AI provider is currently active.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ActiveProvider {
    Anthropic,
    OpenAi,
    Ollama,
    #[default]
    #[serde(other)]
    None,
}

/// Top-level AI configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub active_provider: ActiveProvider,
    pub anthropic: AnthropicConfig,
    pub openai: OpenAiConfig,
    pub ollama: OllamaConfig,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            active_provider: ActiveProvider::None,
            anthropic: AnthropicConfig::default(),
            openai: OpenAiConfig::default(),
            ollama: OllamaConfig::default(),
        }
    }
}

/// Anthropic (Claude) provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "claude-sonnet-4-6".into(),
            max_tokens: 1024,
        }
    }
}

/// OpenAI (or compatible endpoint) provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
    pub api_key: String,
    /// Base URL — override for Groq, Together, Azure, etc.
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            max_tokens: 1024,
        }
    }
}

/// Local Ollama provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
            model: "llama3.2".into(),
            max_tokens: 2048,
        }
    }
}
