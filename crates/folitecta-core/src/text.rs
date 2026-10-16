//! Text extraction and search.

use crate::document::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_words: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub page: usize,
    /// Ordinal of the hit within its page.
    pub index: usize,
    /// Character offset of the match inside the page text.
    pub offset: usize,
    pub snippet: String,
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

impl Document {
    /// Plain text of a page (whitespace normalised). Empty when extraction fails.
    pub fn page_text(&self, page: usize) -> String {
        if page >= self.page_count() {
            return String::new();
        }
        match self.inner.extract_text(&[page as u32 + 1]) {
            Ok(t) => normalize(&t),
            Err(_) => String::new(),
        }
    }

    /// Text of every page.
    pub fn all_text(&self) -> Vec<String> {
        (0..self.page_count()).map(|p| self.page_text(p)).collect()
    }

    /// Find `query` on every page.
    pub fn search(&self, query: &str, opts: SearchOptions) -> Vec<SearchHit> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for page in 0..self.page_count() {
            let text = self.page_text(page);
            let (hay, needle) = if opts.case_sensitive {
                (text.clone(), query.to_string())
            } else {
                (text.to_lowercase(), query.to_lowercase())
            };
            let chars: Vec<char> = hay.chars().collect();
            let needle_chars: Vec<char> = needle.chars().collect();
            let orig_chars: Vec<char> = text.chars().collect();
            if needle_chars.is_empty() || chars.len() < needle_chars.len() {
                continue;
            }
            let mut index = 0;
            let mut i = 0;
            while i + needle_chars.len() <= chars.len() {
                if chars[i..i + needle_chars.len()] == needle_chars[..] {
                    let end = i + needle_chars.len();
                    let boundary_ok = !opts.whole_words
                        || ((i == 0 || !is_word_char(chars[i - 1]))
                            && (end >= chars.len() || !is_word_char(chars[end])));
                    if boundary_ok {
                        let start = i.saturating_sub(40);
                        let stop = (end + 40).min(orig_chars.len());
                        let mut snippet: String = orig_chars[start..stop].iter().collect();
                        if start > 0 {
                            snippet.insert(0, '…');
                        }
                        if stop < orig_chars.len() {
                            snippet.push('…');
                        }
                        hits.push(SearchHit {
                            page,
                            index,
                            offset: i,
                            snippet,
                        });
                        index += 1;
                        i = end;
                        continue;
                    }
                }
                i += 1;
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

    /// Pages whose extracted text contains no word characters (candidates for OCR).
    pub fn pages_without_text(&self) -> Vec<usize> {
        (0..self.page_count())
            .filter(|p| !self.page_text(*p).chars().any(is_word_char))
            .collect()
    }
}
