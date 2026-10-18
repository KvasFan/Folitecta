//! Spell checking: the words of the page text, of the comments and of the
//! form fields that the dictionaries do not know, suggestions for them, and
//! their correction.
//!
//! The dictionaries are Hunspell files (`<code>.aff` and `<code>.dic`) read at
//! run time from the folders the application chooses (the source tree keeps
//! them in `dictionaries/`, each with its own licence); spellbook checks the
//! words. A word is right when any of the dictionaries loaded knows it, so a
//! text mixing two languages is checked with both.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use lopdf::ObjectId;
use rayon::prelude::*;

use crate::document::Document;
use crate::error::{Error, Result};
use crate::forms::FieldKind;
use crate::textedit::{ReplaceReport, TextEditReport};

// ---- Dictionaries ---------------------------------------------------------------------

/// The two files of a Hunspell dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryFile {
    /// Language code: the name of the files (`en_US`, `it_IT`).
    pub code: String,
    pub aff: PathBuf,
    pub dic: PathBuf,
}

/// The dictionaries of the folders: every `<code>.aff` with a `<code>.dic`
/// beside it, the first folder winning for a code; sorted by code.
pub fn find_dictionaries(dirs: &[PathBuf]) -> Vec<DictionaryFile> {
    let mut out: Vec<DictionaryFile> = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for aff in entries.flatten().map(|e| e.path()) {
            if !aff.extension().is_some_and(|x| x.eq_ignore_ascii_case("aff")) {
                continue;
            }
            let Some(code) = aff.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
                continue;
            };
            let dic = aff.with_extension("dic");
            if dic.is_file() && !out.iter().any(|o| o.code.eq_ignore_ascii_case(&code)) {
                out.push(DictionaryFile { code, aff, dic });
            }
        }
    }
    out.sort_by(|a, b| a.code.cmp(&b.code));
    out
}

/// The encoding an `.aff` file names on its `SET` line.
fn encoding_of(aff: &[u8]) -> String {
    aff.split(|b| *b == b'\n')
        .map(|l| String::from_utf8_lossy(l).trim().to_string())
        .find_map(|l| l.strip_prefix("SET").map(|s| s.trim().to_ascii_uppercase()))
        .unwrap_or_else(|| "UTF-8".into())
}

/// The text of a dictionary file: UTF-8, or the bytes of the older Latin
/// dictionaries (ISO8859-1, ISO8859-15, Windows-1252) that say so with `SET`.
fn decode(bytes: &[u8], set: &str) -> Result<String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok(s.to_string());
    }
    let set = set.replace(['-', '_'], "");
    let latin = |b: u8| -> char {
        match (set.as_str(), b) {
            ("ISO885915", 0xA4) => '€',
            ("ISO885915", 0xA6) => 'Š',
            ("ISO885915", 0xA8) => 'š',
            ("ISO885915", 0xB4) => 'Ž',
            ("ISO885915", 0xB8) => 'ž',
            ("ISO885915", 0xBC) => 'Œ',
            ("ISO885915", 0xBD) => 'œ',
            ("ISO885915", 0xBE) => 'Ÿ',
            ("WINDOWS1252" | "CP1252", 0x80..=0x9F) => {
                const HIGH: &str = "€\u{81}‚ƒ„…†‡ˆ‰Š‹Œ\u{8D}Ž\u{8F}\u{90}‘’“”•–—˜™š›œ\u{9D}žŸ";
                HIGH.chars().nth((b - 0x80) as usize).unwrap_or('?')
            }
            _ => b as char,
        }
    };
    match set.as_str() {
        "ISO88591" | "ISO885915" | "WINDOWS1252" | "CP1252" => Ok(bytes.iter().map(|b| latin(*b)).collect()),
        _ => Err(Error::Invalid(format!(
            "the dictionary is written in the {set} encoding; only UTF-8 and Latin-1 dictionaries are read"
        ))),
    }
}

// ---- Words ----------------------------------------------------------------------------

/// A word of a text: its characters `start..end` and its spelling as checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextWord {
    pub start: usize,
    pub end: usize,
    /// The word as the dictionaries see it: straight apostrophes, ligatures
    /// written as their letters, soft hyphens left out.
    pub word: String,
    /// The word goes on in the next one, after a hyphen at the end of the line.
    pub hyphenated: bool,
    /// An apostrophe follows the word (`po'`, `students'`).
    pub apostrophe_after: bool,
}

fn is_letter(c: char) -> bool {
    c.is_alphanumeric() || is_combining(c)
}

fn is_combining(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36F)
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | 'ʼ')
}

fn is_hyphen(c: char) -> bool {
    matches!(c, '-' | '\u{2010}' | '\u{AD}')
}

/// Characters of web and e-mail addresses, paths and file names: their
/// pieces are not words to check.
fn address_mask(chars: &[char]) -> Vec<bool> {
    let mut mask = vec![false; chars.len()];
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let s = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let chunk: String = chars[s..i].iter().collect::<String>().to_lowercase();
        let at = chunk.find('@');
        // A dot with letters on both sides: a domain or a file name.
        let inner_dot = chars[s..i]
            .windows(3)
            .any(|w| w[0].is_alphanumeric() && w[1] == '.' && w[2].is_alphanumeric());
        let address = chunk.contains("://")
            || chunk.starts_with("www.")
            || chunk.contains('\\')
            || at.is_some_and(|a| chunk[a..].contains('.'))
            || inner_dot;
        if address {
            mask[s..i].iter_mut().for_each(|m| *m = true);
        }
    }
    mask
}

/// The words of a text, with what is needed to check them.
pub fn text_words(text: &str) -> Vec<TextWord> {
    let chars: Vec<char> = text.chars().collect();
    let skip = address_mask(&chars);
    let n = chars.len();
    let letter_at = |i: usize| i < n && chars[i].is_alphabetic();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if !is_letter(chars[i]) {
            i += 1;
            continue;
        }
        let s = i;
        while i < n {
            let c = chars[i];
            // An apostrophe or a soft hyphen between letters is in the word.
            let joins = (is_apostrophe(c) || c == '\u{AD}') && letter_at(i + 1);
            if is_letter(c) || joins {
                i += 1;
            } else {
                break;
            }
        }
        let e = i;
        if skip[s] {
            continue;
        }
        let mut word = String::with_capacity(e - s);
        for &c in &chars[s..e] {
            match c {
                '’' | 'ʼ' => word.push('\''),
                '\u{AD}' => {}
                'ﬀ' => word.push_str("ff"),
                'ﬁ' => word.push_str("fi"),
                'ﬂ' => word.push_str("fl"),
                'ﬃ' => word.push_str("ffi"),
                'ﬄ' => word.push_str("ffl"),
                'ﬅ' | 'ﬆ' => word.push_str("st"),
                _ => word.push(c),
            }
        }
        // "exam-" at the end of a line and "ple" at the start of the next.
        let mut k = e + 1;
        while k < n && matches!(chars[k], '\r' | '\n') {
            k += 1;
        }
        let hyphenated = e < n && is_hyphen(chars[e]) && k > e + 1 && letter_at(k);
        out.push(TextWord {
            start: s,
            end: e,
            word,
            hyphenated,
            apostrophe_after: e < n && is_apostrophe(chars[e]),
        });
    }
    out
}

/// The spelling a dictionary sees: straight apostrophes.
fn plain(word: &str) -> String {
    word.chars()
        .map(|c| if is_apostrophe(c) { '\'' } else { c })
        .filter(|c| *c != '\u{AD}')
        .collect()
}

/// A lower-case letter followed by a capital: names such as "iPhone" or
/// "JavaScript", which the dictionaries rarely have.
fn mixed_case(word: &str) -> bool {
    let mut lower = false;
    for c in word.chars() {
        if c.is_lowercase() {
            lower = true;
        } else if c.is_uppercase() && lower {
            return true;
        }
    }
    false
}

/// How far apart two words are as typing mistakes go, in half steps,
/// counting at most `cap`: two neighbouring letters swapped or a wrong or
/// missing accent cost 1, a letter added, left out or changed 2. Case does
/// not count.
fn typo_cost(a: &str, b: &str, cap: usize) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    if 2 * a.len().abs_diff(b.len()) >= cap {
        return cap;
    }
    let base = crate::textlayer::strip_diacritic;
    let w = b.len() + 1;
    let mut d = vec![0usize; (a.len() + 1) * w];
    for (j, v) in d.iter_mut().take(w).enumerate() {
        *v = 2 * j;
    }
    for i in 1..=a.len() {
        d[i * w] = 2 * i;
        for j in 1..=b.len() {
            let change = match (a[i - 1], b[j - 1]) {
                (x, y) if x == y => 0,
                (x, y) if base(x) == base(y) => 1,
                _ => 2,
            };
            let mut v = (d[(i - 1) * w + j] + 2)
                .min(d[i * w + j - 1] + 2)
                .min(d[(i - 1) * w + j - 1] + change);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[(i - 2) * w + j - 2] + 1);
            }
            d[i * w + j] = v;
        }
    }
    d[a.len() * w + b.len()].min(cap)
}

// ---- The checker ----------------------------------------------------------------------

/// What the check passes over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellOptions {
    /// Words in capitals only, such as acronyms and codes.
    pub skip_capitals: bool,
    /// Words with digits.
    pub skip_digits: bool,
}

impl Default for SpellOptions {
    fn default() -> Self {
        SpellOptions {
            skip_capitals: true,
            skip_digits: true,
        }
    }
}

/// The dictionaries loaded, with the words the user added and those ignored.
pub struct Speller {
    dicts: Vec<(String, spellbook::Dictionary)>,
    /// The user's own words: right in every language.
    added: RwLock<HashSet<String>>,
    /// Words taken as right while the checker lives (Ignore All).
    ignored: RwLock<HashSet<String>>,
    options: RwLock<SpellOptions>,
    /// Counts the changes of the words and options, for the caches of the callers.
    changes: AtomicU64,
}

impl Default for Speller {
    fn default() -> Self {
        Speller::new()
    }
}

impl std::fmt::Debug for Speller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Speller")
            .field("languages", &self.languages())
            .finish_non_exhaustive()
    }
}

/// Whether `word` is in `set` as the dictionaries take case: a word listed
/// in small letters is right with a capital or in capitals, a name listed
/// with its capital is right in capitals too, not in small letters.
fn listed(set: &RwLock<HashSet<String>>, word: &str) -> bool {
    let set = set.read().unwrap_or_else(|e| e.into_inner());
    if set.is_empty() {
        return false;
    }
    if set.contains(word) || set.contains(&word.to_lowercase()) {
        return true;
    }
    let mut chars = word.chars();
    let capitals = word.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase);
    capitals
        && chars.next().is_some_and(|first| {
            let title: String = first
                .to_uppercase()
                .chain(chars.as_str().to_lowercase().chars())
                .collect();
            set.contains(&title)
        })
}

impl Speller {
    pub fn new() -> Speller {
        Speller {
            dicts: Vec::new(),
            added: RwLock::new(HashSet::new()),
            ignored: RwLock::new(HashSet::new()),
            options: RwLock::new(SpellOptions::default()),
            changes: AtomicU64::new(0),
        }
    }

    /// Add a dictionary from the text of its two files.
    pub fn add_dictionary(&mut self, code: &str, aff: &str, dic: &str) -> Result<()> {
        let dict = spellbook::Dictionary::new(aff, dic)
            .map_err(|e| Error::Invalid(format!("the {code} dictionary cannot be read: {e}")))?;
        self.dicts.push((code.to_string(), dict));
        Ok(())
    }

    /// Read the files of a dictionary and add it.
    pub fn load(&mut self, file: &DictionaryFile) -> Result<()> {
        let aff = std::fs::read(&file.aff).map_err(|e| Error::io(&file.aff, e))?;
        let dic = std::fs::read(&file.dic).map_err(|e| Error::io(&file.dic, e))?;
        let set = encoding_of(&aff);
        self.add_dictionary(&file.code, &decode(&aff, &set)?, &decode(&dic, &set)?)
    }

    /// Codes of the dictionaries loaded, in the order they were added.
    pub fn languages(&self) -> Vec<String> {
        self.dicts.iter().map(|(c, _)| c.clone()).collect()
    }

    pub fn has_dictionaries(&self) -> bool {
        !self.dicts.is_empty()
    }

    pub fn options(&self) -> SpellOptions {
        *self.options.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_options(&self, options: SpellOptions) {
        let mut o = self.options.write().unwrap_or_else(|e| e.into_inner());
        if *o != options {
            *o = options;
            self.changes.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Grows with every change of the words added or ignored and of the options.
    pub fn changes(&self) -> u64 {
        self.changes.load(Ordering::Relaxed)
    }

    /// Take `word` as right in every language (the user's own words).
    /// False when it was there already.
    pub fn add_word(&self, word: &str) -> bool {
        let word = plain(word.trim());
        if word.is_empty() {
            return false;
        }
        let new = self.added.write().unwrap_or_else(|e| e.into_inner()).insert(word);
        if new {
            self.changes.fetch_add(1, Ordering::Relaxed);
        }
        new
    }

    /// Take `word` out of the user's own words. False when it was not there.
    pub fn remove_word(&self, word: &str) -> bool {
        let gone = self
            .added
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&plain(word.trim()));
        if gone {
            self.changes.fetch_add(1, Ordering::Relaxed);
        }
        gone
    }

    /// The user's own words, sorted.
    pub fn added_words(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .added
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect();
        v.sort_by_key(|w| w.to_lowercase());
        v
    }

    /// Take `word` as right from now on (Ignore All).
    pub fn ignore_word(&self, word: &str) {
        if self
            .ignored
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(plain(word))
        {
            self.changes.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Whether a dictionary knows `word`, or the user added or ignored it.
    /// Without dictionaries every word is right.
    pub fn check(&self, word: &str) -> bool {
        if self.dicts.is_empty() {
            return true;
        }
        let word = plain(word);
        listed(&self.added, &word)
            || listed(&self.ignored, &word)
            || self.dicts.iter().any(|(_, d)| d.check(&word))
    }

    /// Whether a word of a text is right, or one the check passes over.
    fn word_ok(&self, w: &TextWord, opts: SpellOptions) -> bool {
        let letters = w.word.chars().filter(|c| c.is_alphabetic()).count();
        if letters < 2
            || (opts.skip_digits && w.word.chars().any(char::is_numeric))
            || (opts.skip_capitals
                && w.word
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .all(char::is_uppercase))
            || mixed_case(&w.word)
            // Accents written as separate marks, which no dictionary spells so.
            || w.word.chars().any(is_combining)
            || self.check(&w.word)
        {
            return true;
        }
        if w.apostrophe_after && self.check(&format!("{}'", w.word)) {
            return true;
        }
        // Elisions and contractions a dictionary only knows by their parts.
        w.word.contains('\'')
            && w.word
                .split('\'')
                .all(|p| p.chars().count() < 2 || self.check(p) || self.check(&format!("{p}'")))
    }

    /// Character ranges and spellings of the words of `text` that are not right.
    pub fn misspelled_in(&self, text: &str) -> Vec<(usize, usize, String)> {
        if self.dicts.is_empty() {
            return Vec::new();
        }
        let opts = self.options();
        let words = text_words(text);
        let mut out = Vec::new();
        let mut k = 0;
        while k < words.len() {
            let w = &words[k];
            // The two halves of a word divided at the end of a line.
            if w.hyphenated && k + 1 < words.len() {
                if self.check(&format!("{}{}", w.word, words[k + 1].word)) {
                    k += 2;
                    continue;
                }
            }
            if !self.word_ok(w, opts) {
                out.push((w.start, w.end, w.word.clone()));
            }
            k += 1;
        }
        out
    }

    /// Words to write instead of `word`, from every dictionary, the likeliest first.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        self.suggest_in(word, "")
    }

    /// Words to write instead of `word`, the likeliest first: single words
    /// before words split in two, the closer typing mistakes first, and at
    /// the same distance the words of the language `context` (the text
    /// around the word) is written in.
    pub fn suggest_in(&self, word: &str, context: &str) -> Vec<String> {
        let word = plain(word);
        // The dictionaries by how many words of the context they know.
        let known: Vec<String> = text_words(context)
            .into_iter()
            .map(|w| w.word)
            .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() > 1)
            .take(200)
            .collect();
        let mut order: Vec<(usize, usize)> = self
            .dicts
            .iter()
            .enumerate()
            .map(|(i, (_, d))| (known.iter().filter(|w| d.check(w)).count(), i))
            .collect();
        order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut found: Vec<(bool, usize, usize, usize, String)> = Vec::new();
        let mut buf = Vec::new();
        for (place, (_, d)) in order.iter().enumerate() {
            self.dicts[*d].1.suggest(&word, &mut buf);
            for (rank, s) in buf.drain(..).enumerate() {
                if s != word && !found.iter().any(|f| f.4 == s) {
                    found.push((s.contains(' '), typo_cost(&word, &s, 8), place, rank, s));
                }
            }
        }
        found.sort();
        found.into_iter().take(12).map(|f| f.4).collect()
    }
}

// ---- Documents ------------------------------------------------------------------------

/// Where a word was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpellPlace {
    /// The text of a page (characters of its text layer).
    Page(usize),
    /// The text of comment `index` of `page`.
    Comment { page: usize, index: usize },
    /// The value of a text field.
    Field(ObjectId),
}

/// A word of a document that the dictionaries do not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misspelling {
    pub place: SpellPlace,
    /// The page the word is on (for a field, the page of its first widget).
    pub page: Option<usize>,
    /// Position of the place in reading order: page by page, the text of the
    /// page, then its comments, then its fields.
    pub order: usize,
    /// Characters of the text of the place.
    pub start: usize,
    pub end: usize,
    pub word: String,
}

impl Misspelling {
    /// Sorts the words in reading order.
    pub fn key(&self) -> (usize, usize) {
        (self.order, self.start)
    }
}

/// What the check looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellScope {
    pub page_text: bool,
    pub comments: bool,
    pub fields: bool,
}

impl Default for SpellScope {
    fn default() -> Self {
        SpellScope {
            page_text: true,
            comments: true,
            fields: true,
        }
    }
}

fn splice(text: &str, start: usize, end: usize, with: &str) -> String {
    let mut out: String = text.chars().take(start).collect();
    out.push_str(with);
    out.extend(text.chars().skip(end));
    out
}

impl Document {
    /// The words of the text of `page` that `speller` does not know.
    pub fn page_misspellings(&self, page: usize, speller: &Speller) -> Vec<Misspelling> {
        let layers = self.text_layers();
        let Some(layer) = layers.get(page) else {
            return Vec::new();
        };
        speller
            .misspelled_in(&layer.text)
            .into_iter()
            .map(|(start, end, word)| Misspelling {
                place: SpellPlace::Page(page),
                page: Some(page),
                order: 0,
                start,
                end,
                word,
            })
            .collect()
    }

    /// The texts the check looks at, in reading order: place, page and text.
    fn spelling_places(&self, scope: SpellScope) -> Vec<(SpellPlace, Option<usize>, String)> {
        let layers = self.text_layers();
        let mut fields: Vec<(Option<usize>, ObjectId, String)> = Vec::new();
        if scope.fields {
            for f in self.form_fields() {
                if f.kind == FieldKind::Text && !f.read_only && !f.barcode && !f.value.trim().is_empty() {
                    fields.push((f.page, f.id, f.value));
                }
            }
        }
        let mut out = Vec::new();
        for page in 0..self.page_count() {
            if scope.page_text {
                if let Some(l) = layers.get(page) {
                    out.push((SpellPlace::Page(page), Some(page), l.text.clone()));
                }
            }
            if scope.comments {
                for a in self.annotations(page).unwrap_or_default() {
                    if a.contents.trim().is_empty()
                        || crate::annots::NON_COMMENT_SUBTYPES.contains(&a.subtype.as_str())
                    {
                        continue;
                    }
                    out.push((
                        SpellPlace::Comment { page, index: a.index },
                        Some(page),
                        a.contents,
                    ));
                }
            }
            for (_, id, value) in fields.iter().filter(|f| f.0 == Some(page)) {
                out.push((SpellPlace::Field(*id), Some(page), value.clone()));
            }
        }
        for (_, id, value) in fields.into_iter().filter(|f| f.0.is_none()) {
            out.push((SpellPlace::Field(id), None, value));
        }
        out
    }

    /// Every word of the document that `speller` does not know, in reading order.
    pub fn misspellings(&self, speller: &Speller, scope: SpellScope) -> Vec<Misspelling> {
        let places = self.spelling_places(scope);
        places
            .par_iter()
            .enumerate()
            .flat_map_iter(|(order, (place, page, text))| {
                speller
                    .misspelled_in(text)
                    .into_iter()
                    .map(move |(start, end, word)| Misspelling {
                        place: *place,
                        page: *page,
                        order,
                        start,
                        end,
                        word,
                    })
            })
            .collect()
    }

    /// The text of a place: the text of the page, of the comment, or the
    /// value of the field.
    pub fn spelling_text(&self, place: SpellPlace) -> Option<String> {
        match place {
            SpellPlace::Page(p) => self.text_layers().get(p).map(|l| l.text.clone()),
            SpellPlace::Comment { page, index } => self
                .annotations(page)
                .ok()?
                .into_iter()
                .find(|a| a.index == index)
                .map(|a| a.contents),
            SpellPlace::Field(id) => self
                .form_fields()
                .into_iter()
                .find(|f| f.id == id)
                .map(|f| f.value),
        }
    }

    /// Write `with` in place of the word `m` (one undo step). The word must
    /// still be where it was found.
    pub fn correct_spelling(&mut self, m: &Misspelling, with: &str) -> Result<TextEditReport> {
        let text = self
            .spelling_text(m.place)
            .ok_or_else(|| Error::Invalid("the text of the word is gone".into()))?;
        let found: String = text
            .chars()
            .skip(m.start)
            .take(m.end.saturating_sub(m.start))
            .collect();
        let same = text_words(&found)
            .first()
            .is_some_and(|w| w.word == m.word && w.end - w.start == found.chars().count());
        if !same {
            return Err(Error::Invalid(format!("\"{}\" is no longer there", m.word)));
        }
        match m.place {
            SpellPlace::Page(page) => self.edit_page_text(page, m.start, m.end, with),
            SpellPlace::Comment { page, index } => {
                self.set_annotation_contents(page, index, &splice(&text, m.start, m.end, with))?;
                Ok(TextEditReport::default())
            }
            SpellPlace::Field(id) => {
                self.set_field_value(id, &splice(&text, m.start, m.end, with))?;
                Ok(TextEditReport::default())
            }
        }
    }

    /// Write `with` in place of every word of `list`, as one undo step. The
    /// words that cannot be changed are left and counted.
    pub fn correct_spellings(&mut self, list: &[Misspelling], with: &str) -> ReplaceReport {
        // From the last word of a place back: a correction moves only the text after it.
        let mut list = list.to_vec();
        list.sort_by_key(|m| std::cmp::Reverse(m.key()));
        let mark = self.undo_mark();
        let mut report = ReplaceReport::default();
        for m in &list {
            match self.correct_spelling(m, with) {
                Ok(r) => {
                    report.replaced += 1;
                    for c in r.substituted.chars() {
                        if !report.substituted.contains(c) {
                            report.substituted.push(c);
                        }
                    }
                }
                Err(e) => {
                    report.refused += 1;
                    report.reason.get_or_insert_with(|| e.to_string());
                }
            }
        }
        self.squash_undo(&mark);
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_of_a_text() {
        let w = text_words("L’albero dell'anno: exam-\nple, www.example.com e-mail a@b.it ﬁne.");
        let spelled: Vec<&str> = w.iter().map(|w| w.word.as_str()).collect();
        assert_eq!(
            spelled,
            ["L'albero", "dell'anno", "exam", "ple", "e", "mail", "fine"]
        );
        assert!(w[2].hyphenated && !w[3].hyphenated);
        // Ranges are in characters of the text.
        assert_eq!((w[0].start, w[0].end), (0, 8));
        assert!(text_words("po' di").first().is_some_and(|w| w.apostrophe_after));
    }

    #[test]
    fn typing_mistakes() {
        assert_eq!(typo_cost("teh", "the", 8), 1);
        assert_eq!(typo_cost("perche", "perché", 8), 1);
        assert_eq!(typo_cost("speling", "spelling", 8), 2);
        assert_eq!(typo_cost("teh", "eh", 8), 2);
        assert_eq!(typo_cost("abc", "xyz", 5), 5);
        assert_eq!(typo_cost("Word", "word", 8), 0);
    }

    #[test]
    fn latin_dictionaries() {
        assert_eq!(decode(b"caf\xe9", "ISO8859-1").unwrap(), "café");
        assert_eq!(decode(b"\xa4", "ISO8859-15").unwrap(), "€");
        assert_eq!(decode(b"\x93a\x94", "WINDOWS-1252").unwrap(), "“a”");
        assert!(decode(b"\xe9", "KOI8-R").is_err());
        assert_eq!(encoding_of(b"# x\nSET ISO8859-1\nTRY abc"), "ISO8859-1");
        assert_eq!(encoding_of(b"TRY abc"), "UTF-8");
    }
}
