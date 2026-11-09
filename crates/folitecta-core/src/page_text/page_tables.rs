//! Tables found in the text of the pages (FOL-L030, FOL-L009): rows are the
//! glyphs that share a line across the page, whatever order they are drawn
//! in; the cells of a row are the runs of glyphs apart by more than a word
//! space; the columns of a table are the gaps that the rows leave empty one
//! under the other. Two columns of running text side by side are not a
//! table, nor are lines that agree on no gap.

use std::ops::Range;

use crate::document::Document;
use crate::textlayer::PageText;

/// A table found on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundTable {
    pub page: usize,
    /// Its area (display space, y down: `[x0, y0, x1, y1]`).
    pub bbox: [f32; 4],
    /// Its rows, each with a cell for every column (empty where the row has
    /// nothing).
    pub rows: Vec<Vec<String>>,
}

impl FoundTable {
    pub fn columns(&self) -> usize {
        self.rows.first().map(Vec::len).unwrap_or(0)
    }
}

/// A gap wider than this many ems parts two cells.
const CELL_GAP: f32 = 0.6;
/// A gap wider than this many ems is a space between words.
const WORD_GAP: f32 = 0.22;

/// A run of glyphs of a row: a cell, or part of one.
#[derive(Debug, Clone)]
struct Run {
    x0: f32,
    x1: f32,
    text: String,
}

/// The glyphs on one line across the page.
#[derive(Debug, Clone)]
struct Row {
    y0: f32,
    y1: f32,
    size: f32,
    runs: Vec<Run>,
}

impl Row {
    fn middle(&self) -> f32 {
        (self.y0 + self.y1) / 2.0
    }
}

/// The rows of the upright text of a page, from the top.
fn rows_of(text: &PageText) -> Vec<Row> {
    struct G<'a> {
        x0: f32,
        x1: f32,
        y0: f32,
        y1: f32,
        size: f32,
        text: &'a str,
    }
    let mut glyphs: Vec<G> = text
        .glyphs
        .iter()
        .filter(|g| {
            let q = g.quad;
            // Upright: the top edge level and running to the right.
            g.size > 0.0 && !g.text.trim().is_empty() && (q[3] - q[1]).abs() < 0.1 * g.size && q[2] > q[0]
        })
        .map(|g| G {
            x0: g.bbox[0],
            x1: g.bbox[2],
            y0: g.bbox[1],
            y1: g.bbox[3],
            size: g.size,
            text: g.text.as_str(),
        })
        .collect();
    glyphs.sort_by(|a, b| (a.y0 + a.y1).total_cmp(&(b.y0 + b.y1)));
    // Lines: glyphs whose middles are close to the middle of the line.
    let mut lines: Vec<Vec<G>> = Vec::new();
    for g in glyphs {
        let mid = (g.y0 + g.y1) / 2.0;
        let joins = lines.last().is_some_and(|l| {
            let (y0, y1) = l
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), g| (a.min(g.y0), b.max(g.y1)));
            let h = (y1 - y0).max(g.y1 - g.y0);
            (mid - (y0 + y1) / 2.0).abs() <= 0.35 * h
        });
        if joins {
            if let Some(l) = lines.last_mut() {
                l.push(g);
            }
        } else {
            lines.push(vec![g]);
        }
    }
    lines
        .into_iter()
        .map(|mut l| {
            l.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            let mut runs: Vec<Run> = Vec::new();
            let mut prev: Option<(f32, f32)> = None;
            for g in &l {
                let gap = prev.map(|(x1, size)| (g.x0 - x1, size.max(g.size)));
                match (gap, runs.last_mut()) {
                    (Some((gap, size)), Some(run)) if gap <= CELL_GAP * size => {
                        if gap > WORD_GAP * size {
                            run.text.push(' ');
                        }
                        run.text.push_str(g.text);
                        run.x1 = run.x1.max(g.x1);
                    }
                    _ => runs.push(Run {
                        x0: g.x0,
                        x1: g.x1,
                        text: g.text.to_string(),
                    }),
                }
                prev = Some((prev.map_or(g.x1, |(x1, _)| x1.max(g.x1)), g.size));
            }
            let mut sizes: Vec<f32> = l.iter().map(|g| g.size).collect();
            sizes.sort_by(f32::total_cmp);
            Row {
                y0: l.iter().map(|g| g.y0).fold(f32::MAX, f32::min),
                y1: l.iter().map(|g| g.y1).fold(f32::MIN, f32::max),
                size: sizes[sizes.len() / 2],
                runs,
            }
        })
        .collect()
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

/// The gaps between the columns of `rows` (left and right edges), those
/// at most `allowed` rows run across.
fn column_gaps(rows: &[&Row], allowed: usize, min_width: f32) -> Vec<(f32, f32)> {
    let mut edges: Vec<(f32, i32)> = Vec::new();
    for r in rows {
        for run in &r.runs {
            edges.push((run.x0, 1));
            edges.push((run.x1, -1));
        }
    }
    // Openings before closings at the same place: touching runs leave no gap.
    edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
    let (Some(first), Some(last)) = (
        edges.first().map(|e| e.0),
        edges.iter().map(|e| e.0).reduce(f32::max),
    ) else {
        return Vec::new();
    };
    let mut gaps = Vec::new();
    let mut depth = 0i32;
    let mut open: Option<f32> = None;
    for (x, d) in edges {
        let before = depth;
        depth += d;
        let free = |k: i32| k as usize <= allowed;
        if !free(before) && free(depth) {
            open = Some(x);
        } else if free(before) && !free(depth) {
            if let Some(a) = open.take() {
                if a > first && x < last && x - a >= min_width {
                    gaps.push((a, x));
                }
            }
        }
    }
    gaps
}

/// The table made of `rows` (consecutive rows of a page), when they are one.
/// The table made of `rows`, and of the rows of one cell after them that
/// go on its last row (`after`: words wrapped in a cell, not in its first
/// column, close under the row above).
fn table_of(rows: &[&Row], after: &[&Row]) -> Option<([f32; 4], Vec<Vec<String>>)> {
    let size = median(rows.iter().map(|r| r.size).collect());
    let many: Vec<&Row> = rows.iter().copied().filter(|r| r.runs.len() >= 2).collect();
    let gaps = column_gaps(&many, many.len() / 6, 0.4 * size);
    if gaps.is_empty() {
        return None;
    }
    let bounds: Vec<f32> = gaps.iter().map(|(a, b)| (a + b) / 2.0).collect();
    let columns = bounds.len() + 1;
    let column = |x: f32| bounds.iter().filter(|b| **b < x).count();
    let mut rows: Vec<&Row> = rows.to_vec();
    for r in after {
        let last = rows[rows.len() - 1];
        let close = r.middle() - last.middle() <= 1.6 * r.size.max(last.size);
        let inside = r.runs.iter().all(|run| column(run.x0 + 0.1) > 0);
        if !(close && inside) {
            break;
        }
        rows.push(r);
    }
    let rows = rows.as_slice();
    let mut cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut cells = vec![String::new(); columns];
            for run in &r.runs {
                let c = &mut cells[column(run.x0 + 0.1)];
                if !c.is_empty() {
                    c.push(' ');
                }
                c.push_str(run.text.trim());
            }
            cells
        })
        .collect();
    // A row closer to the one above than rows usually are, with nothing in
    // the first column, goes on the cells of that row (words wrapped in it).
    let steps: Vec<f32> = rows.windows(2).map(|w| w[1].middle() - w[0].middle()).collect();
    let step = median(steps.clone());
    let first_filled = cells.iter().filter(|c| !c[0].is_empty()).count();
    if first_filled * 5 >= cells.len() * 3 {
        let mut k = cells.len();
        while k > 1 {
            k -= 1;
            if cells[k][0].is_empty() && steps[k - 1] < 0.75 * step {
                let row = cells.remove(k);
                for (c, text) in row.into_iter().enumerate() {
                    if !text.is_empty() {
                        let above = &mut cells[k - 1][c];
                        if !above.is_empty() {
                            above.push(' ');
                        }
                        above.push_str(&text);
                    }
                }
            }
        }
    }
    let full = cells
        .iter()
        .filter(|r| r.iter().filter(|c| !c.is_empty()).count() >= 2)
        .count();
    if full < 3 && !(full >= 2 && columns >= 3) {
        return None;
    }
    // Columns of running text side by side are not a table.
    let prose = (0..columns).all(|c| {
        let filled: Vec<usize> = cells
            .iter()
            .map(|r| r[c].chars().count())
            .filter(|n| *n > 0)
            .collect();
        !filled.is_empty() && filled.iter().sum::<usize>() > 30 * filled.len()
    });
    if prose {
        return None;
    }
    let bbox = rows
        .iter()
        .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, r| {
            let x0 = r.runs.iter().map(|x| x.x0).fold(f32::MAX, f32::min);
            let x1 = r.runs.iter().map(|x| x.x1).fold(f32::MIN, f32::max);
            [b[0].min(x0), b[1].min(r.y0), b[2].max(x1), b[3].max(r.y1)]
        });
    Some((bbox, cells))
}

/// The tables of a page's text: their areas and cells, from the top.
pub fn find_tables(text: &PageText) -> Vec<([f32; 4], Vec<Vec<String>>)> {
    let rows = rows_of(text);
    let mut out = Vec::new();
    // Runs of rows with two cells or more, one close under the other; rows
    // of one cell between them belong to them.
    let mut run: Vec<&Row> = Vec::new();
    let mut pending: Vec<&Row> = Vec::new();
    let close = |run: &mut Vec<&Row>, pending: &mut Vec<&Row>, out: &mut Vec<_>| {
        let after = std::mem::take(pending);
        if run.len() >= 2 {
            if let Some(t) = table_of(run, &after) {
                out.push(t);
            }
        }
        run.clear();
    };
    for row in &rows {
        let last = pending.last().or(run.last());
        let near = last.is_some_and(|l| row.middle() - l.middle() <= 2.8 * l.size.max(row.size));
        if !near {
            close(&mut run, &mut pending, &mut out);
        }
        if row.runs.len() >= 2 {
            run.append(&mut pending);
            run.push(row);
        } else if !run.is_empty() {
            pending.push(row);
        }
    }
    close(&mut run, &mut pending, &mut out);
    out
}

impl Document {
    /// The tables found in the text of `pages`, page by page from the top;
    /// the values of form fields count as text where they are shown (read
    /// from a copy whose fields are flattened).
    pub fn page_tables(&self, pages: Range<usize>) -> Vec<FoundTable> {
        let has_fields = pages.clone().any(|p| {
            self.annotations(p)
                .is_ok_and(|a| a.iter().any(|x| x.subtype == "Widget"))
        });
        if has_fields {
            let list: Vec<usize> = pages.clone().collect();
            let flat = self.duplicate().and_then(|mut copy| {
                copy.flatten(&list, crate::flatten::FlattenScope::FormFields)?;
                Ok(copy)
            });
            if let Ok(flat) = flat {
                return flat.tables_of_text(pages);
            }
        }
        self.tables_of_text(pages)
    }

    fn tables_of_text(&self, pages: Range<usize>) -> Vec<FoundTable> {
        let layers = self.text_layers();
        pages
            .filter_map(|p| layers.get(p).map(|l| (p, l)))
            .flat_map(|(page, layer)| {
                find_tables(layer)
                    .into_iter()
                    .map(move |(bbox, rows)| FoundTable { page, bbox, rows })
            })
            .collect()
    }
}
