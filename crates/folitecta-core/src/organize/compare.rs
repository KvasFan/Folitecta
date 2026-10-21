//! Comparing the text of two documents word by word: what was inserted,
//! deleted or changed, with its place on the pages of both documents.

use crate::document::Document;

/// Kind of a difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Inserted,
    Deleted,
    Changed,
}

impl DiffKind {
    pub fn name(&self) -> &'static str {
        match self {
            DiffKind::Inserted => "inserted",
            DiffKind::Deleted => "deleted",
            DiffKind::Changed => "changed",
        }
    }
}

/// A run of words of one page: page, character range in its text layer.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSpan {
    pub page: usize,
    pub start: usize,
    pub end: usize,
    /// Rectangles of the words (display space).
    pub rects: Vec<[f32; 4]>,
}

/// One difference between the documents.
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    pub kind: DiffKind,
    /// Where the old words are (deleted or changed), per page.
    pub old: Vec<PageSpan>,
    /// Where the new words are (inserted or changed), per page.
    pub new: Vec<PageSpan>,
    pub old_text: String,
    pub new_text: String,
    /// Page of the old document near the difference (also for insertions).
    pub old_page: usize,
    /// Page of the new document near the difference (also for deletions).
    pub new_page: usize,
}

/// Result of a comparison.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Comparison {
    pub differences: Vec<Difference>,
    pub old_pages: usize,
    pub new_pages: usize,
    pub old_words: usize,
    pub new_words: usize,
    /// For every page of the new document, the page of the old one to show
    /// beside it: where its first word that both documents share is (pages
    /// without one follow the page before them).
    pub page_map: Vec<usize>,
}

impl Comparison {
    pub fn count(&self, kind: DiffKind) -> usize {
        self.differences.iter().filter(|d| d.kind == kind).count()
    }

    /// Pages of the new document with differences.
    pub fn new_pages_changed(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.differences.iter().map(|d| d.new_page).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// A word of a document: page, character range, folded text.
#[derive(Debug, Clone)]
struct Word {
    page: usize,
    start: usize,
    end: usize,
    text: String,
}

fn words_of(doc: &Document) -> Vec<Word> {
    let mut out = Vec::new();
    for (page, layer) in doc.text_layers().iter().enumerate() {
        let chars: Vec<char> = layer.text.chars().collect();
        for (s, e) in layer.words() {
            let text: String = chars[s..e].iter().collect();
            out.push(Word {
                page,
                start: s,
                end: e,
                text,
            });
        }
    }
    out
}

/// Most time spent looking for the smallest edit script; past it the diff
/// is completed with coarser steps.
const DIFF_DEADLINE: std::time::Duration = std::time::Duration::from_secs(4);

/// Pairs of equal positions of two sequences, in order (Myers' algorithm in
/// linear space).
fn common_pairs(a: &[u64], b: &[u64]) -> Vec<(usize, usize)> {
    let deadline = std::time::Instant::now() + DIFF_DEADLINE;
    let ops = similar::capture_diff_slices_deadline(similar::Algorithm::Myers, a, b, Some(deadline));
    let mut pairs = Vec::new();
    for op in ops {
        if let similar::DiffOp::Equal {
            old_index,
            new_index,
            len,
        } = op
        {
            pairs.extend((0..len).map(|k| (old_index + k, new_index + k)));
        }
    }
    pairs
}
fn hash(s: &str, fold: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if fold {
        s.to_lowercase().hash(&mut h);
    } else {
        s.hash(&mut h);
    }
    h.finish()
}

/// Spans of consecutive words, split by page.
fn spans(doc: &Document, words: &[Word]) -> Vec<PageSpan> {
    let layers = doc.text_layers();
    let mut out: Vec<PageSpan> = Vec::new();
    for w in words {
        match out.last_mut() {
            Some(s) if s.page == w.page => s.end = w.end,
            _ => out.push(PageSpan {
                page: w.page,
                start: w.start,
                end: w.end,
                rects: Vec::new(),
            }),
        }
    }
    for s in &mut out {
        if let Some(l) = layers.get(s.page) {
            s.rects = l.range_rects(s.start, s.end);
        }
    }
    out
}

/// The old page beside every new page, from the pages of the words both
/// documents share (new page, old page, in order).
fn page_map(shared: &[(usize, usize)], new_pages: usize, old_pages: usize) -> Vec<usize> {
    let mut first: Vec<Option<usize>> = vec![None; new_pages];
    for &(np, op) in shared {
        if let Some(slot) = first.get_mut(np) {
            slot.get_or_insert(op);
        }
    }
    let last = old_pages.saturating_sub(1);
    let mut prev: Option<usize> = None;
    first
        .into_iter()
        .enumerate()
        .map(|(p, m)| {
            let v = m.unwrap_or_else(|| prev.map(|q| q + 1).unwrap_or(p)).min(last);
            prev = Some(v);
            v
        })
        .collect()
}

impl Document {
    /// Compare the words of this (old) document with `new`. `ignore_case`
    /// treats "Word" and "word" as equal.
    pub fn compare_with(&self, new: &Document, ignore_case: bool) -> Comparison {
        let (wa, wb) = (words_of(self), words_of(new));
        let ha: Vec<u64> = wa.iter().map(|w| hash(&w.text, ignore_case)).collect();
        let hb: Vec<u64> = wb.iter().map(|w| hash(&w.text, ignore_case)).collect();
        let pairs = common_pairs(&ha, &hb);
        let mut diffs = Vec::new();
        let (mut i, mut j) = (0usize, 0usize);
        let mut emit = |i0: usize, i1: usize, j0: usize, j1: usize| {
            if i0 == i1 && j0 == j1 {
                return;
            }
            let kind = match (i1 > i0, j1 > j0) {
                (true, true) => DiffKind::Changed,
                (true, false) => DiffKind::Deleted,
                _ => DiffKind::Inserted,
            };
            let ow = &wa[i0..i1];
            let nw = &wb[j0..j1];
            let near = |words: &[Word], at: usize| {
                words
                    .get(at)
                    .or_else(|| words.get(at.saturating_sub(1)))
                    .map(|w| w.page)
                    .unwrap_or(0)
            };
            diffs.push(Difference {
                kind,
                old: spans(self, ow),
                new: spans(new, nw),
                old_text: ow.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "),
                new_text: nw.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "),
                old_page: near(&wa, i0),
                new_page: near(&wb, j0),
            });
        };
        let pages: Vec<(usize, usize)> = pairs.iter().map(|&(pi, pj)| (wb[pj].page, wa[pi].page)).collect();
        for (pi, pj) in pairs {
            emit(i, pi, j, pj);
            i = pi + 1;
            j = pj + 1;
        }
        emit(i, wa.len(), j, wb.len());
        Comparison {
            differences: diffs,
            old_pages: self.page_count(),
            new_pages: new.page_count(),
            old_words: wa.len(),
            new_words: wb.len(),
            page_map: page_map(&pages, new.page_count(), self.page_count()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_scripts_keep_common_words() {
        let a = [1u64, 2, 3, 4, 5, 6];
        let b = [1u64, 2, 9, 4, 6, 7];
        let p = common_pairs(&a, &b);
        assert_eq!(p, vec![(0, 0), (1, 1), (3, 3), (5, 4)]);
        assert!(common_pairs(&[], &[1]).is_empty());
        assert_eq!(common_pairs(&[1, 2], &[1, 2]), vec![(0, 0), (1, 1)]);
        assert_eq!(common_pairs(&[1, 2, 3], &[4, 2, 5]), vec![(1, 1)]);
        assert!(common_pairs(&[1, 2, 3], &[7, 8, 9]).is_empty());
        // Every pair found is a real match and in order.
        let a: Vec<u64> = (0..400).map(|i| (i * 7 % 13) as u64).collect();
        let b: Vec<u64> = (0..380).map(|i| (i * 5 % 13) as u64).collect();
        let p = common_pairs(&a, &b);
        assert!(p.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
        assert!(p.iter().all(|(i, j)| a[*i] == b[*j]));
    }

    #[test]
    fn pages_follow_the_words_they_share() {
        // New page 1 is inserted (no shared word), new page 2 holds old page 1.
        let shared = [(0, 0), (0, 0), (2, 1), (3, 2), (3, 3)];
        assert_eq!(page_map(&shared, 5, 4), vec![0, 1, 1, 2, 3]);
        // No shared word at all: page beside page, within the old document.
        assert_eq!(page_map(&[], 3, 2), vec![0, 1, 1]);
        assert!(page_map(&[], 0, 2).is_empty());
    }

    #[test]
    fn long_and_very_different_texts_finish() {
        let a: Vec<u64> = (0..40_000u64).map(|i| i % 997).collect();
        let b: Vec<u64> = (0..40_000u64).map(|i| (i * 3 + i / 7) % 991).collect();
        let start = std::time::Instant::now();
        let p = common_pairs(&a, &b);
        assert!(start.elapsed() < DIFF_DEADLINE + std::time::Duration::from_secs(4));
        assert!(!p.is_empty());
        assert!(p.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
        assert!(p.iter().all(|(i, j)| a[*i] == b[*j]));
    }
}
