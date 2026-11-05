//! The text of a document as it is read (FOL-L042): the paragraphs of the
//! pages in reading order with their place in the text of their page,
//! headings found by the size of their letters, running heads and feet left
//! out, words broken at the end of a line joined again. The reflow view
//! shows it, reading aloud speaks it.

use std::collections::{HashMap, HashSet};

use crate::document::Document;

/// A paragraph (or heading) of a page.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadBlock {
    pub page: usize,
    /// Its characters in the text of the page (`[start, end)`).
    pub start: usize,
    pub end: usize,
    pub text: String,
    /// 1 to 4 for headings, the largest first; 0 for text.
    pub level: u8,
}

/// The text of characters `a..b` of a page: lines joined with spaces, a word
/// broken by a hyphen at the end of a line joined again.
fn joined(chars: &[char]) -> String {
    joined_map(chars).0
}

/// [`joined`], with the place in `chars` of every character of the text.
fn joined_map(chars: &[char]) -> (String, Vec<usize>) {
    let mut out: Vec<(char, usize)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            while out.last().is_some_and(|(c, _)| *c == ' ') {
                out.pop();
            }
            let after = chars[i + 1..].iter().find(|c| !c.is_whitespace()).copied();
            // A hyphen right after a letter at the end of the line: a word
            // broken (before a small letter) or a compound (before a capital).
            let n = out.len();
            let hyphen = n >= 2
                && out[n - 1].0 == '-'
                && out[n - 2].0.is_alphabetic()
                && after.is_some_and(|a| a.is_alphabetic());
            if hyphen {
                if after.is_some_and(|a| a.is_lowercase()) {
                    out.pop();
                }
                // The next line follows with no space.
                while chars.get(i + 1).is_some_and(|c| c.is_whitespace()) {
                    i += 1;
                }
            } else if out.last().is_some_and(|(c, _)| *c != ' ') {
                out.push((' ', i));
            }
        } else if c.is_whitespace() {
            if out.last().is_some_and(|(c, _)| *c != ' ') {
                out.push((' ', i));
            }
        } else {
            out.push((c, i));
        }
        i += 1;
    }
    while out.last().is_some_and(|(c, _)| *c == ' ') {
        out.pop();
    }
    (
        out.iter().map(|(c, _)| *c).collect(),
        out.iter().map(|(_, i)| *i).collect(),
    )
}

/// Sizes in half points.
fn half_points(size: f32) -> u32 {
    (size * 2.0).round().max(0.0) as u32
}

/// A paragraph's text without its digits, to find the ones repeated on many
/// pages (running heads and feet, page numbers).
fn without_digits(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_ascii_digit())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Document {
    /// The characters of the page text of the word that begins at character
    /// `offset` of the text of `block` (as it is read): `[start, end)`.
    /// `None` when the block's text is not that of its page any more.
    pub fn read_word(&self, block: &ReadBlock, offset: usize) -> Option<(usize, usize)> {
        let layers = self.text_layers();
        let chars: Vec<char> = layers.get(block.page)?.text.chars().collect();
        let (a, b) = (block.start.min(chars.len()), block.end.min(chars.len()));
        let (text, map) = joined_map(&chars[a..b]);
        if text != block.text {
            return None;
        }
        let spoken: Vec<char> = text.chars().collect();
        // From the start of the word the offset falls in to its end.
        let mut first = offset.min(spoken.len().checked_sub(1)?);
        while first > 0 && !spoken[first - 1].is_whitespace() {
            first -= 1;
        }
        let mut last = first;
        while last + 1 < spoken.len() && !spoken[last + 1].is_whitespace() {
            last += 1;
        }
        if spoken[first].is_whitespace() {
            return None;
        }
        Some((a + map[first], a + map[last] + 1))
    }

    /// The paragraphs of pages `pages` as they are read (see the module).
    pub fn reading_blocks(&self, pages: std::ops::Range<usize>) -> Vec<ReadBlock> {
        let layers = self.text_layers();
        let n = layers.len();
        // Every paragraph of the document, with the size most of its letters have.
        struct Raw {
            page: usize,
            start: usize,
            end: usize,
            text: String,
            size: u32,
            letters: usize,
        }
        let mut raw = Vec::new();
        for (page, layer) in layers.iter().enumerate() {
            let chars: Vec<char> = layer.text.chars().collect();
            for (a, b) in layer.paragraphs() {
                let (a, b) = (a.min(chars.len()), b.min(chars.len()));
                let text = joined(&chars[a..b]);
                if text.is_empty() {
                    continue;
                }
                let mut sizes: HashMap<u32, usize> = HashMap::new();
                for i in a..b {
                    if let Some(g) = layer
                        .char_glyph
                        .get(i)
                        .copied()
                        .flatten()
                        .and_then(|g| layer.glyphs.get(g))
                    {
                        if !g.text.trim().is_empty() {
                            *sizes.entry(half_points(g.size)).or_default() += 1;
                        }
                    }
                }
                let (size, letters) = sizes.into_iter().max_by_key(|(s, k)| (*k, *s)).unwrap_or((0, 0));
                raw.push(Raw {
                    page,
                    start: a,
                    end: b,
                    text,
                    size,
                    letters,
                });
            }
        }
        // The size most letters have; larger short paragraphs are headings.
        let mut weight: HashMap<u32, usize> = HashMap::new();
        for r in &raw {
            *weight.entry(r.size).or_default() += r.letters;
        }
        let body = weight
            .into_iter()
            .max_by_key(|(s, k)| (*k, std::cmp::Reverse(*s)))
            .map(|(s, _)| s)
            .unwrap_or(0);
        let heading = |r: &Raw| {
            body > 0
                && r.size as f32 >= body as f32 * 1.15
                && r.size >= body + 2
                && r.text.split_whitespace().count() <= 16
        };
        let mut sizes: Vec<u32> = raw.iter().filter(|r| heading(r)).map(|r| r.size).collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        sizes.dedup();
        // Short paragraphs on three pages and a quarter of them are running heads.
        let mut on_pages: HashMap<String, HashSet<usize>> = HashMap::new();
        for r in raw.iter().filter(|r| r.text.chars().count() <= 120) {
            on_pages
                .entry(without_digits(&r.text))
                .or_default()
                .insert(r.page);
        }
        let running = |r: &Raw| {
            r.text.chars().count() <= 120
                && on_pages
                    .get(&without_digits(&r.text))
                    .is_some_and(|p| p.len() >= 3 && p.len() * 4 >= n)
        };
        // A page number alone is not read either.
        let number = |r: &Raw| {
            r.text
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_whitespace() || "-–—.".contains(c))
        };
        raw.into_iter()
            .filter(|r| pages.contains(&r.page) && !running(r) && !number(r))
            .map(|r| {
                let level = if heading(&r) {
                    (sizes.iter().position(|s| *s == r.size).unwrap_or(0) + 1).min(4) as u8
                } else {
                    0
                };
                ReadBlock {
                    page: r.page,
                    start: r.start,
                    end: r.end,
                    text: r.text,
                    level,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn lines_are_joined_and_broken_words_mended() {
        assert_eq!(joined(&chars("A line\nand the next")), "A line and the next");
        assert_eq!(joined(&chars("an exam-\nple of it")), "an example of it");
        // A dash between words and a hyphen before a capital stay.
        assert_eq!(joined(&chars("well -\nknown")), "well - known");
        assert_eq!(joined(&chars("Rhein-\nMain")), "Rhein-Main");
        assert_eq!(without_digits("Page 12 of 40"), without_digits("page 3 of 40"));
    }

    #[test]
    fn every_character_read_has_its_place_on_the_page() {
        let source = chars("an exam-\nple  of it");
        let (text, map) = joined_map(&source);
        assert_eq!(text, "an example of it");
        assert_eq!(map.len(), text.chars().count());
        // "ple" comes from the second line, "of" after two spaces.
        assert_eq!(source[map[7]], 'p');
        assert_eq!(map[7], 9);
        assert_eq!(source[map[11]], 'o');
        assert_eq!(map[11], 14);
    }
}
