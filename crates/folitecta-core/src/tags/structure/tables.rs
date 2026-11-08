//! Tables of the tags made readable (FOL-L041): header cells for the tables
//! that have none, the columns a cell spans in the rows that have fewer
//! cells than the others, and the rows a cell spans down when a row below
//! it starts further right.

use lopdf::{Dictionary, Object, ObjectId, dictionary};

use super::StructElem;
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::deref;

/// The rows of a table, those of its header, body and foot included, in order.
fn rows(table: &StructElem) -> Vec<&StructElem> {
    let mut out = Vec::new();
    for c in table.children() {
        match c.role.as_str() {
            "TR" => out.push(c),
            "THead" | "TBody" | "TFoot" => out.extend(c.children().filter(|r| r.role == "TR")),
            _ => {}
        }
    }
    out
}

/// The cells of a row.
fn cells(row: &StructElem) -> Vec<&StructElem> {
    row.children()
        .filter(|c| matches!(c.role.as_str(), "TH" | "TD"))
        .collect()
}

/// Whether a cell of the table spans rows or columns already.
fn has_spans(table: &StructElem) -> bool {
    let mut spans = false;
    table.walk(&mut |d, _| {
        spans |= d
            .attributes
            .iter()
            .any(|a| a.ends_with("/ColSpan") || a.ends_with("/RowSpan"));
    });
    spans
}

/// The columns each cell of a short row spans, `width` columns in all, from
/// the left edges of the cells of the widest row (`columns`) and of the row
/// (`lefts`): a cell starts at the column whose edge is nearest. `None` when
/// the edges do not tell (missing, or two cells in one column).
#[cfg(test)]
fn spans_by_place(columns: &[f32], lefts: &[f32]) -> Option<Vec<usize>> {
    let starts = starts_by_place(columns, lefts)?;
    if starts.first() != Some(&0) {
        return None;
    }
    Some(spans_of(&starts, columns.len()))
}

/// The column each cell of a row starts in, from their left edges: the
/// column whose edge is nearest. `None` when the edges do not tell.
fn starts_by_place(columns: &[f32], lefts: &[f32]) -> Option<Vec<usize>> {
    let nearest = |x: f32| {
        columns
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map(|(i, _)| i)
    };
    let starts: Vec<usize> = lefts.iter().map(|x| nearest(*x)).collect::<Option<_>>()?;
    if starts.is_empty() || starts.windows(2).any(|w| w[0] >= w[1]) {
        return None;
    }
    Some(starts)
}

/// The columns cells starting at `starts` span, up to the next one, the
/// last to the end of the `width` columns.
fn spans_of(starts: &[usize], width: usize) -> Vec<usize> {
    let mut spans: Vec<usize> = starts.windows(2).map(|w| w[1] - w[0]).collect();
    if let Some(last) = starts.last() {
        spans.push(width.saturating_sub(*last).max(1));
    }
    spans
}

/// The columns each cell of a row of `count` cells spans when the page does
/// not tell: one each, the last cell the rest.
fn spans_by_order(count: usize, width: usize) -> Vec<usize> {
    let mut spans = vec![1; count];
    if let Some(last) = spans.last_mut() {
        *last = width + 1 - count;
    }
    spans
}

impl Document {
    /// Set `key` of the table attributes (`/O /Table`) of element `id`,
    /// adding them when the element has none.
    fn set_table_attribute(&mut self, id: ObjectId, key: &str, value: Object) -> Result<()> {
        let owned =
            |d: &Dictionary| d.get(b"O").ok().and_then(|o| o.as_name().ok()) == Some(b"Table".as_slice());
        let attrs = self.inner.get_dictionary(id)?.get(b"A").ok().cloned();
        let mut list = match attrs {
            None => Vec::new(),
            Some(Object::Array(a)) => a,
            Some(o) => vec![o],
        };
        for o in list.iter_mut() {
            match o {
                Object::Dictionary(d) if owned(d) => {
                    d.set(key, value);
                    let a = if list.len() == 1 {
                        list.remove(0)
                    } else {
                        Object::Array(list)
                    };
                    self.inner.get_dictionary_mut(id)?.set("A", a);
                    return Ok(());
                }
                Object::Reference(r) => {
                    let r = *r;
                    if deref(&self.inner, &Object::Reference(r))
                        .as_dict()
                        .is_ok_and(owned)
                    {
                        self.inner.get_dictionary_mut(r)?.set(key, value);
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        list.push(Object::Dictionary(dictionary! { "O" => "Table", key => value }));
        let a = if list.len() == 1 {
            list.remove(0)
        } else {
            Object::Array(list)
        };
        self.inner.get_dictionary_mut(id)?.set("A", a);
        Ok(())
    }

    /// Take `key` away from the table attributes of element `id`.
    fn remove_table_attribute(&mut self, id: ObjectId, key: &[u8]) -> Result<()> {
        let owned =
            |d: &Dictionary| d.get(b"O").ok().and_then(|o| o.as_name().ok()) == Some(b"Table".as_slice());
        let Some(attrs) = self.inner.get_dictionary(id)?.get(b"A").ok().cloned() else {
            return Ok(());
        };
        let mut list = match attrs {
            Object::Array(a) => a,
            o => vec![o],
        };
        let mut refs = Vec::new();
        for o in list.iter_mut() {
            match o {
                Object::Dictionary(d) if owned(d) => {
                    d.remove(key);
                }
                Object::Reference(r) => refs.push(*r),
                _ => {}
            }
        }
        for r in refs {
            if let Ok(d) = self.inner.get_dictionary_mut(r) {
                if owned(d) {
                    d.remove(key);
                }
            }
        }
        let a = if list.len() == 1 {
            list.remove(0)
        } else {
            Object::Array(list)
        };
        self.inner.get_dictionary_mut(id)?.set("A", a);
        Ok(())
    }

    /// The value of `key` among the table attributes of element `id`.
    fn table_attribute(&self, id: ObjectId, key: &[u8]) -> Option<Object> {
        let attrs = self.inner.get_dictionary(id).ok()?.get(b"A").ok()?;
        let list = match deref(&self.inner, attrs) {
            Object::Array(a) => a.clone(),
            o => vec![o.clone()],
        };
        list.iter().find_map(|o| {
            let d = deref(&self.inner, o).as_dict().ok()?;
            (d.get(b"O").ok()?.as_name().ok()? == b"Table").then(|| d.get(key).ok().cloned())?
        })
    }

    /// The tables of the tags with their summaries (empty when they have
    /// none), in the order of the tags.
    pub fn table_summaries(&self) -> Vec<(StructElem, String)> {
        self.tag_tables()
            .unwrap_or_default()
            .into_iter()
            .map(|t| {
                let summary = self
                    .table_attribute(t.id, b"Summary")
                    .and_then(|o| crate::util::text_of(&self.inner, &o))
                    .unwrap_or_default();
                (t, summary)
            })
            .collect()
    }

    /// Give tables their summaries (`/Summary` of the table attributes);
    /// an empty text takes a summary away. One undo step; returns how many
    /// tables changed.
    pub fn set_table_summaries(&mut self, summaries: &[(ObjectId, String)]) -> Result<usize> {
        let now: Vec<(ObjectId, String)> = self
            .table_summaries()
            .into_iter()
            .map(|(t, s)| (t.id, s))
            .collect();
        let changes: Vec<(ObjectId, String)> = summaries
            .iter()
            .filter(|(id, text)| {
                now.iter()
                    .find(|(t, _)| t == id)
                    .is_some_and(|(_, old)| old.trim() != text.trim())
            })
            .cloned()
            .collect();
        if changes.is_empty() {
            return Ok(0);
        }
        self.edit_step(|doc| {
            for (id, text) in &changes {
                if text.trim().is_empty() {
                    doc.remove_table_attribute(*id, b"Summary")?;
                } else {
                    doc.set_table_attribute(*id, "Summary", lopdf::text_string(text.trim()))?;
                }
            }
            Ok(changes.len())
        })
    }

    /// The tables of the tags.
    fn tag_tables(&self) -> Result<Vec<StructElem>> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let mut tables = Vec::new();
        tree.walk(&mut |e, _| {
            if e.role == "Table" {
                tables.push(e.clone());
            }
        });
        Ok(tables)
    }

    /// The tables without header cells get them: the cells of their first
    /// row become header cells (`TH`) of their column. One undo step;
    /// returns how many tables changed.
    pub fn fix_table_headers(&mut self) -> Result<usize> {
        let mut heads: Vec<Vec<ObjectId>> = Vec::new();
        for table in self.tag_tables()? {
            let mut th = false;
            table.walk(&mut |d, _| th |= d.role == "TH");
            if th {
                continue;
            }
            let first: Vec<ObjectId> = rows(&table)
                .first()
                .map(|r| cells(r).iter().map(|c| c.id).collect())
                .unwrap_or_default();
            if !first.is_empty() {
                heads.push(first);
            }
        }
        if heads.is_empty() {
            return Ok(0);
        }
        self.edit_step(|doc| {
            for &id in heads.iter().flatten() {
                doc.inner
                    .get_dictionary_mut(id)?
                    .set("S", Object::Name(b"TH".to_vec()));
                doc.set_table_attribute(id, "Scope", Object::Name(b"Column".to_vec()))?;
            }
            Ok(heads.len())
        })
    }

    /// In the tables without spans whose rows have different numbers of
    /// cells, the cells of the shorter rows span columns (`/ColSpan`): those
    /// their text covers on the page, measured against the cells of the
    /// widest row; when the page does not tell, the last cell of the row
    /// spans the columns left. A short row whose first cell stands in a
    /// later column leaves the columns before it to the cells above them,
    /// which span rows down to it (`/RowSpan`). One undo step; returns how
    /// many tables changed.
    pub fn fix_table_spans(&mut self) -> Result<usize> {
        // The text of a cell, not the lines and fills drawn around it.
        let left = |doc: &Document, e: &StructElem| -> Option<f32> {
            let mut x: Option<f32> = None;
            for (page, mcid) in e.content() {
                let items = doc.marked_content(page);
                for item in items.iter().filter(|i| i.mcid == Some(mcid) && i.has_text) {
                    x = Some(x.map_or(item.rect[0], |x| x.min(item.rect[0])));
                }
            }
            x
        };
        let mut changes: Vec<(ObjectId, usize)> = Vec::new();
        let mut row_spans: Vec<(ObjectId, usize)> = Vec::new();
        let mut tables = 0;
        for table in self.tag_tables()? {
            if has_spans(&table) {
                continue;
            }
            let rows = rows(&table);
            let width = rows.iter().map(|r| cells(r).len()).max().unwrap_or(0);
            if width < 2 || rows.iter().all(|r| cells(r).len() == width) {
                continue;
            }
            let columns: Option<Vec<f32>> = rows
                .iter()
                .find(|r| cells(r).len() == width)
                .and_then(|r| cells(r).iter().map(|c| left(self, c)).collect());
            let mut changed = false;
            // The cell each column belongs to, row after row.
            let mut owner: Vec<Option<ObjectId>> = vec![None; width];
            let mut downs: Vec<(ObjectId, usize)> = Vec::new();
            for row in &rows {
                let row_cells = cells(row);
                if row_cells.is_empty() {
                    continue;
                }
                let lefts: Option<Vec<f32>> = row_cells.iter().map(|c| left(self, c)).collect();
                let placed = columns
                    .as_deref()
                    .zip(lefts.as_deref())
                    .and_then(|(c, l)| starts_by_place(c, l));
                let starts = match placed {
                    Some(s) => s,
                    None if row_cells.len() >= width => (0..row_cells.len()).collect(),
                    None => {
                        // The order alone: one column each, the last the rest.
                        let spans = spans_by_order(row_cells.len(), width);
                        let mut at = 0;
                        spans
                            .iter()
                            .map(|s| {
                                let start = at;
                                at += s;
                                start
                            })
                            .collect()
                    }
                };
                // The columns before the first cell: the cells above span down.
                let mut above: Vec<ObjectId> =
                    owner[..starts[0].min(width)].iter().flatten().copied().collect();
                above.dedup();
                for id in above {
                    match downs.iter_mut().find(|(o, _)| *o == id) {
                        Some((_, n)) => *n += 1,
                        None => downs.push((id, 2)),
                    }
                }
                let spans = spans_of(&starts, width);
                for ((cell, span), start) in row_cells.iter().zip(&spans).zip(&starts) {
                    if *span > 1 && row_cells.len() < width {
                        changes.push((cell.id, *span));
                        changed = true;
                    }
                    for col in owner.iter_mut().skip(*start).take(*span) {
                        *col = Some(cell.id);
                    }
                }
            }
            changed |= !downs.is_empty();
            row_spans.extend(downs);
            tables += usize::from(changed);
        }
        if changes.is_empty() && row_spans.is_empty() {
            return Ok(0);
        }
        self.edit_step(|doc| {
            for (id, span) in &changes {
                doc.set_table_attribute(*id, "ColSpan", Object::Integer(*span as i64))?;
            }
            for (id, span) in &row_spans {
                doc.set_table_attribute(*id, "RowSpan", Object::Integer(*span as i64))?;
            }
            Ok(tables)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_span_to_the_column_of_the_next_cell() {
        let columns = [50.0, 150.0, 250.0, 350.0];
        // A cell over the first two columns, then two of one column.
        assert_eq!(
            spans_by_place(&columns, &[52.0, 251.0, 349.0]),
            Some(vec![2, 1, 1])
        );
        // One cell over the first, one over the other three.
        assert_eq!(spans_by_place(&columns, &[50.0, 160.0]), Some(vec![1, 3]));
        // Two cells in one column, or a row that does not start at the left: no answer.
        assert_eq!(spans_by_place(&columns, &[50.0, 60.0]), None);
        assert_eq!(spans_by_place(&columns, &[150.0, 250.0]), None);
        assert_eq!(spans_by_order(2, 4), vec![1, 3]);
        assert_eq!(spans_by_order(1, 3), vec![3]);
    }
}
