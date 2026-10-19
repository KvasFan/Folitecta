//! The words of the removed text ("terms"), to find the places of the
//! document that quote them (bookmarks, link targets, destination names,
//! tags, document information) and to check that the saved file holds them
//! nowhere it should not.
//!
//! The terms come from the glyphs actually removed (hidden ones included),
//! expanded to whole words. For each mark: the touched words as one phrase,
//! and — when the mark holds at most six words — each word of four letters
//! or more (or with a digit) that is not a common word. Matching ignores
//! case, respects word boundaries, and a space matches any run of white space.

use std::collections::BTreeSet;

use crate::page_text::probe::ProbeGlyph;

/// A word or phrase of the removed text and what replaces it where it is quoted.
#[derive(Clone, PartialEq, Eq)]
pub struct Term {
    /// Lower case.
    pub(crate) text: Vec<char>,
    /// As written on the page.
    pub(crate) shown: String,
    pub(crate) replacement: String,
}

impl std::fmt::Debug for Term {
    // The removed text is never written to logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Term({} characters)", self.text.len())
    }
}

impl Term {
    /// The term as written on the page (for the person applying the redaction).
    pub fn shown(&self) -> &str {
        &self.shown
    }

    pub(crate) fn new(shown: &str, replacement: &str) -> Term {
        let norm: String = shown.split_whitespace().collect::<Vec<_>>().join(" ");
        Term {
            text: norm.chars().map(lower).collect(),
            shown: norm,
            replacement: replacement.to_string(),
        }
    }
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Common words never taken as terms on their own.
const COMMON: &[&str] = &[
    "about", "after", "also", "been", "before", "being", "between", "both", "but", "could", "does", "each",
    "from", "have", "here", "into", "just", "more", "most", "much", "must", "only", "other", "over", "page",
    "pages", "same", "should", "some", "such", "than", "that", "their", "them", "then", "there", "these",
    "they", "this", "those", "under", "very", "were", "what", "when", "where", "which", "while", "will",
    "with", "would", "your", "anche", "come", "della", "delle", "dello", "degli", "dalla", "dalle", "dagli",
    "nella", "nelle", "negli", "sulla", "sulle", "sugli", "alla", "alle", "agli", "dopo", "prima", "questo",
    "questa", "questi", "queste", "quello", "quella", "quelli", "quelle", "sono", "essere", "avere", "stato",
    "stata", "ogni", "tutti", "tutte", "molto", "perché", "quando", "dove", "pagina", "pagine", "loro",
    "anno",
];

/// A single word worth looking for elsewhere.
fn word_worth(w: &str) -> bool {
    let n = w.chars().filter(|c| c.is_alphanumeric()).count();
    let digit = w.chars().any(|c| c.is_ascii_digit());
    (n >= 4 || (n >= 3 && digit)) && !COMMON.contains(&w.to_lowercase().as_str())
}

/// The origin, advance and height of a glyph from its quad (display space).
fn geometry(g: &ProbeGlyph) -> ((f32, f32), (f32, f32), f32) {
    let q = g.quad;
    // Corners: upper left, upper right, lower left, lower right; the
    // baseline lies a fifth of the way up from the bottom.
    let (ul, ur, ll, lr) = ((q[0], q[1]), (q[2], q[3]), (q[4], q[5]), (q[6], q[7]));
    let origin = (ll.0 + (ul.0 - ll.0) * 0.2, ll.1 + (ul.1 - ll.1) * 0.2);
    let advance = (lr.0 - ll.0, lr.1 - ll.1);
    let height = ((ul.0 - ll.0).powi(2) + (ul.1 - ll.1).powi(2)).sqrt();
    let _ = ur;
    (origin, advance, height)
}

/// The next glyph continues the word of `a` (no gap, same line).
fn adjacent(a: &ProbeGlyph, b: &ProbeGlyph) -> bool {
    let ((ox, oy), (ax, ay), h) = geometry(a);
    let ((bx, by), _, _) = geometry(b);
    let (ex, ey) = (ox + ax, oy + ay);
    ((bx - ex).powi(2) + (by - ey).powi(2)).sqrt() < 0.25 * h.max(0.1)
}

fn meets(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

/// The terms of the glyphs a page drew (in drawing order) under `marks`
/// (display space, each with the text that replaces its quotes).
pub(crate) fn terms_of_page(glyphs: &[ProbeGlyph], marks: &[([f32; 4], String)]) -> Vec<Term> {
    let glyphs: Vec<&ProbeGlyph> = glyphs
        .iter()
        .filter(|g| !g.covered && !g.text.is_empty() && g.text != "\u{0}")
        .collect();
    // The text of the page with the glyph of each character.
    let mut chars: Vec<(char, Option<usize>)> = Vec::new();
    for (i, g) in glyphs.iter().enumerate() {
        for c in g.text.chars() {
            chars.push((c, Some(i)));
        }
        if glyphs.get(i + 1).is_none_or(|n| !adjacent(g, n)) {
            chars.push((' ', None));
        }
    }
    // Words: runs of letters and digits, with their glyphs.
    let mut words: Vec<(String, BTreeSet<usize>)> = Vec::new();
    let mut cur = (String::new(), BTreeSet::new());
    for (c, g) in &chars {
        if c.is_alphanumeric() {
            cur.0.push(*c);
            cur.1.extend(*g);
        } else if !cur.0.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
    }
    if !cur.0.is_empty() {
        words.push(cur);
    }
    let mut out: Vec<Term> = Vec::new();
    let mut push = |t: Term| {
        if !t.text.is_empty() && !out.iter().any(|o| o.text == t.text) {
            out.push(t);
        }
    };
    for (rect, replacement) in marks {
        let removed: BTreeSet<usize> = glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| meets(&g.body, rect))
            .map(|(i, _)| i)
            .collect();
        let touched: Vec<&String> = words
            .iter()
            .filter(|(_, gs)| gs.iter().any(|g| removed.contains(g)))
            .map(|(w, _)| w)
            .collect();
        if touched.is_empty() {
            continue;
        }
        let phrase = touched.iter().map(|w| w.as_str()).collect::<Vec<_>>().join(" ");
        if touched.len() >= 2 && phrase.chars().filter(|c| c.is_alphanumeric()).count() >= 3 {
            push(Term::new(&phrase, replacement));
        }
        if touched.len() <= 6 {
            for w in touched {
                if word_worth(w) {
                    push(Term::new(w, replacement));
                }
            }
        }
    }
    out
}

/// Where `term` matches `text` at `i` (on word boundaries): the end.
fn match_at(text: &[char], i: usize, term: &[char]) -> Option<usize> {
    if i > 0 && text[i - 1].is_alphanumeric() {
        return None;
    }
    let mut j = i;
    let mut k = 0;
    while k < term.len() {
        if term[k] == ' ' {
            let start = j;
            while j < text.len() && text[j].is_whitespace() {
                j += 1;
            }
            if j == start {
                return None;
            }
        } else {
            if j >= text.len() || lower(text[j]) != term[k] {
                return None;
            }
            j += 1;
        }
        k += 1;
    }
    if j < text.len() && text[j].is_alphanumeric() {
        return None;
    }
    Some(j)
}

/// How many times the terms occur in `s`.
pub(crate) fn count_terms(s: &str, terms: &[Term]) -> usize {
    if terms.is_empty() || s.is_empty() {
        return 0;
    }
    let text: Vec<char> = s.chars().collect();
    let mut order: Vec<&Term> = terms.iter().collect();
    order.sort_by_key(|t| std::cmp::Reverse(t.text.len()));
    let mut n = 0;
    let mut i = 0;
    while i < text.len() {
        match order.iter().find_map(|t| match_at(&text, i, &t.text)) {
            Some(end) if end > i => {
                n += 1;
                i = end;
            }
            _ => i += 1,
        }
    }
    n
}

/// `s` with the terms replaced (None when it holds none).
pub(crate) fn replace_terms(s: &str, terms: &[Term]) -> Option<String> {
    if terms.is_empty() {
        return None;
    }
    let text: Vec<char> = s.chars().collect();
    let mut order: Vec<&Term> = terms.iter().collect();
    order.sort_by_key(|t| std::cmp::Reverse(t.text.len()));
    let mut out = String::with_capacity(s.len());
    let mut changed = false;
    let mut i = 0;
    while i < text.len() {
        match order
            .iter()
            .find_map(|t| match_at(&text, i, &t.text).map(|e| (e, t)))
        {
            Some((end, t)) if end > i => {
                out.push_str(&t.replacement);
                changed = true;
                i = end;
            }
            _ => {
                out.push(text[i]);
                i += 1;
            }
        }
    }
    changed.then_some(out)
}

/// The terms found in raw bytes read as Latin-1 text (streams, names).
pub(crate) fn count_in_bytes(bytes: &[u8], terms: &[Term]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let s: String = bytes.iter().map(|b| *b as char).collect();
    count_terms(&s, terms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(c: char, x: f32) -> ProbeGlyph {
        // A glyph 6 wide, 10 high, baseline at y = 100 (display space, y down).
        ProbeGlyph {
            op: Some(0),
            chain: Vec::new(),
            text: c.to_string(),
            quad: [x, 92.0, x + 6.0, 92.0, x, 102.0, x + 6.0, 102.0],
            body: [x, 93.0, x + 6.0, 101.0],
            to_next: Some(600.0),
            own_advance: Some(600.0),
            covered: false,
        }
    }

    fn line(text: &str) -> Vec<ProbeGlyph> {
        let mut x = 10.0;
        let mut out = Vec::new();
        for c in text.chars() {
            if c != ' ' {
                out.push(glyph(c, x));
            }
            x += 6.0;
        }
        out
    }

    fn texts(t: &[Term]) -> Vec<String> {
        t.iter().map(|t| t.text.iter().collect()).collect()
    }

    #[test]
    fn words_of_a_mark_and_their_phrase() {
        let g = line("Interview with John Smith today");
        // The mark covers "John Smi".
        let t = terms_of_page(&g, &[([94.0, 90.0, 154.0, 104.0], "[r]".into())]);
        assert_eq!(texts(&t), vec!["john smith", "john", "smith"]);
        let s = "Interview with John  Smith; Johnson stays";
        assert_eq!(replace_terms(s, &t).unwrap(), "Interview with [r]; Johnson stays");
        assert_eq!(count_terms("JOHN met smith", &t), 2);
    }

    #[test]
    fn long_areas_give_their_phrase_and_common_words_never_count() {
        let g = line("one two three four five six seven");
        let t = terms_of_page(&g, &[([0.0, 0.0, 500.0, 200.0], "[r]".into())]);
        assert_eq!(texts(&t), vec!["one two three four five six seven"]);
        let g = line("plan for this");
        let t = terms_of_page(&g, &[([0.0, 0.0, 500.0, 200.0], "[r]".into())]);
        assert_eq!(texts(&t), vec!["plan for this", "plan"]);
        let g = line("Code 4711");
        let t = terms_of_page(&g, &[([40.0, 0.0, 500.0, 200.0], "x".into())]);
        assert_eq!(texts(&t), vec!["4711"]);
    }
}
