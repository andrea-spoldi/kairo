pub mod ai;

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

pub use ai::{ActiveProvider, AiConfig, AnthropicConfig, OllamaConfig, OpenAiConfig};

/// Errors that can occur when loading or saving configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to determine home directory")]
    NoHomeDir,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("TOML serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Top-level Kairo configuration, persisted to `~/.kairo/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct KairoConfig {
    pub ai: AiConfig,
    // general: GeneralConfig  ← reserved for Phase 17+
}

impl KairoConfig {
    /// Return the path to `~/.kairo/config.toml`.
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        let home = dirs::home_dir().ok_or(ConfigError::NoHomeDir)?;
        Ok(home.join(".kairo").join("config.toml"))
    }

    /// Load from disk. Returns `Default` if the file does not exist yet.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path()?;
        if !path.exists() {
            info!("no config file at {path:?} — using defaults");
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)?;
        let cfg: Self = toml::from_str(&raw)?;
        info!("loaded config from {path:?}");
        Ok(cfg)
    }

    /// Persist to `~/.kairo/config.toml`, creating the directory if needed.
    ///
    /// File permissions are restricted to `0600` on Unix platforms so that
    /// API keys stored in the file are not readable by other users.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::config_path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }

        let toml_str = toml::to_string_pretty(self)?;
        fs::write(&path, &toml_str)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = fs::set_permissions(&path, fs::Permissions::from_mode(0o600)) {
                warn!("could not set config file permissions: {e}");
            }
        }

        info!("saved config to {path:?}");
        Ok(())
    }
}
