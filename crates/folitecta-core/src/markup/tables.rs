//! Tables drawn on pages: rows and columns of text cells with lines, fills
//! and merged cells. A table stays editable as a comment holding its data
//! until it is flattened into the page.

use lopdf::{Object, ObjectId, dictionary};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::fonts::{GlyphProblem, TextWriter, previous_font};
use crate::util::fmt_num;

/// Name of the stamp comments holding tables.
pub const TABLE_STAMP: &str = "FolitectaTable";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableCell {
    pub text: String,
    pub fill: Option<[f32; 3]>,
    /// Rows and columns covered (1 for a plain cell).
    pub rowspan: usize,
    pub colspan: usize,
    pub bold: bool,
    /// `left`, `center` or `right`.
    pub align: String,
}

impl Default for TableCell {
    fn default() -> Self {
        TableCell {
            text: String::new(),
            fill: None,
            rowspan: 1,
            colspan: 1,
            bold: false,
            align: "left".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Table {
    /// Relative heights of the rows and widths of the columns.
    pub rows: Vec<f32>,
    pub cols: Vec<f32>,
    /// `cells[row][col]`; cells covered by a merged cell are kept but not drawn.
    pub cells: Vec<Vec<TableCell>>,
    pub line_color: [f32; 3],
    pub line_width: f32,
    pub text_color: [f32; 3],
    pub font_size: f32,
}

impl Default for Table {
    fn default() -> Self {
        Table::new(3, 3)
    }
}

impl Table {
    pub fn new(rows: usize, cols: usize) -> Table {
        let (rows, cols) = (rows.clamp(1, 200), cols.clamp(1, 50));
        Table {
            rows: vec![1.0; rows],
            cols: vec![1.0; cols],
            cells: vec![vec![TableCell::default(); cols]; rows],
            line_color: [0.2, 0.2, 0.2],
            line_width: 0.8,
            text_color: [0.0, 0.0, 0.0],
            font_size: 10.0,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn col_count(&self) -> usize {
        self.cols.len()
    }

    /// The merged cell covering `(r, c)`: its top-left corner.
    pub fn owner(&self, r: usize, c: usize) -> (usize, usize) {
        for (rr, row) in self.cells.iter().enumerate().take(r + 1) {
            for (cc, cell) in row.iter().enumerate().take(c + 1) {
                if rr + cell.rowspan > r && cc + cell.colspan > c && (cell.rowspan > 1 || cell.colspan > 1) {
                    return (rr, cc);
                }
            }
        }
        (r, c)
    }

    fn check(&self, r: usize, c: usize) -> Result<()> {
        if r >= self.row_count() || c >= self.col_count() {
            return Err(Error::Invalid("the cell is outside the table".into()));
        }
        Ok(())
    }

    /// Insert a row before `at` (at the end when `at` is the row count).
    pub fn insert_row(&mut self, at: usize) {
        let at = at.min(self.row_count());
        let h = self.rows.get(at).or(self.rows.last()).copied().unwrap_or(1.0);
        self.rows.insert(at, h);
        self.cells
            .insert(at, vec![TableCell::default(); self.col_count()]);
        // Merged cells across the new row grow.
        for r in 0..at {
            for cell in &mut self.cells[r] {
                if cell.rowspan > 1 && r + cell.rowspan > at {
                    cell.rowspan += 1;
                }
            }
        }
    }

    pub fn insert_col(&mut self, at: usize) {
        let at = at.min(self.col_count());
        let w = self.cols.get(at).or(self.cols.last()).copied().unwrap_or(1.0);
        self.cols.insert(at, w);
        for row in &mut self.cells {
            for (c, cell) in row.iter_mut().enumerate().take(at) {
                if cell.colspan > 1 && c + cell.colspan > at {
                    cell.colspan += 1;
                }
            }
            row.insert(at, TableCell::default());
        }
    }

    /// Delete rows `from..=to` (a table keeps at least one row).
    pub fn delete_rows(&mut self, from: usize, to: usize) -> Result<()> {
        let to = to.min(self.row_count().saturating_sub(1));
        if from > to || to - from + 1 >= self.row_count() {
            return Err(Error::Invalid("a table keeps at least one row".into()));
        }
        self.unmerge_all_touching(from, to, 0, self.col_count() - 1);
        self.rows.drain(from..=to);
        self.cells.drain(from..=to);
        Ok(())
    }

    pub fn delete_cols(&mut self, from: usize, to: usize) -> Result<()> {
        let to = to.min(self.col_count().saturating_sub(1));
        if from > to || to - from + 1 >= self.col_count() {
            return Err(Error::Invalid("a table keeps at least one column".into()));
        }
        self.unmerge_all_touching(0, self.row_count() - 1, from, to);
        self.cols.drain(from..=to);
        for row in &mut self.cells {
            row.drain(from..=to);
        }
        Ok(())
    }

    /// Split every merged cell that overlaps the given range.
    fn unmerge_all_touching(&mut self, r0: usize, r1: usize, c0: usize, c1: usize) {
        for r in 0..self.row_count() {
            for c in 0..self.col_count() {
                let cell = &self.cells[r][c];
                let (re, ce) = (r + cell.rowspan - 1, c + cell.colspan - 1);
                if (cell.rowspan > 1 || cell.colspan > 1) && r <= r1 && re >= r0 && c <= c1 && ce >= c0 {
                    self.cells[r][c].rowspan = 1;
                    self.cells[r][c].colspan = 1;
                }
            }
        }
    }

    /// Merge the cells of a rectangle into one (texts are joined).
    pub fn merge(&mut self, r0: usize, c0: usize, r1: usize, c1: usize) -> Result<()> {
        let (r0, r1) = (r0.min(r1), r0.max(r1));
        let (c0, c1) = (c0.min(c1), c0.max(c1));
        self.check(r1, c1)?;
        if r0 == r1 && c0 == c1 {
            return Ok(());
        }
        self.unmerge_all_touching(r0, r1, c0, c1);
        let mut texts = Vec::new();
        for r in r0..=r1 {
            for c in c0..=c1 {
                let t = std::mem::take(&mut self.cells[r][c].text);
                if !t.trim().is_empty() {
                    texts.push(t);
                }
            }
        }
        let cell = &mut self.cells[r0][c0];
        cell.text = texts.join(" ");
        cell.rowspan = r1 - r0 + 1;
        cell.colspan = c1 - c0 + 1;
        Ok(())
    }

    /// Split the merged cell covering `(r, c)` back into plain cells.
    pub fn split(&mut self, r: usize, c: usize) -> Result<bool> {
        self.check(r, c)?;
        let (or, oc) = self.owner(r, c);
        let cell = &mut self.cells[or][oc];
        if cell.rowspan == 1 && cell.colspan == 1 {
            return Ok(false);
        }
        cell.rowspan = 1;
        cell.colspan = 1;
        Ok(true)
    }

    /// Cells drawn: `(row, col, x, y, w, h)` in fractions of the table size.
    pub fn layout(&self) -> Vec<(usize, usize, f32, f32, f32, f32)> {
        let sum = |v: &[f32]| v.iter().sum::<f32>().max(0.0001);
        let (th, tw) = (sum(&self.rows), sum(&self.cols));
        let mut ys = vec![0.0f32];
        for h in &self.rows {
            ys.push(ys.last().unwrap() + h / th);
        }
        let mut xs = vec![0.0f32];
        for w in &self.cols {
            xs.push(xs.last().unwrap() + w / tw);
        }
        let mut covered = vec![vec![false; self.col_count()]; self.row_count()];
        let mut out = Vec::new();
        for r in 0..self.row_count() {
            for c in 0..self.col_count() {
                if covered[r][c] {
                    continue;
                }
                let cell = &self.cells[r][c];
                let re = (r + cell.rowspan).min(self.row_count());
                let ce = (c + cell.colspan).min(self.col_count());
                for row in covered.iter_mut().take(re).skip(r) {
                    for x in row.iter_mut().take(ce).skip(c) {
                        *x = true;
                    }
                }
                out.push((r, c, xs[c], ys[r], xs[ce] - xs[c], ys[re] - ys[r]));
            }
        }
        out
    }

    /// Content stream drawing the table inside `rect` (PDF space), and the
    /// writer of its text, whose font goes in the resources of the drawing
    /// ([`Document::drawing_resources`]). The font is chosen for the text of
    /// all the cells drawn; text that cannot be drawn is refused before
    /// anything is drawn.
    pub(crate) fn drawing(&self, rect: [f32; 4]) -> std::result::Result<(String, TextWriter), GlyphProblem> {
        let f = fmt_num;
        let cells = self.layout();
        let mut writer = TextWriter::for_pieces(
            cells
                .iter()
                .map(|(r, col, ..)| self.cells[*r][*col].text.as_str()),
        )?;
        let (x0, y1) = (rect[0], rect[3]);
        let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
        let mut c = String::new();
        let lc = self.line_color;
        // Fills first, then text, then lines over them.
        for (r, col, cx, cy, cw, ch) in &cells {
            if let Some(fill) = self.cells[*r][*col].fill {
                c.push_str(&format!(
                    "{} {} {} rg {} {} {} {} re f\n",
                    f(fill[0]),
                    f(fill[1]),
                    f(fill[2]),
                    f(x0 + cx * w),
                    f(y1 - (cy + ch) * h),
                    f(cw * w),
                    f(ch * h)
                ));
            }
        }
        let size = self.font_size.max(2.0);
        let pad = (size * 0.35).max(2.0);
        let tc = self.text_color;
        for (r, col, cx, cy, cw, _ch) in &cells {
            let cell = &self.cells[*r][*col];
            let text = writer.piece(&cell.text);
            if text.trim().is_empty() {
                continue;
            }
            let factor = if cell.bold { 1.06 } else { 1.0 };
            let left = x0 + cx * w;
            let top = y1 - cy * h;
            let inner = (cw * w - 2.0 * pad).max(1.0);
            let lines = writer.wrap_piece(&text, size * factor, inner);
            for (k, line) in lines.iter().enumerate() {
                let lw = writer.width(line, size) * factor;
                let x = match cell.align.as_str() {
                    "center" => left + (cw * w - lw) / 2.0,
                    "right" => left + cw * w - pad - lw,
                    _ => left + pad,
                };
                let y = top - pad - size * (0.8 + 1.2 * k as f32);
                let at = format!("{} {} Td", f(x), f(y));
                c.push_str(&writer.text_object(line, size, Some(tc), &at, cell.bold, ("TR", "TB"))?);
                c.push('\n');
            }
        }
        if self.line_width > 0.0 {
            c.push_str(&format!(
                "{} {} {} RG {} w\n",
                f(lc[0]),
                f(lc[1]),
                f(lc[2]),
                f(self.line_width)
            ));
            for (_, _, cx, cy, cw, ch) in &cells {
                c.push_str(&format!(
                    "{} {} {} {} re S\n",
                    f(x0 + cx * w),
                    f(y1 - (cy + ch) * h),
                    f(cw * w),
                    f(ch * h)
                ));
            }
        }
        Ok((c, writer))
    }
}

impl Document {
    pub(crate) fn table_resources(&mut self) -> lopdf::Dictionary {
        let regular = self.inner.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let bold = self.inner.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding",
        });
        dictionary! { "Font" => dictionary! { "TR" => regular, "TB" => bold } }
    }

    /// The resources of a drawing of a table or a tape: its standard fonts
    /// (`TR`, `TB`), or the Unicode font of `writer`; `previous` is used
    /// again when it is the same subset (a drawing made again).
    pub(crate) fn drawing_resources(
        &mut self,
        writer: TextWriter,
        previous: Option<ObjectId>,
    ) -> Result<lopdf::Dictionary> {
        if writer.is_standard() {
            return Ok(self.table_resources());
        }
        let name = writer.resource_name();
        let font = writer.finish(self, previous)?;
        Ok(dictionary! { "Font" => dictionary! { name => font } })
    }

    /// Add a table inside `rect` (PDF space) as an editable comment.
    pub fn add_table(
        &mut self,
        page: usize,
        rect: [f32; 4],
        table: &Table,
        author: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let r = crate::geom::normalize(rect);
        if r[2] - r[0] < 10.0 || r[3] - r[1] < 6.0 {
            return Err(Error::Invalid("the table is too small".into()));
        }
        let json = serde_json::to_string(table).map_err(|e| Error::Invalid(e.to_string()))?;
        // Text that cannot be drawn is refused before the document changes.
        let (content, writer) = table.drawing(r)?;
        self.begin_edit();
        let mut annot = self.base_annotation(page_id, "Stamp", r, author, "", table.line_color);
        annot.set("Name", Object::Name(TABLE_STAMP.as_bytes().to_vec()));
        annot.set("Subj", lopdf::text_string("Table"));
        annot.set("FolitectaTable", Object::string_literal(json));
        let res = match self.drawing_resources(writer, None) {
            Ok(res) => res,
            Err(e) => {
                self.abandon_edit();
                return Err(e);
            }
        };
        self.add_appearance(&mut annot, r, content, res);
        let id = self.push_drawn_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// The table of a comment, when it is one.
    pub fn table_at(&self, page: usize, index: usize) -> Option<Table> {
        let id = self.annotation_object(page, index).ok()?;
        let d = self.inner.get_dictionary(id).ok()?;
        match d.get(b"FolitectaTable").ok()? {
            Object::String(s, _) => serde_json::from_slice(s).ok(),
            _ => None,
        }
    }

    /// Replace the table of a comment and redraw it.
    pub fn set_table(&mut self, page: usize, index: usize, table: &Table) -> Result<()> {
        if self.table_at(page, index).is_none() {
            return Err(Error::Invalid("the comment is not a table".into()));
        }
        let id = self.annotation_object(page, index)?;
        let rect = self
            .annotations(page)?
            .get(index)
            .map(|a| a.rect)
            .ok_or_else(|| Error::Invalid("table not found".into()))?;
        let json = serde_json::to_string(table).map_err(|e| Error::Invalid(e.to_string()))?;
        // Text that cannot be drawn is refused before the document changes.
        let (content, writer) = table.drawing(rect)?;
        let mut annot = self.inner.get_dictionary(id)?.clone();
        let previous = previous_font(&self.inner, &annot, &writer);
        self.begin_edit();
        let res = match self.drawing_resources(writer, previous) {
            Ok(res) => res,
            Err(e) => {
                self.abandon_edit();
                return Err(e);
            }
        };
        annot.set("FolitectaTable", Object::string_literal(json));
        self.add_appearance(&mut annot, rect, content, res);
        *self.inner.get_dictionary_mut(id)? = annot;
        self.commit()
    }

    /// Tables of the document: `(page, index)`.
    pub fn tables(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for p in 0..self.page_count() {
            for a in self.annotations(p).unwrap_or_default() {
                if a.subtype == "Stamp" && self.table_at(p, a.index).is_some() {
                    out.push((p, a.index));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_columns_and_merges() {
        let mut t = Table::new(3, 3);
        t.cells[0][0].text = "A".into();
        t.cells[0][1].text = "B".into();
        t.merge(0, 0, 1, 1).unwrap();
        assert_eq!(t.cells[0][0].text, "A B");
        assert_eq!(t.owner(1, 1), (0, 0));
        assert_eq!(t.layout().len(), 6, "a 2x2 merge hides 3 cells");
        t.insert_row(1);
        assert_eq!(t.cells[0][0].rowspan, 3, "the merge grows across the new row");
        t.insert_col(3);
        assert_eq!(t.col_count(), 4);
        assert!(t.split(2, 1).unwrap());
        assert_eq!(t.layout().len(), 16);
        t.delete_rows(0, 1).unwrap();
        t.delete_cols(3, 3).unwrap();
        assert_eq!((t.row_count(), t.col_count()), (2, 3));
        assert!(t.delete_rows(0, 1).is_err());
        assert!(t.merge(0, 0, 5, 5).is_err());
    }
}
