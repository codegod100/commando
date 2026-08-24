use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Ollama,
    OpenAi,
    Anthropic,
    OpenRouter,
    Custom,
}

impl Provider {
    pub const ALL: [Provider; 5] = [
        Provider::Ollama,
        Provider::OpenAi,
        Provider::Anthropic,
        Provider::OpenRouter,
        Provider::Custom,
    ];

    pub fn as_label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::OpenRouter => "OpenRouter",
            Self::Custom => "Custom",
        }
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL
            .get(index as usize)
            .copied()
            .unwrap_or(Self::Ollama)
    }

    pub fn index(self) -> u32 {
        Self::ALL
            .iter()
            .position(|item| *item == self)
            .unwrap_or(0) as u32
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::Ollama => "http://127.0.0.1:11434/v1",
            Self::OpenAi => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::Custom => "http://127.0.0.1:1234/v1",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Ollama => "llama3.2",
            Self::OpenAi => "gpt-4.1",
            Self::Anthropic => "claude-sonnet-4-0",
            Self::OpenRouter => "anthropic/claude-sonnet-4",
            Self::Custom => "local-model",
        }
    }

    pub fn suggested_models(self) -> &'static [&'static str] {
        match self {
            Self::Ollama => &["llama3.2", "qwen2.5-coder", "mistral", "gemma3", "deepseek-r1"],
            Self::OpenAi => &["gpt-4.1", "gpt-4o", "o4-mini", "gpt-4.1-mini"],
            Self::Anthropic => &[
                "claude-sonnet-4-0",
                "claude-opus-4-0",
                "claude-haiku-4-5",
            ],
            Self::OpenRouter => &[
                "anthropic/claude-sonnet-4",
                "openai/gpt-4.1",
                "google/gemini-2.5-pro",
            ],
            Self::Custom => &["local-model"],
        }
    }

    pub fn needs_api_key(self) -> bool {
        !matches!(self, Self::Ollama)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub provider: Provider,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub workspace: PathBuf,
    pub timeout_secs: u64,
    pub max_iterations: u32,
    pub force_dark: bool,
}

impl Default for Config {
    fn default() -> Self {
        let workspace = default_workspace();
        Self {
            provider: Provider::Ollama,
            base_url: Provider::Ollama.default_base_url().to_string(),
            api_key: String::new(),
            model: Provider::Ollama.default_model().to_string(),
            workspace,
            timeout_secs: 90,
            max_iterations: 24,
            force_dark: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        let Ok(raw) = fs::read_to_string(&path) else {
            let config = Self::default();
            let _ = config.save();
            return config;
        };
        toml::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, toml::to_string_pretty(self).expect("config serializes"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn apply_provider_defaults(&mut self) {
        self.base_url = self.provider.default_base_url().to_string();
        self.model = self.provider.default_model().to_string();
    }

    pub fn ready_hint(&self) -> Option<String> {
        if self.provider.needs_api_key() && self.api_key.trim().is_empty() {
            Some(format!(
                "Add an API key in Settings to use {}.",
                self.provider.as_label()
            ))
        } else {
            None
        }
    }
}

pub fn config_path() -> PathBuf {
    directories::ProjectDirs::from("app", "commando", "Commando")
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("commando.toml"))
}

pub fn default_workspace() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| {
            dirs.desktop_dir()
                .map(Path::to_path_buf)
                .or_else(|| Some(dirs.home_dir().to_path_buf()))
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn display_path(path: &Path) -> String {
    if let Some(home) = directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
        if let Ok(stripped) = path.strip_prefix(&home) {
            if stripped.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}

pub fn expand_user_path(input: &str) -> PathBuf {
    let trimmed = input.trim();
    if trimmed == "~" {
        return directories::UserDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"))
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_round_trip() {
        for provider in Provider::ALL {
            assert_eq!(Provider::from_index(provider.index()), provider);
        }
    }

    #[test]
    fn expand_home_prefix() {
        let expanded = expand_user_path("~/Documents");
        assert!(expanded.ends_with("Documents"));
        assert!(!expanded.starts_with("~"));
    }
}
