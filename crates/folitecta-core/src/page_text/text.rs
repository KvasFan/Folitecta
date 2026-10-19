//! Text extraction and search, built on the positioned text layer.

use std::sync::Arc;

use crate::document::Document;
use crate::textlayer::PageText;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_words: bool,
    /// Accented letters match their base letter (`é` matches `e`).
    pub ignore_diacritics: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub page: usize,
    /// Ordinal of the hit within its page.
    pub index: usize,
    /// Character range `[start, end)` inside the page text layer.
    pub start: usize,
    pub end: usize,
    /// Rectangles covering the hit, display space (points, y down).
    pub rects: Vec<[f32; 4]>,
    pub snippet: String,
    /// Which word of the query it matches (0 for a phrase).
    pub word: usize,
}

/// A hit of the character range `[start, end)` of a page, with some context.
pub(crate) fn make_hit(
    page: usize,
    index: usize,
    layer: &PageText,
    chars: &[char],
    start: usize,
    end: usize,
    word: usize,
) -> SearchHit {
    let from = start.saturating_sub(40);
    let to = (end + 40).min(chars.len());
    let mut snippet: String = chars[from.min(to)..to]
        .iter()
        .map(|c| if c.is_whitespace() { ' ' } else { *c })
        .collect();
    if from > 0 {
        snippet.insert(0, '…');
    }
    if to < chars.len() {
        snippet.push('…');
    }
    SearchHit {
        page,
        index,
        start,
        end,
        rects: layer.range_rects(start, end),
        snippet,
        word,
    }
}

fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The text of the pages, kept per page for the revision of its text.
#[derive(Default)]
pub(crate) struct TextCache {
    pages: Vec<Option<(u64, Arc<PageText>)>>,
    /// Every page together, for a revision of the document.
    all: Option<(u64, Arc<Vec<PageText>>)>,
}

impl Document {
    /// Whether the text kept for `page` is that of its current text.
    fn text_kept(&self, cache: &TextCache, page: usize) -> Option<Arc<PageText>> {
        match cache.pages.get(page) {
            Some(Some((rev, text))) if *rev == self.text_revision(page) => Some(text.clone()),
            _ => None,
        }
    }

    /// Positioned text of every page. The text of a page is extracted once
    /// for each change of its text: a change of comments only, or of other
    /// pages, keeps it.
    pub fn text_layers(&self) -> Arc<Vec<PageText>> {
        let mut cache = self.text_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((rev, layers)) = cache.all.as_ref() {
            if *rev == self.revision() {
                return layers.clone();
            }
        }
        let n = self.page_count();
        cache.pages.resize(n, None);
        let missing: Vec<usize> = (0..n).filter(|p| self.text_kept(&cache, *p).is_none()).collect();
        if !missing.is_empty() {
            for (p, text) in crate::textlayer::extract_pages(self.bytes(), &missing).unwrap_or_default() {
                cache.pages[p] = Some((self.text_revision(p), Arc::new(text)));
            }
        }
        let all: Vec<PageText> = cache
            .pages
            .iter()
            .map(|e| e.as_ref().map(|(_, t)| (**t).clone()).unwrap_or_default())
            .collect();
        let all = Arc::new(all);
        cache.all = Some((self.revision(), all.clone()));
        all
    }

    /// Positioned text of one page, extracted alone when it is not kept (the
    /// first text selection does not extract every page).
    pub fn page_text_layer(&self, page: usize) -> Option<Arc<PageText>> {
        let mut cache = self.text_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(text) = self.text_kept(&cache, page) {
            return Some(text);
        }
        let n = self.page_count();
        if page >= n {
            return None;
        }
        cache.pages.resize(n, None);
        let (_, text) = crate::textlayer::extract_pages(self.bytes(), &[page])
            .ok()?
            .pop()?;
        let text = Arc::new(text);
        cache.pages[page] = Some((self.text_revision(page), text.clone()));
        Some(text)
    }

    /// After an edit of the text of one page: its text is extracted again at
    /// once (the others are kept by themselves).
    pub(crate) fn refresh_text_layer(&self, page: usize, _before: u64) {
        let _ = self.page_text_layer(page);
    }

    /// Positioned text of one page.
    pub fn text_layer(&self, page: usize) -> Option<PageText> {
        self.text_layers().get(page).cloned()
    }

    /// Plain text of a page (whitespace normalised). Empty when the page has no text.
    pub fn page_text(&self, page: usize) -> String {
        self.text_layers()
            .get(page)
            .map(|l| normalize(&l.text))
            .unwrap_or_default()
    }

    /// Text of every page.
    pub fn all_text(&self) -> Vec<String> {
        self.text_layers().iter().map(|l| normalize(&l.text)).collect()
    }

    /// Find `query` on every page.
    pub fn search(&self, query: &str, opts: SearchOptions) -> Vec<SearchHit> {
        let mut hits = Vec::new();
        for (page, layer) in self.text_layers().iter().enumerate() {
            let chars: Vec<char> = layer.text.chars().collect();
            for (index, (start, end)) in layer.find(query, opts).into_iter().enumerate() {
                hits.push(make_hit(page, index, layer, &chars, start, end, 0));
            }
        }
        hits
    }

    /// `(words, characters)` over the whole document.
    pub fn word_count(&self) -> (usize, usize) {
        let mut words = 0;
        let mut chars = 0;
        for text in self.all_text() {
            words += text.split_whitespace().count();
            chars += text.chars().filter(|c| !c.is_whitespace()).count();
        }
        (words, chars)
    }

    /// Pages whose text contains no word characters (candidates for OCR).
    pub fn pages_without_text(&self) -> Vec<usize> {
        self.all_text()
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.chars().any(is_word_char))
            .map(|(i, _)| i)
            .collect()
    }
}
