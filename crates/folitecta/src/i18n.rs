//! Localization: embedded JSON string tables with English fallback.
//!
//! Each language has one file per area of the interface in
//! `assets/i18n/<lang>/<area>.json`; a key lives in the file of the area of
//! its first word (see [`AREAS`]). English command labels come from the
//! command registry, so `en/commands.json` stays empty.

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

/// The areas of the translation files and the first words of their keys
/// (checked by the tests: every key is in the file of its area).
#[cfg(test)]
const AREAS: &[(&str, &[&str])] = &[
    ("commands", &["cmd", "macrodesc"]),
    (
        "interface",
        &[
            "app", "tab", "group", "menu", "pane", "label", "status", "palette", "welcome", "popup", "sel",
            "tool", "layout", "filter", "pos", "props", "history", "session", "rename", "text", "recovery",
            "qat",
        ],
    ),
    ("dialogs", &["dialog"]),
    ("messages", &["toast"]),
    (
        "comments",
        &[
            "annot",
            "style",
            "styles",
            "le",
            "review",
            "comments",
            "stamps",
            "stampel",
            "stampfield",
            "redact",
            "calc",
            "table",
            "measure",
        ],
    ),
    (
        "pages",
        &[
            "marks",
            "macro",
            "split",
            "sources",
            "picture",
            "inserttext",
            "recompress",
            "meta",
            "docinfo",
            "relationship",
            "explorer",
            "portfolio",
            "attach",
            "layers",
            "bookmark",
            "links",
            "compare",
            "audit",
            "media",
            "multiplace",
            "barcode",
            "ocr",
        ],
    ),
    (
        "forms",
        &[
            "field",
            "fields",
            "fieldkind",
            "fieldcopies",
            "actions",
            "js",
            "launch",
            "tab_order",
        ],
    ),
    ("protect", &["sig", "signs", "policy"]),
    ("accessibility", &["acc", "tags", "alt", "order", "content"]),
    ("search", &["search", "findrepl", "spell"]),
];

/// The area of a key: the file it belongs in.
#[cfg(test)]
fn area_of(key: &str) -> Option<&'static str> {
    let first = key.split('.').next().unwrap_or_default();
    AREAS
        .iter()
        .find(|(_, words)| words.contains(&first))
        .map(|(area, _)| *area)
}

macro_rules! tables {
    ($lang:literal) => {
        [
            (
                "commands",
                include_str!(concat!("../assets/i18n/", $lang, "/commands.json")),
            ),
            (
                "interface",
                include_str!(concat!("../assets/i18n/", $lang, "/interface.json")),
            ),
            (
                "dialogs",
                include_str!(concat!("../assets/i18n/", $lang, "/dialogs.json")),
            ),
            (
                "messages",
                include_str!(concat!("../assets/i18n/", $lang, "/messages.json")),
            ),
            (
                "comments",
                include_str!(concat!("../assets/i18n/", $lang, "/comments.json")),
            ),
            (
                "pages",
                include_str!(concat!("../assets/i18n/", $lang, "/pages.json")),
            ),
            (
                "forms",
                include_str!(concat!("../assets/i18n/", $lang, "/forms.json")),
            ),
            (
                "protect",
                include_str!(concat!("../assets/i18n/", $lang, "/protect.json")),
            ),
            (
                "accessibility",
                include_str!(concat!("../assets/i18n/", $lang, "/accessibility.json")),
            ),
            (
                "search",
                include_str!(concat!("../assets/i18n/", $lang, "/search.json")),
            ),
        ]
    };
}

const EN_FILES: [(&str, &str); 10] = tables!("en");
const IT_FILES: [(&str, &str); 10] = tables!("it");

static EN: LazyLock<HashMap<String, String>> = LazyLock::new(|| load(&EN_FILES));
static IT: LazyLock<HashMap<String, String>> = LazyLock::new(|| load(&IT_FILES));

fn load(files: &[(&str, &str)]) -> HashMap<String, String> {
    let mut all = HashMap::new();
    for (area, src) in files {
        let table: HashMap<String, String> =
            serde_json::from_str(src).unwrap_or_else(|e| panic!("invalid i18n file {area}: {e}"));
        all.extend(table);
    }
    all
}

fn table(lang: Lang) -> &'static HashMap<String, String> {
    match lang {
        Lang::En => &EN,
        Lang::It => &IT,
    }
}

/// Translate a UI string key. `cmd.<id>` keys fall back to the registry label.
pub fn t(lang: Lang, key: &str) -> String {
    if let Some(s) = table(lang).get(key).or_else(|| EN.get(key)) {
        return s.clone();
    }
    if let Some(cmd) = key
        .strip_prefix("cmd.")
        .and_then(|id| folitecta_core::registry().get(id))
    {
        return cmd.label.clone();
    }
    key.to_string()
}

/// Friendly, localized name of an annotation subtype (`Text` is a sticky note, ...).
pub fn annot_name(lang: Lang, subtype: &str) -> String {
    let key = format!("annot.{subtype}");
    match table(lang).get(&key).or_else(|| EN.get(&key)) {
        Some(s) => s.clone(),
        None => subtype.to_string(),
    }
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
        assert_eq!(t(Lang::En, "cmd.saveAs"), "Save As…");
        assert_eq!(tf(Lang::En, "toast.opened", &[("name", "a.pdf")]), "Opened a.pdf");
    }

    #[test]
    fn every_key_is_in_the_file_of_its_area() {
        for files in [&EN_FILES, &IT_FILES] {
            for (area, src) in files.iter() {
                let table: HashMap<String, String> = serde_json::from_str(src).unwrap();
                for key in table.keys() {
                    assert_eq!(area_of(key), Some(*area), "{key} is in {area}.json");
                }
            }
        }
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
