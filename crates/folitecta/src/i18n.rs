//! Minimal localization: embedded JSON string tables with English fallback.

use std::collections::HashMap;
use std::sync::LazyLock;

use folitecta_core::Command;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Lang {
    #[default]
    En,
    It,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::It];

    pub fn code(&self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::It => "it",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::It => "Italiano",
        }
    }

    pub fn from_code(code: &str) -> Lang {
        match code {
            "it" => Lang::It,
            _ => Lang::En,
        }
    }
}

static EN: LazyLock<HashMap<String, String>> = LazyLock::new(|| load(include_str!("../assets/i18n/en.json")));
static IT: LazyLock<HashMap<String, String>> = LazyLock::new(|| load(include_str!("../assets/i18n/it.json")));

fn load(src: &str) -> HashMap<String, String> {
    serde_json::from_str(src).expect("valid i18n json")
}

fn table(lang: Lang) -> &'static HashMap<String, String> {
    match lang {
        Lang::En => &EN,
        Lang::It => &IT,
    }
}

/// Translate a UI string key.
pub fn t(lang: Lang, key: &str) -> String {
    table(lang)
        .get(key)
        .or_else(|| EN.get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// Translate with `{name}` placeholders.
pub fn tf(lang: Lang, key: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(lang, key);
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Localized label of a registry command (falls back to the registry label).
pub fn cmd_label(lang: Lang, cmd: &Command) -> String {
    if lang != Lang::En {
        if let Some(s) = table(lang).get(&format!("cmd.{}", cmd.id)) {
            return s.clone();
        }
    }
    cmd.label.clone()
}

/// Localized label of a classic menu, ribbon tab or pane by id.
pub fn ui_label(lang: Lang, prefix: &str, id: &str, fallback: &str) -> String {
    let key = format!("{prefix}.{id}");
    match table(lang)
        .get(&key)
        .or_else(|| if lang != Lang::En { EN.get(&key) } else { None })
    {
        Some(s) => s.clone(),
        None => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_load_and_fallback_works() {
        assert_eq!(t(Lang::En, "dialog.ok"), "OK");
        assert_eq!(t(Lang::It, "menu.edit"), "Modifica");
        assert_eq!(t(Lang::It, "no.such.key"), "no.such.key");
        assert_eq!(tf(Lang::En, "toast.opened", &[("name", "a.pdf")]), "Opened a.pdf");
    }

    #[test]
    fn italian_covers_every_english_ui_key() {
        for key in EN.keys() {
            assert!(IT.contains_key(key), "missing Italian translation for {key}");
        }
    }

    #[test]
    fn italian_command_labels_reference_known_commands() {
        let registry = folitecta_core::registry();
        for key in IT.keys().filter(|k| k.starts_with("cmd.")) {
            assert!(
                registry.get(&key[4..]).is_some(),
                "it.json labels unknown command {key}"
            );
        }
    }
}
