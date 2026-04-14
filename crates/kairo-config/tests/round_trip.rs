use kairo_config::{ActiveProvider, AiConfig, AnthropicConfig, KairoConfig};

#[test]
fn round_trip_defaults() {
    let cfg = KairoConfig::default();
    let toml_str = toml::to_string_pretty(&cfg).expect("serialize");
    let parsed: KairoConfig = toml::from_str(&toml_str).expect("deserialize");
    assert_eq!(parsed.ai.active_provider, ActiveProvider::None);
    assert_eq!(parsed.ai.anthropic.model, "claude-sonnet-4-6");
    assert_eq!(parsed.ai.openai.model, "gpt-4o");
    assert_eq!(parsed.ai.ollama.model, "llama3.2");
}

#[test]
fn round_trip_with_values() {
    let cfg = KairoConfig {
        ai: AiConfig {
            active_provider: ActiveProvider::Anthropic,
            anthropic: AnthropicConfig {
                api_key: "sk-ant-test-key".into(),
                model: "claude-opus-4-6".into(),
                max_tokens: 2048,
            },
            ..Default::default()
        },
    };
    let toml_str = toml::to_string_pretty(&cfg).expect("serialize");
    let parsed: KairoConfig = toml::from_str(&toml_str).expect("deserialize");
    assert_eq!(parsed.ai.active_provider, ActiveProvider::Anthropic);
    assert_eq!(parsed.ai.anthropic.api_key, "sk-ant-test-key");
    assert_eq!(parsed.ai.anthropic.model, "claude-opus-4-6");
    assert_eq!(parsed.ai.anthropic.max_tokens, 2048);
    // Other providers keep defaults.
    assert!(parsed.ai.openai.api_key.is_empty());
}

#[test]
fn missing_fields_use_defaults() {
    let partial = r#"
[ai]
active_provider = "ollama"

[ai.ollama]
model = "mistral"
"#;
    let cfg: KairoConfig = toml::from_str(partial).expect("deserialize");
    assert_eq!(cfg.ai.active_provider, ActiveProvider::Ollama);
    assert_eq!(cfg.ai.ollama.model, "mistral");
    // base_url should fall back to default.
    assert_eq!(cfg.ai.ollama.base_url, "http://localhost:11434");
    // Anthropic key should be empty (default).
    assert!(cfg.ai.anthropic.api_key.is_empty());
}
