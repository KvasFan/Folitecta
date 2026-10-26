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
            "qat", "pencil",
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
            "contents",
            "ocr",
            "read",
            "reflow",
            "scans",
            "scan",
            "paths",
            "sequence",
            "office",
            "templates",
            "preflight",
            "grey",
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
    ("short", &["short"]),
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
            (
                "short",
                include_str!(concat!("../assets/i18n/", $lang, "/short.json")),
            ),
        ]
    };
}

const EN_FILES: [(&str, &str); 11] = tables!("en");
const IT_FILES: [(&str, &str); 11] = tables!("it");

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

/// Translate with `{name}` placeholders. When `count` is 1, the singular
/// form `<key>_one` is used where the language has one (English and Italian
/// use it for one only; zero takes the plural).
pub fn tf(lang: Lang, key: &str, args: &[(&str, &str)]) -> String {
    let singular = format!("{key}_one");
    let one = args.iter().any(|(k, v)| *k == "count" && *v == "1");
    let key = if one && table(lang).contains_key(&singular) {
        singular.as_str()
    } else {
        key
    };
    let mut s = t(lang, key);
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Why generated text cannot be drawn, in the language of the interface.
pub fn glyph_problem(lang: Lang, problem: &folitecta_core::GlyphProblem) -> String {
    use folitecta_core::GlyphProblem as P;
    use folitecta_core::fonts::describe_chars;
    match problem {
        P::Missing { font, chars } => tf(
            lang,
            "toast.glyphs_missing",
            &[("font", font), ("chars", &describe_chars(chars))],
        ),
        P::NeedsLayout(chars) => tf(lang, "toast.glyphs_layout", &[("chars", &describe_chars(chars))]),
        P::FontMissing { file, .. } => tf(lang, "toast.font_missing", &[("file", file)]),
        P::FontUnreadable { path, reason } => tf(
            lang,
            "toast.font_unreadable",
            &[("file", &path.display().to_string()), ("reason", reason)],
        ),
        P::NotEmbeddable { font } => tf(lang, "toast.font_not_embeddable", &[("font", font)]),
        P::StampText(chars) => tf(lang, "toast.glyphs_stamp", &[("chars", &describe_chars(chars))]),
    }
}

/// A comment of an imported file that cannot be drawn: page, author, the
/// start of its text and why.
pub fn undrawable_comment(lang: Lang, c: &folitecta_core::UndrawableComment) -> String {
    let excerpt = |s: &str| {
        let mut text: String = s.chars().take(40).collect();
        if s.chars().count() > 40 {
            text.push('…');
        }
        text
    };
    let text = excerpt(&c.contents);
    let author = if c.author.is_empty() {
        String::new()
    } else {
        format!(", {}", c.author)
    };
    let mut out = tf(
        lang,
        "toast.comment_undrawable",
        &[
            ("page", &(c.page + 1).to_string()),
            ("author", &author),
            ("text", &text),
            ("problem", glyph_problem(lang, &c.problem).trim_end_matches('.')),
        ],
    );
    if !c.replies.is_empty() {
        let replies: Vec<String> = c
            .replies
            .iter()
            .map(|r| {
                if r.author.is_empty() {
                    format!("“{}”", excerpt(&r.contents))
                } else {
                    format!("{}: “{}”", r.author, excerpt(&r.contents))
                }
            })
            .collect();
        out.push(' ');
        out.push_str(&tf(
            lang,
            "toast.comment_replies_left_out",
            &[("list", &replies.join(", "))],
        ));
    }
    out
}

/// The comments of an import that cannot be drawn, one after the other.
pub fn undrawable_list(lang: Lang, list: &[folitecta_core::UndrawableComment]) -> String {
    list.iter()
        .map(|c| undrawable_comment(lang, c))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The message of an error of the core: the problems of generated text and
/// the errors of protection in the language of the interface, the others as
/// the core words them.
pub fn core_error(lang: Lang, err: &folitecta_core::Error) -> String {
    use folitecta_core::Error as E;
    match err {
        E::WrongPassword => t(lang, "dialog.password.wrong"),
        E::PasswordRequired => t(lang, "toast.password_required"),
        E::ProtectedSource => t(lang, "toast.protected_source"),
        E::UnsupportedProtection(what) if what == folitecta_core::security::CERTIFICATE_PROTECTION => {
            t(lang, "toast.certificate_protection")
        }
        E::UnsupportedProtection(what) => tf(lang, "toast.unsupported_protection", &[("what", what)]),
        E::ProtectionNotKept(why) => tf(lang, "toast.protection_not_kept", &[("why", why)]),
        E::SignaturesNotKept => t(lang, "toast.signatures_not_kept"),
        folitecta_core::Error::Glyphs(p) => {
            format!("{} {}", glyph_problem(lang, p), t(lang, "toast.nothing_changed"))
        }
        folitecta_core::Error::Undrawable(list) => tf(
            lang,
            "toast.comments_undrawable",
            &[
                ("count", &list.len().to_string()),
                ("list", &undrawable_list(lang, list)),
            ],
        ),
        other => other.to_string(),
    }
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

/// The short name shown under the icon of a ribbon tool (`Zoom +`, `Pencil`);
/// made from the label when the tool has none of its own (see [`shorten`]).
pub fn short_label(lang: Lang, cmd: &Command) -> String {
    let key = format!("short.{}", cmd.id);
    match table(lang).get(&key).or_else(|| EN.get(&key)) {
        Some(s) => s.clone(),
        None => shorten(lang, &cmd_label(lang, cmd)),
    }
}

/// A label of at most about a dozen letters: without its ellipsis, a leading
/// verb such as "Add" and the small words; at most its first two words.
fn shorten(lang: Lang, label: &str) -> String {
    const LIMIT: usize = 12;
    let label = label.trim().trim_end_matches(['…', '.']).trim();
    if label.chars().count() <= LIMIT {
        return label.to_string();
    }
    let (verbs, small): (&[&str], &[&str]) = match lang {
        Lang::En => (
            &["Add", "Insert", "Show", "Create", "Set", "Make", "Open"],
            &[
                "Tool", "the", "a", "an", "to", "from", "of", "for", "in", "on", "with", "as", "and",
            ],
        ),
        Lang::It => (
            &["Aggiungi", "Inserisci", "Mostra", "Crea", "Imposta", "Apri"],
            &[
                "Strumento",
                "il",
                "lo",
                "la",
                "i",
                "gli",
                "le",
                "un",
                "una",
                "di",
                "da",
                "a",
                "in",
                "su",
                "per",
                "con",
                "del",
                "della",
                "dei",
                "delle",
                "al",
                "alla",
                "nel",
                "nella",
                "e",
            ],
        ),
    };
    let mut words: Vec<&str> = label.split_whitespace().collect();
    if words.len() > 1 && verbs.contains(&words[0]) {
        words.remove(0);
    }
    let kept: Vec<&str> = words.iter().copied().filter(|w| !small.contains(w)).collect();
    if !kept.is_empty() {
        words = kept;
    }
    let two = words.iter().take(2).copied().collect::<Vec<_>>().join(" ");
    let short = if two.chars().count() <= LIMIT + 2 || words.len() < 2 {
        two
    } else {
        words[0].to_string()
    };
    // A capital letter first, as the name it comes from.
    let mut chars = short.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => short,
    }
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
    fn problems_of_generated_text_are_translated() {
        let missing = folitecta_core::Error::Glyphs(folitecta_core::GlyphProblem::Missing {
            font: "FreeSans".into(),
            chars: vec!['漢'],
        });
        let en = core_error(Lang::En, &missing);
        let it = core_error(Lang::It, &missing);
        assert!(
            en.contains("漢 (U+6F22)") && en.ends_with("The document was not changed."),
            "{en}"
        );
        assert!(
            it.contains("漢 (U+6F22)") && it.contains("non ha i glifi"),
            "{it}"
        );
        let list = vec![folitecta_core::UndrawableComment {
            page: 1,
            author: "Reviewer".into(),
            name: "c1".into(),
            contents: "שלום".into(),
            problem: folitecta_core::GlyphProblem::NeedsLayout(vec!['ש']),
            replies: vec![folitecta_core::LeftOutReply {
                author: "Second".into(),
                contents: "ok".into(),
            }],
        }];
        let it = core_error(Lang::It, &folitecta_core::Error::Undrawable(list));
        assert!(
            it.contains("(1)") && it.contains("pagina 2, Reviewer: “שלום”"),
            "{it}"
        );
        assert!(it.contains("ancora disegnare nel testo generato"), "{it}");
        assert!(
            it.contains("(risposte lasciate fuori con esso: Second: “ok”)"),
            "{it}"
        );
        let stamp = folitecta_core::Error::Glyphs(folitecta_core::GlyphProblem::StampText(vec!['Ł']));
        let it = core_error(Lang::It, &stamp);
        assert!(
            it.starts_with("Ł (U+0141): i timbri di testo") && !it.contains("Helvetica"),
            "{it}"
        );
        assert_eq!(
            core_error(Lang::It, &folitecta_core::Error::NoPath),
            folitecta_core::Error::NoPath.to_string()
        );
    }

    #[test]
    fn counted_messages_take_their_singular_for_one() {
        let n = |lang, count: &str| tf(lang, "toast.pages_deleted", &[("count", count)]);
        assert_eq!(n(Lang::En, "1"), "1 page deleted");
        assert_eq!(n(Lang::En, "0"), "0 pages deleted");
        assert_eq!(n(Lang::En, "2"), "2 pages deleted");
        assert_eq!(n(Lang::It, "1"), "1 pagina eliminata");
        assert_eq!(n(Lang::It, "3"), "3 pagine eliminate");
        // Messages without a singular form keep theirs.
        assert_eq!(
            tf(Lang::En, "dialog.print.side_label", &[("n", "1"), ("count", "1")]),
            "Side 1 of 1"
        );
    }

    #[test]
    fn singular_forms_stand_next_to_their_message() {
        let placeholders = |s: &str| -> std::collections::BTreeSet<String> {
            s.split('{')
                .skip(1)
                .filter_map(|p| p.split_once('}').map(|(name, _)| name.to_string()))
                .collect()
        };
        for (lang, files) in [("en", &EN_FILES), ("it", &IT_FILES)] {
            for (area, src) in files.iter() {
                let table: HashMap<String, String> = serde_json::from_str(src).unwrap();
                for (key, one) in &table {
                    let Some(base) = key.strip_suffix("_one") else {
                        continue;
                    };
                    let plural = table
                        .get(base)
                        .unwrap_or_else(|| panic!("{lang}/{area}: {key} without {base}"));
                    assert!(plural.contains("{count}"), "{lang}/{area}: {base} counts nothing");
                    assert!(
                        placeholders(one).is_subset(&placeholders(plural)),
                        "{lang}/{area}: {key} has other placeholders than {base}"
                    );
                }
            }
        }
        for key in EN.keys().filter(|k| k.ends_with("_one")) {
            assert!(IT.contains_key(key), "{key} has no Italian singular");
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

    #[test]
    fn short_labels_are_short_and_name_known_commands() {
        let registry = folitecta_core::registry();
        for table in [&*EN, &*IT] {
            for (key, v) in table.iter().filter(|(k, _)| k.starts_with("short.")) {
                let id = &key["short.".len()..];
                assert!(
                    id == "more" || registry.get(id).is_some(),
                    "{key} names no command"
                );
                assert!(v.chars().count() <= 13, "{key} = {v:?} is too long");
            }
        }
        let zoom_in = registry.get("zoomIn").unwrap();
        assert_eq!(short_label(Lang::En, zoom_in), "Zoom +");
        assert_eq!(
            short_label(Lang::It, registry.get("tool.loupe").unwrap()),
            "Lente"
        );
        // Without a short name: the label without its ellipsis, made shorter.
        assert_eq!(short_label(Lang::En, registry.get("open").unwrap()), "Open");
        assert_eq!(shorten(Lang::En, "Add Sticky Note"), "Sticky Note");
        assert_eq!(shorten(Lang::En, "Create Web Links from URLs…"), "Web Links");
        assert_eq!(shorten(Lang::En, "Export Measures to CSV"), "Export");
        assert_eq!(shorten(Lang::En, "Rotate Counterclockwise"), "Rotate");
        assert_eq!(shorten(Lang::It, "Aggiungi collegamento"), "Collegamento");
        assert_eq!(shorten(Lang::It, "Ruota in senso antiorario"), "Ruota senso");
    }
}
