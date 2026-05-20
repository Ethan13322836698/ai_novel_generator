use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub font_size: f32,
    pub auto_save: bool,
    pub setup_done: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: "DeepSeek".to_string(),
            api_key: String::new(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            model: "deepseek-chat".to_string(),
            temperature: 0.8,
            max_tokens: 4096,
            font_size: 15.0,
            auto_save: true,
            setup_done: false,
        }
    }
}

impl AppConfig {
    fn config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "novelai", "NovelAI").map(|dirs| {
            let dir = dirs.config_dir().to_path_buf();
            std::fs::create_dir_all(&dir).ok();
            dir.join("config.json")
        })
    }

    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str(&data) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Ok(json) = serde_json::to_string_pretty(self) {
                std::fs::write(path, json).ok();
            }
        }
    }
}

pub struct ProviderPreset {
    pub name: &'static str,
    pub base_url: &'static str,
    pub models: &'static [&'static str],
}

pub fn provider_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            name: "DeepSeek",
            base_url: "https://api.deepseek.com/v1",
            models: &["deepseek-chat", "deepseek-reasoner"],
        },
        ProviderPreset {
            name: "OpenAI",
            base_url: "https://api.openai.com/v1",
            models: &["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-3.5-turbo"],
        },
        ProviderPreset {
            name: "Gemini",
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
            models: &["gemini-flash-lite-latest", "gemini-flash-latest", "gemini-pro-latest"],
        },
        ProviderPreset {
            name: "Ollama",
            base_url: "http://localhost:11434/v1",
            models: &["llama3.2", "qwen2.5", "deepseek-r1", "mistral", "gemma3"],
        },
        ProviderPreset {
            name: "自定义",
            base_url: "",
            models: &[],
        },
    ]
}
