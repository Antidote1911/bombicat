use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::model::Logic;

/// Nombre de niveaux de difficulté.
pub const LEVEL_COUNT: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// Langue du système : français si la locale commence par « fr », sinon anglais.
    pub fn from_system() -> Self {
        Self::from_locale(sys_locale::get_locale().as_deref())
    }

    fn from_locale(locale: Option<&str>) -> Self {
        match locale {
            Some(l) if l.to_ascii_lowercase().starts_with("fr") => Lang::Fr,
            _ => Lang::En,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    pub level: usize,
    /// `None` : suivre la langue du système.
    #[serde(default)]
    pub lang: Option<Lang>,
    /// Le clic droit passe par « ? » entre le drapeau et rien.
    #[serde(default = "default_true")]
    pub question_marks: bool,
    /// Grilles pouvant exiger l'étude de tous les cas sur une zone.
    #[serde(default)]
    pub advanced_logic: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { level: 1, lang: None, question_marks: true, advanced_logic: false }
    }
}

impl Preferences {
    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(mut prefs) = serde_json::from_str::<Preferences>(&data) {
                prefs.level = prefs.level.min(LEVEL_COUNT - 1);
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

    /// (largeur, hauteur, chats). La densité de chats croît à chaque niveau :
    /// 10 %, 13 %, 17 %, 22 %, 25 %, 25 %.
    pub fn level_params(level: usize) -> (usize, usize, usize) {
        match level {
            0 => (15, 10, 15),
            1 => (20, 15, 40),
            2 => (25, 15, 62),
            3 => (27, 20, 120),
            4 => (35, 22, 190),
            5 => (40, 25, 250),
            _ => (15, 15, 25),
        }
    }

    pub fn level_name(level: usize, lang: Lang) -> &'static str {
        match (level, lang) {
            (0, Lang::Fr) => "Débutant",
            (1, Lang::Fr) => "Normal",
            (2, Lang::Fr) => "Difficile",
            (3, Lang::Fr) => "Ultra Difficile",
            (4, Lang::Fr) => "Géant",
            (0, Lang::En) => "Beginner",
            (1, Lang::En) => "Normal",
            (2, Lang::En) => "Hard",
            (3, Lang::En) => "Very Hard",
            (4, Lang::En) => "Giant",
            (5, _) => "Chuck Norris",
            _ => "?",
        }
    }

    pub fn effective_lang(&self) -> Lang {
        self.lang.unwrap_or_else(Lang::from_system)
    }

    /// Niveau de raisonnement exigé par les grilles générées.
    pub fn logic(&self) -> Logic {
        if self.advanced_logic { Logic::Advanced } else { Logic::Basic }
    }

    pub fn effective_dims(&self) -> (usize, usize, usize) {
        Self::level_params(self.level)
    }

    /// Signature des règles d'un niveau, enregistrée avec chaque score :
    /// seuls les scores obtenus avec les mêmes règles sont comparables.
    /// Exemple : `20x15/40`, ou `20x15/40+` avec les déductions avancées.
    pub fn rules(&self, level: usize) -> String {
        let (w, h, cats) = Self::level_params(level);
        format!("{w}x{h}/{cats}{}", if self.advanced_logic { "+" } else { "" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_file_still_loads() {
        let prefs: Preferences = serde_json::from_str(r#"{ "level": 3 }"#).unwrap();
        assert_eq!(prefs, Preferences { level: 3, ..Preferences::default() });
        assert!(prefs.question_marks);
    }

    #[test]
    fn preferences_round_trip() {
        let prefs = Preferences {
            level: 5, lang: Some(Lang::En), question_marks: false, advanced_logic: true,
        };
        let json = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<Preferences>(&json).unwrap(), prefs);
        assert_eq!(prefs.rules(5), "40x25/250+");
    }

    #[test]
    fn density_grows_with_level() {
        let density = |level| {
            let (w, h, cats) = Preferences::level_params(level);
            cats as f64 / (w * h) as f64
        };
        for level in 1..LEVEL_COUNT {
            assert!(density(level) > density(level - 1), "niveau {level}");
        }
    }

    #[test]
    fn language_follows_locale() {
        assert_eq!(Lang::from_locale(Some("fr-FR")), Lang::Fr);
        assert_eq!(Lang::from_locale(Some("fr_CA.UTF-8")), Lang::Fr);
        assert_eq!(Lang::from_locale(Some("en-US")), Lang::En);
        assert_eq!(Lang::from_locale(None), Lang::En);
    }
}
