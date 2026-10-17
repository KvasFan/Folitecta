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

/// Edit script between two sequences (Myers' algorithm): pairs of equal
/// positions, in order.
fn common_pairs(a: &[u64], b: &[u64]) -> Vec<(usize, usize)> {
    // Equal ends are matched directly, which keeps the search small.
    let mut prefix = 0;
    while prefix < a.len() && prefix < b.len() && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < a.len() - prefix
        && suffix < b.len() - prefix
        && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let (ma, mb) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    let mut pairs: Vec<(usize, usize)> = (0..prefix).map(|i| (i, i)).collect();
    for (i, j) in middle_pairs(ma, mb) {
        pairs.push((i + prefix, j + prefix));
    }
    for k in 0..suffix {
        pairs.push((a.len() - suffix + k, b.len() - suffix + k));
    }
    pairs
}

/// Most differences searched for in one piece; beyond, the texts are cut
/// into aligned chunks compared one by one.
const MAX_EDITS: usize = 3000;
const CHUNK: usize = 1500;

fn middle_pairs(a: &[u64], b: &[u64]) -> Vec<(usize, usize)> {
    if let Some(p) = myers(a, b, MAX_EDITS) {
        return p;
    }
    let chunks = (a.len().max(b.len()) / CHUNK).max(1);
    let mut out = Vec::new();
    for c in 0..chunks {
        let (a0, a1) = (a.len() * c / chunks, a.len() * (c + 1) / chunks);
        let (b0, b1) = (b.len() * c / chunks, b.len() * (c + 1) / chunks);
        if let Some(p) = myers(&a[a0..a1], &b[b0..b1], MAX_EDITS) {
            out.extend(p.into_iter().map(|(i, j)| (i + a0, j + b0)));
        }
    }
    out
}

/// Pairs of equal positions (Myers' algorithm), or `None` when more than
/// `limit` edits are needed. Only the reachable diagonals of each step are
/// kept, so the memory grows with the square of the edits, not the length.
fn myers(a: &[u64], b: &[u64], limit: usize) -> Option<Vec<(usize, usize)>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    if n == 0 || m == 0 {
        return Some(Vec::new());
    }
    let max = (n + m) as usize;
    let limit = limit.min(max) as isize;
    // v[k + d] for the diagonals -d..=d of the current step.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut prev: Vec<isize> = vec![0; 3];
    for d in 0..=limit {
        let mut cur = vec![0isize; (2 * d + 1) as usize];
        // x on diagonal k after step d - 1 (offset by d - 1).
        let get = |v: &Vec<isize>, k: isize| -> isize {
            let i = k + (d - 1);
            if i < 0 || i as usize >= v.len() {
                -1
            } else {
                v[i as usize]
            }
        };
        let mut k = -d;
        while k <= d {
            let mut x = if d == 0 {
                0
            } else if k == -d || (k != d && get(&prev, k - 1) < get(&prev, k + 1)) {
                get(&prev, k + 1)
            } else {
                get(&prev, k - 1) + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            cur[(k + d) as usize] = x;
            if x >= n && y >= m {
                trace.push(cur);
                return Some(backtrack(&trace, n, m));
            }
            k += 2;
        }
        trace.push(cur.clone());
        prev = cur;
    }
    None
}

fn backtrack(trace: &[Vec<isize>], n: isize, m: isize) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (1..trace.len() as isize).rev() {
        let v = &trace[(d - 1) as usize];
        let get = |k: isize| -> isize {
            let i = k + (d - 1);
            if i < 0 || i as usize >= v.len() {
                -1
            } else {
                v[i as usize]
            }
        };
        let k = x - y;
        let prev_k = if k == -d || (k != d && get(k - 1) < get(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = get(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            pairs.push((x as usize, y as usize));
        }
        x = prev_x;
        y = prev_y;
    }
    // The first snake (step 0) from the origin.
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        pairs.push((x as usize, y as usize));
    }
    pairs.reverse();
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
        // Middle parts only (no common ends).
        assert_eq!(myers(&[1, 2, 3], &[4, 2, 5], 10), Some(vec![(1, 1)]));
        assert_eq!(myers(&[1, 2, 3], &[7, 8, 9], 1), None);
        // Every pair found is a real match and in order.
        let a: Vec<u64> = (0..400).map(|i| (i * 7 % 13) as u64).collect();
        let b: Vec<u64> = (0..380).map(|i| (i * 5 % 13) as u64).collect();
        let p = common_pairs(&a, &b);
        assert!(p.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
        assert!(p.iter().all(|(i, j)| a[*i] == b[*j]));
    }
}
