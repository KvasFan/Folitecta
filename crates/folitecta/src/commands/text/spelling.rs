//! Spell checking: the checker with the dictionaries of the languages
//! chosen, Spell Check and Replace (the words the dictionaries do not know,
//! one by one, in the text of the pages, the comments and the form fields),
//! and the words marked on the pages while their text is edited.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use dioxus::prelude::*;
use folitecta_core::marks::pdf_rect_to_display;
use folitecta_core::spelling::find_dictionaries;
use folitecta_core::{DictionaryFile, Misspelling, SpellOptions, SpellPlace, SpellScope, Speller};

use crate::commands::{handlers, no_doc};
use crate::files;
use crate::i18n::{Lang, t, tf};
use crate::state::{AppState, Dialog, DocTab, SpellRun, ToastKind};

// ---- The checker ------------------------------------------------------------------------

/// The checker, once the dictionaries of the languages chosen are read.
#[derive(Clone, Default)]
pub struct SpellStatus {
    pub speller: Option<Arc<Speller>>,
    /// The dictionaries are being read.
    pub loading: bool,
    /// The languages of the checker (or of the one being made).
    pub languages: Vec<String>,
    /// Languages without a dictionary that could be read: code, and why (None: not found).
    pub missing: Vec<(String, Option<String>)>,
}

pub static SPELL: GlobalSignal<SpellStatus> = Signal::global(SpellStatus::default);

/// Suggestions for the word shown: the word, and what to write instead
/// (None while they are being found).
pub static SUGGESTIONS: GlobalSignal<Option<(String, Option<Vec<String>>)>> = Signal::global(|| None);

/// The text of "Change to" in Spell Check and Replace.
pub static CHANGE_TO: GlobalSignal<String> = Signal::global(String::new);

/// The folders searched for dictionaries, the preferred first: the user's
/// own, the program's, the source tree (when run from there) and, on Linux,
/// the system's.
pub fn dictionary_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = user_dir() {
        dirs.push(d);
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    {
        dirs.push(exe_dir.join("dictionaries"));
        // The resources of a macOS application bundle, a Linux installation.
        dirs.push(exe_dir.join("../Resources/dictionaries"));
        dirs.push(exe_dir.join("../share/folitecta/dictionaries"));
    }
    dirs.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../dictionaries"
    )));
    #[cfg(target_os = "linux")]
    for d in [
        "/usr/share/hunspell",
        "/usr/share/myspell/dicts",
        "/usr/share/myspell",
    ] {
        dirs.push(PathBuf::from(d));
    }
    dirs
}

/// The folder of the user's dictionaries and words.
pub fn user_dir() -> Option<PathBuf> {
    crate::state::app_data_dir().map(|d| d.join("dictionaries"))
}

/// The dictionaries that can be chosen.
pub fn available() -> Vec<DictionaryFile> {
    find_dictionaries(&dictionary_dirs())
}

/// The name of a language in its own language, with its code.
pub fn language_name(code: &str) -> String {
    let base = code.split(['_', '-']).next().unwrap_or(code);
    let name = match base {
        "en" => "English",
        "it" => "Italiano",
        "de" => "Deutsch",
        "fr" => "Français",
        "es" => "Español",
        "pt" => "Português",
        "nl" => "Nederlands",
        "pl" => "Polski",
        "cs" => "Čeština",
        "sk" => "Slovenčina",
        "sl" => "Slovenščina",
        "hr" => "Hrvatski",
        "hu" => "Magyar",
        "ro" => "Română",
        "sv" => "Svenska",
        "da" => "Dansk",
        "nb" | "no" => "Norsk",
        "fi" => "Suomi",
        "el" => "Ελληνικά",
        "ru" => "Русский",
        "uk" => "Українська",
        "bg" => "Български",
        "tr" => "Türkçe",
        "ca" => "Català",
        _ => return code.to_string(),
    };
    format!("{name} ({code})")
}

/// The file of the user's own words, one per line.
fn words_file() -> Option<PathBuf> {
    user_dir().map(|d| d.join("words.txt"))
}

/// The languages chosen (codes): unless chosen, the one of the interface and English.
pub fn chosen(st: AppState) -> Vec<String> {
    let saved = st.value("spell.languages");
    if !saved.trim().is_empty() {
        return saved
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    match st.lang() {
        Lang::It => vec!["it_IT".into(), "en_US".into()],
        Lang::En => vec!["en_US".into()],
    }
}

/// Check in language `code` too, or not.
pub fn choose_language(st: AppState, code: &str, on: bool) {
    let mut list = chosen(st);
    list.retain(|c| c != code);
    if on {
        list.push(code.to_string());
    }
    // An empty value would mean the defaults again.
    st.set_value(
        "spell.languages",
        if list.is_empty() {
            ",".to_string()
        } else {
            list.join(",")
        },
    );
    ensure_loaded(st);
}

/// The options of the check, as saved.
pub fn options(st: AppState) -> SpellOptions {
    SpellOptions {
        skip_capitals: st.value("spell.capitals") != "0",
        skip_digits: st.value("spell.digits") != "0",
    }
}

/// What the check looks at, as saved.
pub fn scope(st: AppState) -> SpellScope {
    SpellScope {
        page_text: st.value("spell.pages") != "0",
        comments: st.value("spell.comments") != "0",
        fields: st.value("spell.fields") != "0",
    }
}

/// Change a saved option of the check (`spell.capitals`, `spell.pages`, ...).
pub fn set_flag(st: AppState, key: &str, on: bool) {
    st.set_value(key, if on { "1" } else { "0" });
    if let Some(sp) = checker() {
        sp.set_options(options(st));
    }
    refresh(st);
    show_current(st);
}

/// Read the dictionaries of `languages`, with the user's words.
fn load(languages: &[String], options: SpellOptions) -> (Speller, Vec<(String, Option<String>)>) {
    let files = available();
    let mut speller = Speller::new();
    let mut missing = Vec::new();
    for code in languages {
        match files.iter().find(|f| f.code.eq_ignore_ascii_case(code)) {
            Some(f) => {
                if let Err(e) = speller.load(f) {
                    missing.push((code.clone(), Some(e.to_string())));
                }
            }
            None => missing.push((code.clone(), None)),
        }
    }
    speller.set_options(options);
    if let Some(text) = words_file().and_then(|p| std::fs::read_to_string(p).ok()) {
        for w in text.lines() {
            speller.add_word(w);
        }
    }
    (speller, missing)
}

/// Make the checker for the languages chosen, in the background, unless it
/// is there already or being made.
pub fn ensure_loaded(st: AppState) {
    let want = chosen(st);
    {
        let s = SPELL.peek();
        if s.loading || (s.speller.is_some() && s.languages == want) {
            return;
        }
    }
    let opts = options(st);
    dioxus::core::spawn_forever(async move {
        {
            let mut s = SPELL.write();
            if s.loading {
                return;
            }
            s.loading = true;
            s.languages = want.clone();
        }
        let langs = want.clone();
        let (speller, missing) = tokio::task::spawn_blocking(move || load(&langs, opts))
            .await
            .unwrap_or_else(|_| (Speller::new(), Vec::new()));
        {
            let mut s = SPELL.write();
            s.speller = Some(Arc::new(speller));
            s.missing = missing;
            s.loading = false;
        }
        // The choice changed while the dictionaries were read.
        if chosen(st) != want {
            ensure_loaded(st);
            return;
        }
        if matches!(*st.dialog.peek(), Dialog::SpellCheck) {
            begin(st);
        }
    });
}

fn save_words(sp: &Speller) {
    let Some(path) = words_file() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut text = sp.added_words().join("\n");
    text.push('\n');
    let _ = folitecta_core::util::write_atomic(path, text);
}

/// The checker made last (a copy of the handle: no borrow of the signal is
/// kept while the caller writes signals).
fn checker() -> Option<Arc<Speller>> {
    SPELL.peek().speller.clone()
}

/// The checker is ready for the languages chosen (not being read, not made
/// for other languages).
fn current_languages(st: AppState) -> bool {
    let want = chosen(st);
    let s = SPELL.peek();
    !s.loading && s.speller.is_some() && s.languages == want
}

/// Take a word out of the user's own words.
pub fn remove_word(st: AppState, word: &str) {
    if let Some(sp) = checker() {
        if sp.remove_word(word) {
            save_words(&sp);
            refresh(st);
            show_current(st);
        }
    }
}

// ---- Spell Check and Replace ------------------------------------------------------------

/// Make the list of words again when what it was made for changed (the
/// document, the checker, its words or options, what is checked), then find
/// the word shown again from where it was. False without a checker.
fn refresh(st: AppState) -> bool {
    // Not with the checker of other languages while the chosen ones are read.
    if !current_languages(st) {
        return false;
    }
    let Some(sp) = checker().filter(|s| s.has_dictionaries()) else {
        return false;
    };
    let scope = scope(st);
    st.with_active(|t| {
        let key = (t.doc.revision(), Arc::as_ptr(&sp) as usize, sp.changes(), scope);
        if t.spell.as_ref().is_some_and(|r| r.made_for == Some(key)) {
            return;
        }
        let mut list = t.doc.misspellings(&sp, scope);
        let run = t.spell.get_or_insert_with(SpellRun::default);
        list.retain(|m| {
            !run.passed
                .iter()
                .any(|(p, s, w)| *p == m.place && *s == m.start && *w == m.word)
        });
        run.list = list;
        run.made_for = Some(key);
        run.current = run.anchor.and_then(|a| {
            run.list
                .iter()
                .position(|m| m.key() >= a)
                .or((!run.list.is_empty()).then_some(0))
        });
    });
    true
}

/// Start the check from the current page.
fn begin(st: AppState) {
    let page = st.current_page();
    st.with_active(|t| {
        let run = t.spell.get_or_insert_with(SpellRun::default);
        run.made_for = None;
        run.anchor = None;
        run.current = None;
    });
    if !refresh(st) {
        return;
    }
    let first = st
        .read_active(|t| {
            let list = &t.spell.as_ref()?.list;
            list.iter()
                .position(|m| m.page.is_some_and(|p| p >= page))
                .or((!list.is_empty()).then_some(0))
        })
        .flatten();
    match first {
        Some(i) => show(st, i),
        None => *SUGGESTIONS.write() = None,
    }
}

/// The word shown, if any.
pub fn current(t: &DocTab) -> Option<&Misspelling> {
    let run = t.spell.as_ref()?;
    run.list.get(run.current?)
}

/// Show word `i` of the list: its page, the word marked, and the suggestions for it.
fn show(st: AppState, i: usize) {
    let Some(m) = st
        .with_active(|t| {
            let run = t.spell.as_mut()?;
            let m = run.list.get(i)?.clone();
            run.current = Some(i);
            run.anchor = Some(m.key());
            Some(m)
        })
        .flatten()
    else {
        return;
    };
    if let Some(page) = m.page {
        if st.current_page() != page {
            st.navigate(page, true);
        }
    }
    // The page may have to be laid out before its mark can be scrolled to.
    // On the root scope: the caller may be a task or a dialog that closes.
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_millis(90)).await;
        let _ = document::eval(
            "const m = document.querySelector('#canvas .spell-current'); if (m) { m.scrollIntoView({ block: 'center', inline: 'nearest' }); }",
        )
        .await;
    });
    *CHANGE_TO.write() = m.word.clone();
    // The text around the word tells which language to suggest from first.
    let context = st
        .read_active(|t| t.doc.spelling_text(m.place))
        .flatten()
        .map(|text| {
            let chars: Vec<char> = text.chars().collect();
            let to = (m.end + 400).min(chars.len());
            chars[m.start.saturating_sub(400).min(to)..to]
                .iter()
                .collect::<String>()
        })
        .unwrap_or_default();
    suggest(m.word, context);
}

/// Show the word that is current again (after the list was made again).
fn show_current(st: AppState) {
    match st
        .read_active(|t| t.spell.as_ref().and_then(|r| r.current))
        .flatten()
    {
        Some(i) => show(st, i),
        None => *SUGGESTIONS.write() = None,
    }
}

/// Find the suggestions for `word` (written in `context`) in the background.
/// The first one goes in "Change to" unless something else was typed there
/// meanwhile.
fn suggest(word: String, context: String) {
    let Some(sp) = checker() else {
        return;
    };
    let known = SUGGESTIONS.peek().clone().filter(|(w, _)| *w == word);
    if let Some((_, list)) = known {
        if let Some(first) = list.and_then(|l| l.first().cloned()) {
            *CHANGE_TO.write() = first;
        }
        return;
    }
    *SUGGESTIONS.write() = Some((word.clone(), None));
    dioxus::core::spawn_forever(async move {
        let w = word.clone();
        let list = tokio::task::spawn_blocking(move || sp.suggest_in(&w, &context))
            .await
            .unwrap_or_default();
        let still = SUGGESTIONS.peek().as_ref().is_some_and(|(w, _)| *w == word);
        if still {
            if let Some(first) = list.first() {
                if *CHANGE_TO.peek() == word {
                    *CHANGE_TO.write() = first.clone();
                }
            }
            *SUGGESTIONS.write() = Some((word, Some(list)));
        }
    });
}

/// Open Spell Check and Replace, from the current page.
fn open(st: AppState) {
    if no_doc(st) {
        return;
    }
    st.set_dialog(Dialog::SpellCheck);
    ensure_loaded(st);
    begin(st);
}

/// The next word (or, with `back`, the one before), going round at the end.
fn step(st: AppState, back: bool) {
    if !matches!(*st.dialog.peek(), Dialog::SpellCheck) {
        open(st);
        return;
    }
    if !refresh(st) {
        return;
    }
    let next = st
        .read_active(|t| {
            let run = t.spell.as_ref()?;
            let n = run.list.len();
            if n == 0 {
                return None;
            }
            Some(match (run.current, back) {
                (Some(i), false) => (i + 1) % n,
                (Some(i), true) => (i + n - 1) % n,
                (None, false) => 0,
                (None, true) => n - 1,
            })
        })
        .flatten();
    if let Some(i) = next {
        show(st, i);
    }
}

fn next(st: AppState) {
    step(st, false);
}

fn previous(st: AppState) {
    step(st, true);
}

/// Pass over the word shown, here only.
pub fn ignore(st: AppState) {
    if !matches!(*st.dialog.peek(), Dialog::SpellCheck) {
        open(st);
        return;
    }
    let left = st
        .with_active(|t| {
            let run = t.spell.as_mut()?;
            let i = run.current?;
            let m = run.list.remove(i);
            run.passed.push((m.place, m.start, m.word));
            run.current = if run.list.is_empty() {
                None
            } else {
                Some(i % run.list.len())
            };
            run.current
        })
        .flatten();
    match left {
        Some(i) => show(st, i),
        None => *SUGGESTIONS.write() = None,
    }
}

/// Pass over the word shown everywhere, until Folitecta closes.
pub fn ignore_all(st: AppState) {
    if !matches!(*st.dialog.peek(), Dialog::SpellCheck) {
        open(st);
        return;
    }
    let word = st.read_active(|t| current(t).map(|m| m.word.clone())).flatten();
    if let (Some(word), Some(sp)) = (word, checker()) {
        sp.ignore_word(&word);
        refresh(st);
        show_current(st);
    }
}

/// Add the word shown to the user's own words.
pub fn add_word(st: AppState) {
    let word = st.read_active(|t| current(t).map(|m| m.word.clone())).flatten();
    if let (Some(word), Some(sp)) = (word, checker()) {
        sp.add_word(&word);
        save_words(&sp);
        refresh(st);
        show_current(st);
    }
}

/// After a correction of `m` with `with`: the words passed over after it in
/// the same text moved, and the check goes on after the new text.
fn after_correction(st: AppState, m: &Misspelling, with: &str) {
    let delta = with.chars().count() as isize - (m.end - m.start) as isize;
    st.with_active(|t| {
        if let Some(run) = t.spell.as_mut() {
            for (p, s, _) in run.passed.iter_mut() {
                if *p == m.place && *s > m.start {
                    *s = (*s as isize + delta).max(0) as usize;
                }
            }
            run.anchor = Some((m.order, m.start + with.chars().count()));
        }
    });
    files::set_window_title(st);
    refresh(st);
    show_current(st);
}

/// Write `with` in place of the word shown.
pub fn change(st: AppState, with: &str) {
    let Some(m) = st.read_active(|t| current(t).cloned()).flatten() else {
        return;
    };
    let with = with.trim();
    if with.is_empty() || with == m.word {
        ignore(st);
        return;
    }
    match st.with_active(|t| t.doc.correct_spelling(&m, with)) {
        Some(Ok(report)) => {
            if !report.substituted.is_empty() {
                st.toast(
                    ToastKind::Info,
                    tf(
                        st.lang(),
                        "toast.text_substituted",
                        &[("chars", &report.substituted)],
                    ),
                    "",
                );
            }
            after_correction(st, &m, with);
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// Write `with` in place of the word shown and of the same word everywhere.
pub fn change_all(st: AppState, with: &str) {
    let Some((m, all)) = st
        .read_active(|t| {
            let m = current(t)?.clone();
            let all: Vec<Misspelling> = t
                .spell
                .as_ref()?
                .list
                .iter()
                .filter(|o| o.word == m.word)
                .cloned()
                .collect();
            Some((m, all))
        })
        .flatten()
    else {
        return;
    };
    let with = with.trim();
    if with.is_empty() || with == m.word {
        ignore_all(st);
        return;
    }
    let Some(r) = st.with_active(|t| t.doc.correct_spellings(&all, with)) else {
        return;
    };
    let lang = st.lang();
    if r.refused == 0 {
        st.toast(
            ToastKind::Ok,
            tf(lang, "toast.replaced", &[("count", &r.replaced.to_string())]),
            "",
        );
    } else {
        st.toast(
            ToastKind::Warn,
            tf(
                lang,
                "toast.replaced_some",
                &[
                    ("count", &r.replaced.to_string()),
                    ("left", &r.refused.to_string()),
                ],
            ),
            r.reason.unwrap_or_default(),
        );
    }
    // The words before the first one corrected moved only in its own text.
    let first = all.iter().min_by_key(|o| o.key()).cloned().unwrap_or(m);
    after_correction(st, &first, with);
}

/// Open the folder of the user's dictionaries (made when missing).
pub fn open_user_dir(st: AppState) {
    let Some(dir) = user_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    if let Err(e) = crate::util::open_external(&dir.to_string_lossy()) {
        st.error(e);
    }
}

// ---- Marks on the pages -----------------------------------------------------------------

/// Words the dictionaries do not know, per document and page, for the marks
/// shown while the text is edited: what they were found for, and their characters.
type Found = ((u64, usize, u64), Arc<Vec<(usize, usize)>>);
static MARKS: LazyLock<Mutex<HashMap<(u64, usize), Found>>> = LazyLock::new(Default::default);

/// The marks of the spell check on a page (display points): the words the
/// dictionaries do not know, and the word Spell Check and Replace shows.
/// `checking`: the dialog is open; `editing`: the page text is being edited
/// with the option to check spelling on.
pub fn page_marks(t: &DocTab, page: usize, checking: bool, editing: bool) -> (Vec<[f32; 4]>, Vec<[f32; 4]>) {
    let mut words = Vec::new();
    let mut shown = Vec::new();
    if !checking && !editing {
        return (words, shown);
    }
    let layer = t.doc.page_text_layer(page);
    let rects = |s: usize, e: usize| layer.as_ref().map(|l| l.range_rects(s, e)).unwrap_or_default();
    if checking {
        let run = t
            .spell
            .as_ref()
            .filter(|r| r.made_for.is_some_and(|k| k.0 == t.doc.revision()));
        if let Some(run) = run {
            for (i, m) in run.list.iter().enumerate() {
                if m.page != Some(page) {
                    continue;
                }
                let is_shown = run.current == Some(i);
                match m.place {
                    SpellPlace::Page(_) if is_shown => shown = rects(m.start, m.end),
                    SpellPlace::Page(_) => words.extend(rects(m.start, m.end)),
                    SpellPlace::Comment { index, .. } if is_shown => {
                        let size = t.page_sizes.get(page);
                        let annot = t
                            .doc
                            .annotations(page)
                            .ok()
                            .and_then(|l| l.into_iter().find(|a| a.index == index));
                        if let (Some(size), Some(a)) = (size, annot) {
                            shown = vec![pdf_rect_to_display(size, a.rect)];
                        }
                    }
                    SpellPlace::Field(id) if is_shown => {
                        let size = t.page_sizes.get(page);
                        let rect = t
                            .doc
                            .form_fields()
                            .into_iter()
                            .find(|f| f.id == id)
                            .and_then(|f| f.rect);
                        if let (Some(size), Some(r)) = (size, rect) {
                            shown = vec![pdf_rect_to_display(size, r)];
                        }
                    }
                    _ => {}
                }
            }
            return (words, shown);
        }
    }
    let Some(sp) = SPELL.read().speller.clone().filter(|s| s.has_dictionaries()) else {
        return (words, shown);
    };
    let key = (t.doc.revision(), Arc::as_ptr(&sp) as usize, sp.changes());
    let ranges = {
        let mut cache = MARKS.lock().unwrap_or_else(|e| e.into_inner());
        match cache.get(&(t.id, page)) {
            Some((k, r)) if *k == key => r.clone(),
            _ => {
                let r: Arc<Vec<(usize, usize)>> = Arc::new(
                    t.doc
                        .page_misspellings(page, &sp)
                        .into_iter()
                        .map(|m| (m.start, m.end))
                        .collect(),
                );
                if cache.len() > 256 {
                    cache.clear();
                }
                cache.insert((t.id, page), (key, r.clone()));
                r
            }
        }
    };
    for (s, e) in ranges.iter() {
        words.extend(rects(*s, *e));
    }
    (words, shown)
}

/// A line telling where the word shown is.
pub fn place_label(t: &DocTab, lang: Lang, m: &Misspelling) -> String {
    let page = m.page.map(|p| (p + 1).to_string()).unwrap_or_default();
    match m.place {
        SpellPlace::Page(_) => tf(lang, "spell.in_page", &[("page", &page)]),
        SpellPlace::Comment { page: p, index } => {
            let kind = t
                .doc
                .annotations(p)
                .ok()
                .and_then(|l| l.into_iter().find(|a| a.index == index))
                .map(|a| crate::i18n::annot_name(lang, &a.subtype))
                .unwrap_or_default();
            tf(lang, "spell.in_comment", &[("kind", &kind), ("page", &page)])
        }
        SpellPlace::Field(id) => {
            let name = t
                .doc
                .form_fields()
                .into_iter()
                .find(|f| f.id == id)
                .map(|f| f.name)
                .unwrap_or_default();
            tf(lang, "spell.in_field", &[("name", &name)])
        }
    }
}

/// The text around the word shown: before, the word, after (one line).
pub fn context(t: &DocTab, m: &Misspelling) -> Option<(String, String, String)> {
    let text: Vec<char> = t.doc.spelling_text(m.place)?.chars().collect();
    let flat = |s: &[char]| -> String {
        s.iter()
            .map(|c| if c.is_whitespace() { ' ' } else { *c })
            .collect()
    };
    let end = m.end.min(text.len());
    let start = m.start.min(end);
    let from = start.saturating_sub(48);
    let to = (end + 48).min(text.len());
    let mut before = flat(&text[from..start]);
    if from > 0 {
        before.insert(0, '…');
    }
    let mut after = flat(&text[end..to]);
    if to < text.len() {
        after.push('…');
    }
    Some((before, flat(&text[start..end]), after))
}

/// What the dialog says when there is no word to show.
pub fn status_line(st: AppState) -> Option<String> {
    let lang = st.lang();
    let want = chosen(st);
    let s = SPELL.read();
    if s.loading || s.languages != want {
        return Some(t(lang, "spell.loading"));
    }
    match &s.speller {
        None => Some(t(lang, "spell.loading")),
        Some(sp) if !sp.has_dictionaries() => Some(t(lang, "spell.no_dictionary")),
        Some(_) => None,
    }
}

handlers! {
    "document.spellCheckAndRepl" => open,
    "document.spellCheckAndRepl.forward" => next,
    "document.spellCheckAndRepl.back" => previous,
    "document.spellCheckAndRepl.ignore" => ignore,
    "document.spellCheckAndRepl.ignoreAll" => ignore_all,
}
