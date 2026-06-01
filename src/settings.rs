use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preferences {
    pub level: usize,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { level: 1 }
    }
}

impl Preferences {
    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(prefs) = serde_json::from_str(&data) {
                return prefs;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, data);
        }
    }

    fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("bombicat");
        path.push("config.json");
        path
    }

    pub fn level_params(level: usize) -> (usize, usize, usize) {
        match level {
            0 => (15, 10, 15),
            1 => (20, 15, 25),
            2 => (25, 15, 35),
            3 => (27, 20, 110),
            4 => (35, 22, 160),
            5 => (40, 25, 215),
            _ => (15, 15, 25),
        }
    }

    pub fn level_name(level: usize) -> &'static str {
        match level {
            0 => "Débutant",
            1 => "Normal",
            2 => "Difficile",
            3 => "Ultra Difficile",
            4 => "Géant",
            5 => "Chuck Norris",
            _ => "?",
        }
    }

    pub fn effective_dims(&self) -> (usize, usize, usize) {
        Self::level_params(self.level)
    }
}
