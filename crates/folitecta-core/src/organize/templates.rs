//! Templates of new documents (FOL-L010): papers to write on (ruled,
//! squared, dotted, graph paper, music staves, Cornell notes, handwriting
//! lines), planners (a week, a month, a list of things to do) and office
//! papers (a letter, meeting notes, an invoice), made on the paper chosen.
//! The office papers and the list are forms to fill in; the invoice works
//! out its amounts, subtotal and total. Their words come in the language of
//! the interface (`TemplateOptions::labels`).

use std::collections::BTreeMap;

use crate::document::Document;
use crate::error::{Error, Result};
use crate::form_calc::{CalcOp, Calculation};
use crate::form_design::{FieldProps, NewField};
use crate::typeset::{Fonts, Style};
use crate::util::fmt_num;

/// Points in a millimetre.
const MM: f32 = 72.0 / 25.4;

/// The kinds of template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Template {
    Ruled,
    Squared,
    Dotted,
    Graph,
    Music,
    Cornell,
    Handwriting,
    Week,
    Month,
    ToDo,
    Letter,
    Meeting,
    Invoice,
}

/// The groups of the gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateGroup {
    Paper,
    Planner,
    Office,
}

impl Template {
    pub const ALL: [Template; 13] = [
        Template::Ruled,
        Template::Squared,
        Template::Dotted,
        Template::Graph,
        Template::Music,
        Template::Cornell,
        Template::Handwriting,
        Template::Week,
        Template::Month,
        Template::ToDo,
        Template::Letter,
        Template::Meeting,
        Template::Invoice,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Template::Ruled => "ruled",
            Template::Squared => "squared",
            Template::Dotted => "dotted",
            Template::Graph => "graph",
            Template::Music => "music",
            Template::Cornell => "cornell",
            Template::Handwriting => "handwriting",
            Template::Week => "week",
            Template::Month => "month",
            Template::ToDo => "todo",
            Template::Letter => "letter",
            Template::Meeting => "meeting",
            Template::Invoice => "invoice",
        }
    }

    pub fn from_id(id: &str) -> Option<Template> {
        Template::ALL.into_iter().find(|t| t.id() == id)
    }

    pub fn group(self) -> TemplateGroup {
        match self {
            Template::Ruled
            | Template::Squared
            | Template::Dotted
            | Template::Graph
            | Template::Music
            | Template::Cornell
            | Template::Handwriting => TemplateGroup::Paper,
            Template::Week | Template::Month | Template::ToDo => TemplateGroup::Planner,
            Template::Letter | Template::Meeting | Template::Invoice => TemplateGroup::Office,
        }
    }

    /// The template has fields to fill in.
    pub fn fillable(self) -> bool {
        matches!(
            self,
            Template::ToDo | Template::Letter | Template::Meeting | Template::Invoice
        )
    }
}

/// How a template is made.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateOptions {
    /// The paper (points).
    pub width: f32,
    pub height: f32,
    /// How many pages (a month planner: how many months from `month`).
    pub pages: usize,
    /// The first month of a month planner (1 to 12) and its year.
    pub month: u8,
    pub year: i32,
    /// Weeks start on Monday (else on Sunday).
    pub monday_first: bool,
    /// The words of the templates, by key (see [`word`]); English when absent.
    pub labels: BTreeMap<String, String>,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        TemplateOptions {
            width: 595.28,
            height: 841.89,
            pages: 1,
            month: 1,
            year: 2026,
            monday_first: true,
            labels: BTreeMap::new(),
        }
    }
}

/// The English words of the templates, by key: what `labels` replaces.
pub const WORDS: &[(&str, &str)] = &[
    ("title_ruled", "Ruled paper"),
    ("title_week", "Week of"),
    ("title_todo", "To do"),
    ("title_meeting", "Meeting notes"),
    ("title_invoice", "Invoice"),
    ("cues", "Cues"),
    ("notes", "Notes"),
    ("summary", "Summary"),
    ("topic", "Topic"),
    ("date", "Date"),
    ("time", "Time"),
    ("place", "Place"),
    ("attendees", "Attendees"),
    ("agenda", "Agenda"),
    ("decisions", "Decisions"),
    ("actions", "Actions"),
    ("what", "What"),
    ("who", "Who"),
    ("by_when", "By when"),
    ("sender", "From"),
    ("recipient", "To"),
    ("subject", "Subject"),
    ("signature", "Signature"),
    ("number", "Number"),
    ("due_date", "Due date"),
    ("bill_to", "Bill to"),
    ("description", "Description"),
    ("quantity", "Quantity"),
    ("unit_price", "Unit price"),
    ("amount", "Amount"),
    ("subtotal", "Subtotal"),
    ("tax", "Tax"),
    ("total", "Total"),
    ("payment", "Payment details"),
    ("day_1", "Monday"),
    ("day_2", "Tuesday"),
    ("day_3", "Wednesday"),
    ("day_4", "Thursday"),
    ("day_5", "Friday"),
    ("day_6", "Saturday"),
    ("day_7", "Sunday"),
    ("month_1", "January"),
    ("month_2", "February"),
    ("month_3", "March"),
    ("month_4", "April"),
    ("month_5", "May"),
    ("month_6", "June"),
    ("month_7", "July"),
    ("month_8", "August"),
    ("month_9", "September"),
    ("month_10", "October"),
    ("month_11", "November"),
    ("month_12", "December"),
];

/// The word `key` in the language of `labels`, or in English.
pub fn word(labels: &BTreeMap<String, String>, key: &str) -> String {
    labels
        .get(key)
        .filter(|w| !w.trim().is_empty())
        .cloned()
        .or_else(|| WORDS.iter().find(|(k, _)| *k == key).map(|(_, w)| w.to_string()))
        .unwrap_or_else(|| key.to_string())
}

/// The year and month of today (UTC).
pub fn this_month() -> (i32, u8) {
    let now = time::OffsetDateTime::now_utc();
    (now.year(), u8::from(now.month()))
}

/// Days in `month` (1 to 12) of `year`.
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        _ => 28,
    }
}

/// The day of the week of the first day of `month` of `year`: 0 Monday to 6 Sunday.
pub fn first_weekday(year: i32, month: u8) -> u8 {
    // Zeller's congruence (0 Saturday), turned to 0 Monday.
    let (y, m) = if month < 3 {
        (year - 1, i32::from(month) + 12)
    } else {
        (year, i32::from(month))
    };
    let (k, j) = (y.rem_euclid(100), y.div_euclid(100));
    let h = (1 + (13 * (m + 1)) / 5 + k + k / 4 + j / 4 + 5 * j).rem_euclid(7);
    ((h + 5) % 7) as u8
}

/// A light ink for lines and dots, a darker one for words.
const LINE: [f32; 3] = [0.62, 0.74, 0.88];
const FAINT: [f32; 3] = [0.8, 0.86, 0.93];
const MARGIN: [f32; 3] = [0.9, 0.5, 0.5];
const INK: [f32; 3] = [0.15, 0.17, 0.22];
const GREY: [f32; 3] = [0.45, 0.48, 0.55];

/// A field to add once the pages are made: page, kind, rectangle, properties.
type Field = (usize, NewField, [f32; 4], FieldProps);

/// Pages being drawn, with their fonts and fields.
struct Draw {
    w: f32,
    h: f32,
    m: f32,
    pages: Vec<String>,
    fonts: Fonts,
    fields: Vec<Field>,
    /// Calculated fields: the field and its calculation.
    calcs: Vec<(String, Calculation)>,
}

impl Draw {
    fn page(&mut self) -> &mut String {
        if self.pages.is_empty() {
            self.pages.push(String::new());
        }
        self.pages.last_mut().expect("a page")
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, color: [f32; 3], dash: Option<f32>) {
        let f = fmt_num;
        let d = dash
            .map(|d| format!("[{} {}] 0 d ", f(d), f(d)))
            .unwrap_or_default();
        let s = format!(
            "q {}{} {} {} RG {} w {} {} m {} {} l S Q\n",
            d,
            f(color[0]),
            f(color[1]),
            f(color[2]),
            f(width),
            f(a.0),
            f(a.1),
            f(b.0),
            f(b.1)
        );
        self.page().push_str(&s);
    }

    fn rect(&mut self, r: [f32; 4], width: f32, color: [f32; 3]) {
        let f = fmt_num;
        let s = format!(
            "q {} {} {} RG {} w {} {} {} {} re S Q\n",
            f(color[0]),
            f(color[1]),
            f(color[2]),
            f(width),
            f(r[0]),
            f(r[1]),
            f(r[2] - r[0]),
            f(r[3] - r[1])
        );
        self.page().push_str(&s);
    }

    fn fill(&mut self, r: [f32; 4], color: [f32; 3]) {
        let f = fmt_num;
        let s = format!(
            "q {} {} {} rg {} {} {} {} re f Q\n",
            f(color[0]),
            f(color[1]),
            f(color[2]),
            f(r[0]),
            f(r[1]),
            f(r[2] - r[0]),
            f(r[3] - r[1])
        );
        self.page().push_str(&s);
    }

    fn dot(&mut self, (x, y): (f32, f32), r: f32, color: [f32; 3]) {
        // A square dot: at these sizes it prints as round.
        self.fill([x - r, y - r, x + r, y + r], color);
    }

    fn text(&mut self, s: &str, size: f32, (x, y): (f32, f32), color: [f32; 3], bold: bool) -> Result<()> {
        let style = Style {
            bold,
            ..Style::default()
        };
        let piece = self.fonts.piece(s);
        let ops = self.fonts.show(&piece, style, size, (x, y), color)?;
        self.page().push_str(&ops);
        Ok(())
    }

    fn width(&self, s: &str, size: f32, bold: bool) -> f32 {
        let style = Style {
            bold,
            ..Style::default()
        };
        self.fonts.width(&self.fonts.piece(s), style, size)
    }

    /// A title at the top of the page; returns the y under it.
    fn title(&mut self, s: &str) -> Result<f32> {
        let size = 18.0;
        let y = self.h - self.m - size;
        self.text(s, size, (self.m, y), INK, true)?;
        Ok(y - 10.0)
    }

    /// A small label over a field.
    fn label(&mut self, s: &str, (x, y): (f32, f32)) -> Result<()> {
        self.text(s, 8.0, (x, y), GREY, false)
    }

    fn field(&mut self, kind: NewField, rect: [f32; 4], name: &str, multiline: bool) {
        let page = self.pages.len().saturating_sub(1);
        let props = FieldProps {
            name: name.to_string(),
            multiline,
            font_size: if multiline { 10.0 } else { 0.0 },
            border_color: None,
            fill_color: Some([0.95, 0.96, 1.0]),
            border_width: 0.0,
            ..FieldProps::default()
        };
        self.fields.push((page, kind, rect, props));
    }

    /// A labelled field from `x0` to `x1` with its top at `top`; returns the y under it.
    fn labelled(&mut self, label: &str, name: &str, x0: f32, x1: f32, top: f32, height: f32) -> Result<f32> {
        self.label(label, (x0, top - 8.0))?;
        let r = [x0, top - 11.0 - height, x1, top - 11.0];
        self.field(NewField::Text, r, name, height > 22.0);
        self.line((x0, r[1]), (x1, r[1]), 0.6, LINE, None);
        Ok(r[1] - 8.0)
    }
}

/// Every word a template may show, for the choice of the fonts.
fn all_words(t: Template, o: &TemplateOptions) -> Vec<String> {
    let mut out: Vec<String> = WORDS.iter().map(|(k, _)| word(&o.labels, k)).collect();
    out.push(format!("{} 0123456789 /:", o.year));
    let _ = t;
    out
}

fn ruled(d: &mut Draw, spacing: f32, margin_line: bool) {
    let (w, h, m) = (d.w, d.h, d.m);
    let top = h - m - 12.0 * MM;
    let mut y = top;
    while y > m {
        d.line((m, y), (w - m, y), 0.5, LINE, None);
        y -= spacing;
    }
    if margin_line {
        let x = m + 22.0 * MM;
        d.line((x, h - m), (x, m), 0.6, MARGIN, None);
    }
}

fn grid(d: &mut Draw, step: f32, width: f32, color: [f32; 3], every: usize, strong: f32) {
    let (w, h, m) = (d.w, d.h, d.m);
    let nx = ((w - 2.0 * m) / step).floor() as usize;
    let ny = ((h - 2.0 * m) / step).floor() as usize;
    let (x0, y0) = ((w - nx as f32 * step) / 2.0, (h - ny as f32 * step) / 2.0);
    let (x1, y1) = (x0 + nx as f32 * step, y0 + ny as f32 * step);
    for i in 0..=nx {
        let x = x0 + i as f32 * step;
        let lw = if every > 0 && i % every == 0 {
            strong
        } else {
            width
        };
        d.line((x, y0), (x, y1), lw, color, None);
    }
    for j in 0..=ny {
        let y = y0 + j as f32 * step;
        let lw = if every > 0 && j % every == 0 {
            strong
        } else {
            width
        };
        d.line((x0, y), (x1, y), lw, color, None);
    }
}

fn dotted(d: &mut Draw, step: f32) {
    let (w, h, m) = (d.w, d.h, d.m);
    let nx = ((w - 2.0 * m) / step).floor() as usize;
    let ny = ((h - 2.0 * m) / step).floor() as usize;
    let (x0, y0) = ((w - nx as f32 * step) / 2.0, (h - ny as f32 * step) / 2.0);
    for i in 0..=nx {
        for j in 0..=ny {
            d.dot((x0 + i as f32 * step, y0 + j as f32 * step), 0.55, GREY);
        }
    }
}

fn music(d: &mut Draw) {
    let (w, h, m) = (d.w, d.h, d.m);
    let line_gap = 2.0 * MM;
    let staff = 4.0 * line_gap;
    let gap = 11.0 * MM;
    let mut top = h - m - 6.0 * MM;
    while top - staff > m {
        for i in 0..5 {
            let y = top - i as f32 * line_gap;
            d.line((m, y), (w - m, y), 0.5, INK, None);
        }
        top -= staff + gap;
    }
}

fn handwriting(d: &mut Draw) {
    let (w, h, m) = (d.w, d.h, d.m);
    let x = 4.0 * MM;
    let mut top = h - m - 6.0 * MM;
    // Ascender line, dashed middle line, baseline, dotted descender line.
    while top - 3.0 * x > m {
        d.line((m, top), (w - m, top), 0.6, LINE, None);
        d.line((m, top - x), (w - m, top - x), 0.5, LINE, Some(3.0));
        d.line((m, top - 2.0 * x), (w - m, top - 2.0 * x), 0.8, LINE, None);
        d.line((m, top - 3.0 * x), (w - m, top - 3.0 * x), 0.4, FAINT, Some(1.0));
        top -= 3.0 * x + 5.0 * MM;
    }
}

fn cornell(d: &mut Draw, o: &TemplateOptions) -> Result<()> {
    let (w, h, m) = (d.w, d.h, d.m);
    let head = h - m - 14.0 * MM;
    d.label(&word(&o.labels, "topic"), (m, h - m - 8.0))?;
    d.label(&word(&o.labels, "date"), (w - m - 45.0 * MM, h - m - 8.0))?;
    d.line((m, head), (w - m, head), 1.0, INK, None);
    let summary = m + (50.0 * MM).min((h - 2.0 * m) * 0.22);
    let cue = m + (w - 2.0 * m) * 0.3;
    d.line((cue, head), (cue, summary), 0.8, INK, None);
    d.line((m, summary), (w - m, summary), 1.0, INK, None);
    d.label(&word(&o.labels, "cues"), (m, head - 12.0))?;
    d.label(&word(&o.labels, "notes"), (cue + 6.0, head - 12.0))?;
    d.label(&word(&o.labels, "summary"), (m, summary - 12.0))?;
    let mut y = head - 8.0 * MM;
    while y > summary + 3.0 {
        d.line((cue + 4.0, y), (w - m, y), 0.4, LINE, None);
        y -= 8.0 * MM;
    }
    Ok(())
}

/// The week: seven boxes of days and one for notes, two columns.
fn week(d: &mut Draw, o: &TemplateOptions) -> Result<()> {
    let (w, m) = (d.w, d.m);
    let top = d.title(&word(&o.labels, "title_week"))?;
    let tw = d.width(&word(&o.labels, "title_week"), 18.0, true);
    d.line(
        (m + tw + 8.0, top + 10.0),
        (m + tw + 70.0 * MM, top + 10.0),
        0.6,
        LINE,
        None,
    );
    let (cols, rows) = (2usize, 4usize);
    let gap = 4.0 * MM;
    let bw = (w - 2.0 * m - gap) / cols as f32;
    let bh = (top - 6.0 - m - gap * (rows as f32 - 1.0)) / rows as f32;
    let order: Vec<u8> = if o.monday_first {
        (1..=7).collect()
    } else {
        std::iter::once(7).chain(1..=6).collect()
    };
    for k in 0..8 {
        let (c, r) = (k % cols, k / cols);
        let x = m + c as f32 * (bw + gap);
        let ytop = top - 6.0 - r as f32 * (bh + gap);
        let name = match order.get(k) {
            Some(day) => word(&o.labels, &format!("day_{day}")),
            None => word(&o.labels, "notes"),
        };
        d.rect([x, ytop - bh, x + bw, ytop], 0.8, LINE);
        d.fill([x, ytop - 16.0, x + bw, ytop], [0.93, 0.95, 0.99]);
        d.text(&name, 9.5, (x + 6.0, ytop - 11.5), INK, true)?;
        let mut y = ytop - 16.0 - 7.0 * MM;
        while y > ytop - bh + 3.0 {
            d.line((x + 6.0, y), (x + bw - 6.0, y), 0.4, FAINT, None);
            y -= 7.0 * MM;
        }
    }
    Ok(())
}

/// A month: its name and year, the days of the week, the days in a grid.
fn month(d: &mut Draw, o: &TemplateOptions, year: i32, month: u8) -> Result<()> {
    let (w, m) = (d.w, d.m);
    let title = format!("{} {}", word(&o.labels, &format!("month_{month}")), year);
    let top = d.title(&title)? - 4.0;
    let cw = (w - 2.0 * m) / 7.0;
    let head = 18.0;
    let order: Vec<u8> = if o.monday_first {
        (1..=7).collect()
    } else {
        std::iter::once(7).chain(1..=6).collect()
    };
    for (i, day) in order.iter().enumerate() {
        let name = word(&o.labels, &format!("day_{day}"));
        let short: String = name.chars().take(3).collect();
        let x = m + i as f32 * cw;
        d.fill([x, top - head, x + cw, top], [0.93, 0.95, 0.99]);
        let tw = d.width(&short, 9.0, true);
        d.text(&short, 9.0, (x + (cw - tw) / 2.0, top - 12.5), INK, true)?;
    }
    // The column of the first day, the rows the days need.
    let first = first_weekday(year, month);
    let lead = if o.monday_first { first } else { (first + 1) % 7 } as usize;
    let days = days_in_month(year, month) as usize;
    let rows = (lead + days).div_ceil(7);
    let rh = (top - head - m) / rows as f32;
    for r in 0..rows {
        for c in 0..7 {
            let x = m + c as f32 * cw;
            let y = top - head - r as f32 * rh;
            d.rect([x, y - rh, x + cw, y], 0.6, LINE);
            let n = (r * 7 + c + 1) as isize - lead as isize;
            if n >= 1 && n <= days as isize {
                d.text(&n.to_string(), 10.0, (x + 4.0, y - 12.0), INK, false)?;
            }
        }
    }
    Ok(())
}

/// Things to do: a check box and a line for each.
fn todo(d: &mut Draw, o: &TemplateOptions, page: usize) -> Result<()> {
    let (w, m) = (d.w, d.m);
    let mut y = d.title(&word(&o.labels, "title_todo"))? - 6.0;
    let row = 9.0 * MM;
    let mut k = 0;
    while y - row > m {
        let box_ = [m, y - row + 6.0, m + 12.0, y - row + 18.0];
        d.field(
            NewField::CheckBox,
            box_,
            &format!("Done{}_{}", page + 1, k + 1),
            false,
        );
        d.rect(box_, 0.8, LINE);
        let line = [m + 20.0, y - row + 4.0, w - m, y - row + 20.0];
        d.field(
            NewField::Text,
            line,
            &format!("Task{}_{}", page + 1, k + 1),
            false,
        );
        d.line((line[0], line[1]), (line[2], line[1]), 0.5, LINE, None);
        y -= row;
        k += 1;
    }
    Ok(())
}

fn letter(d: &mut Draw, o: &TemplateOptions) -> Result<()> {
    let (w, h, m) = (d.w, d.h, d.m);
    let half = (w - 2.0 * m) / 2.0;
    let address = ((h - 2.0 * m) * 0.12).clamp(20.0, 52.0);
    // The sender and the date at the top, the recipient below on the right.
    let top = h - m;
    let mut y = d.labelled(
        &word(&o.labels, "sender"),
        "From",
        m,
        m + half - 10.0,
        top,
        address,
    )?;
    d.labelled(
        &word(&o.labels, "date"),
        "Date",
        w - m - half * 0.6,
        w - m,
        top,
        16.0,
    )?;
    y = d.labelled(
        &word(&o.labels, "recipient"),
        "To",
        w - m - half + 10.0,
        w - m,
        y - 8.0,
        address,
    )?;
    y = d.labelled(&word(&o.labels, "subject"), "Subject", m, w - m, y - 6.0, 16.0)?;
    let sign = m + (28.0 * MM).min(address);
    let body_top = y - 6.0;
    let r = [m, (sign + 10.0 * MM).min(body_top - 8.0), w - m, body_top];
    d.field(NewField::Text, r, "Body", true);
    let sign_x = w - m - (70.0 * MM).min(half);
    d.label(&word(&o.labels, "signature"), (sign_x, sign + 4.0))?;
    d.line((sign_x, sign), (w - m, sign), 0.6, LINE, None);
    Ok(())
}

fn meeting(d: &mut Draw, o: &TemplateOptions) -> Result<()> {
    let (w, m) = (d.w, d.m);
    let mut y = d.title(&word(&o.labels, "title_meeting"))? - 4.0;
    let third = (w - 2.0 * m - 2.0 * 8.0) / 3.0;
    d.labelled(&word(&o.labels, "date"), "Date", m, m + third, y, 16.0)?;
    d.labelled(
        &word(&o.labels, "time"),
        "Time",
        m + third + 8.0,
        m + 2.0 * third + 8.0,
        y,
        16.0,
    )?;
    y = d.labelled(
        &word(&o.labels, "place"),
        "Place",
        m + 2.0 * (third + 8.0),
        w - m,
        y,
        16.0,
    )?;
    // The rest shared out: a label takes 21 points over each field, the
    // actions a heading and their rows.
    let rest = y - m;
    let rows = ((rest * 0.22 - 30.0) / 16.0).floor().clamp(2.0, 6.0) as usize;
    let actions_h = 30.0 + rows as f32 * 16.0;
    let free = (rest - actions_h - 4.0 * 21.0).max(24.0);
    let parts = [
        ("attendees", "Attendees", 0.12),
        ("agenda", "Agenda", 0.2),
        ("notes", "Notes", 0.48),
        ("decisions", "Decisions", 0.2),
    ];
    for (key, name, share) in parts {
        y = d.labelled(
            &word(&o.labels, key),
            name,
            m,
            w - m,
            y - 2.0,
            (free * share).max(6.0),
        )?;
    }
    // The actions: what, who, by when.
    d.text(&word(&o.labels, "actions"), 10.0, (m, y - 12.0), INK, true)?;
    let mut top = y - 18.0;
    let cols = [m, m + (w - 2.0 * m) * 0.6, m + (w - 2.0 * m) * 0.82, w - m];
    for (i, key) in ["what", "who", "by_when"].iter().enumerate() {
        d.label(&word(&o.labels, key), (cols[i] + 2.0, top - 9.0))?;
    }
    top -= 12.0;
    for row in 0..rows {
        for i in 0..3 {
            let r = [cols[i] + 2.0, top - 16.0, cols[i + 1] - 4.0, top - 2.0];
            let name = ["What", "Who", "When"][i];
            d.field(NewField::Text, r, &format!("Action{}{}", row + 1, name), false);
            d.line((r[0], r[1]), (r[2], r[1]), 0.5, LINE, None);
        }
        top -= 16.0;
    }
    Ok(())
}

fn invoice(d: &mut Draw, o: &TemplateOptions) -> Result<()> {
    let (w, h, m) = (d.w, d.h, d.m);
    let mut y = d.title(&word(&o.labels, "title_invoice"))? - 4.0;
    let address = ((h - 2.0 * m) * 0.1).clamp(24.0, 52.0);
    // Upright paper: who and to whom on the left, one over the other, the
    // number and dates on the right; sideways all three side by side.
    let wide = w > h;
    let cols = if wide { 3.0 } else { 2.0 };
    let cw = (w - 2.0 * m - (cols - 1.0) * 20.0) / cols;
    let col = |i: f32| (m + i * (cw + 20.0), m + i * (cw + 20.0) + cw);
    let (x0, x1) = col(0.0);
    let from_end = d.labelled(&word(&o.labels, "sender"), "From", x0, x1, y, address)?;
    let (rx0, rx1) = col(cols - 1.0);
    let mut ry = d.labelled(&word(&o.labels, "number"), "Number", rx0, rx1, y, 16.0)?;
    ry = d.labelled(&word(&o.labels, "date"), "Date", rx0, rx1, ry, 16.0)?;
    ry = d.labelled(&word(&o.labels, "due_date"), "DueDate", rx0, rx1, ry, 16.0)?;
    let bill_end = if wide {
        let (bx0, bx1) = col(1.0);
        d.labelled(&word(&o.labels, "bill_to"), "BillTo", bx0, bx1, y, address)?
    } else {
        d.labelled(
            &word(&o.labels, "bill_to"),
            "BillTo",
            x0,
            x1,
            from_end - 4.0,
            address,
        )?
    };
    y = from_end.min(ry).min(bill_end);
    // The items: description, quantity, unit price, amount.
    let cols = [
        m,
        m + (w - 2.0 * m) * 0.52,
        m + (w - 2.0 * m) * 0.66,
        m + (w - 2.0 * m) * 0.83,
        w - m,
    ];
    let mut top = y - 8.0;
    d.fill([m, top - 16.0, w - m, top], [0.93, 0.95, 0.99]);
    for (i, key) in ["description", "quantity", "unit_price", "amount"]
        .iter()
        .enumerate()
    {
        let s = word(&o.labels, key);
        let size = 8.5;
        let x = if i == 0 {
            cols[0] + 4.0
        } else {
            cols[i + 1] - 4.0 - d.width(&s, size, true)
        };
        d.text(&s, size, (x, top - 11.5), INK, true)?;
    }
    top -= 16.0;
    // As many rows as the page holds over the sums and the payment details.
    let rows = ((top - m - 60.0 - 45.0) / 18.0).floor().clamp(1.0, 10.0) as usize;
    let mut amounts = Vec::new();
    for row in 1..=rows {
        let names = [
            format!("Item{row}"),
            format!("Qty{row}"),
            format!("Price{row}"),
            format!("Amount{row}"),
        ];
        for i in 0..4 {
            let r = [cols[i] + 3.0, top - 17.0, cols[i + 1] - 3.0, top - 3.0];
            let props = FieldProps {
                name: names[i].clone(),
                align: if i == 0 { 0 } else { 2 },
                read_only: i == 3,
                fill_color: Some([0.95, 0.96, 1.0]),
                border_color: None,
                border_width: 0.0,
                ..FieldProps::default()
            };
            d.fields.push((d.pages.len() - 1, NewField::Text, r, props));
        }
        d.line((m, top - 18.0), (w - m, top - 18.0), 0.4, FAINT, None);
        d.calcs.push((
            names[3].clone(),
            Calculation {
                op: CalcOp::Product,
                fields: vec![names[1].clone(), names[2].clone()],
            },
        ));
        amounts.push(names[3].clone());
        top -= 18.0;
    }
    // Subtotal, tax, total.
    for (key, name) in [("subtotal", "Subtotal"), ("tax", "Tax"), ("total", "Total")] {
        let s = word(&o.labels, key);
        let bold = name == "Total";
        let sw = d.width(&s, 9.0, bold);
        d.text(&s, 9.0, (cols[3] - 8.0 - sw, top - 13.0), INK, bold)?;
        let r = [cols[3] + 3.0, top - 17.0, cols[4] - 3.0, top - 3.0];
        let props = FieldProps {
            name: name.to_string(),
            align: 2,
            read_only: name != "Tax",
            fill_color: Some([0.95, 0.96, 1.0]),
            border_color: None,
            border_width: 0.0,
            ..FieldProps::default()
        };
        d.fields.push((d.pages.len() - 1, NewField::Text, r, props));
        top -= 20.0;
    }
    d.calcs.push((
        "Subtotal".into(),
        Calculation {
            op: CalcOp::Sum,
            fields: amounts,
        },
    ));
    d.calcs.push((
        "Total".into(),
        Calculation {
            op: CalcOp::Sum,
            fields: vec!["Subtotal".into(), "Tax".into()],
        },
    ));
    let pay_top = top - 6.0;
    let pay_h = (pay_top - 11.0 - m).clamp(8.0, 70.0);
    d.labelled(&word(&o.labels, "payment"), "Payment", m, w - m, pay_top, pay_h)?;
    Ok(())
}

impl Document {
    /// A new document from template `t` with `opts`.
    pub fn from_template(t: Template, opts: &TemplateOptions) -> Result<Document> {
        let (w, h) = (
            opts.width.clamp(144.0, 14400.0),
            opts.height.clamp(144.0, 14400.0),
        );
        if !(1..=12).contains(&opts.month) {
            return Err(Error::Invalid("the month is from 1 to 12".into()));
        }
        let words = all_words(t, opts);
        let fonts = Fonts::new(words.iter().map(String::as_str))?;
        let mut d = Draw {
            w,
            h,
            m: (w.min(h) * 0.06).clamp(18.0, 54.0),
            pages: Vec::new(),
            fonts,
            fields: Vec::new(),
            calcs: Vec::new(),
        };
        let pages = match t {
            // One page: the fields of a form are named once.
            Template::Letter | Template::Meeting | Template::Invoice => 1,
            _ => opts.pages.clamp(1, 366),
        };
        for p in 0..pages {
            d.pages.push(String::new());
            match t {
                Template::Ruled => ruled(&mut d, 8.0 * MM, true),
                Template::Squared => grid(&mut d, 5.0 * MM, 0.35, LINE, 0, 0.35),
                Template::Dotted => dotted(&mut d, 5.0 * MM),
                Template::Graph => grid(&mut d, MM, 0.1, [0.97, 0.78, 0.66], 10, 0.45),
                Template::Music => music(&mut d),
                Template::Cornell => cornell(&mut d, opts)?,
                Template::Handwriting => handwriting(&mut d),
                Template::Week => week(&mut d, opts)?,
                Template::Month => {
                    let k = i32::from(opts.month) - 1 + p as i32;
                    month(
                        &mut d,
                        opts,
                        opts.year + k.div_euclid(12),
                        (k.rem_euclid(12) + 1) as u8,
                    )?;
                }
                Template::ToDo => todo(&mut d, opts, p)?,
                Template::Letter => letter(&mut d, opts)?,
                Template::Meeting => meeting(&mut d, opts)?,
                Template::Invoice => invoice(&mut d, opts)?,
            }
        }
        let Draw {
            pages: contents,
            fonts,
            fields,
            calcs,
            ..
        } = d;
        let mut doc = Document::new_blank(contents.len(), w, h)?;
        let ids = doc.page_ids();
        for (id, content) in ids.iter().zip(&contents) {
            if !content.is_empty() {
                doc.append_page_content(*id, content.clone().into_bytes(), None)?;
            }
        }
        fonts.finish(&mut doc, &ids)?;
        let mut made = std::collections::HashMap::new();
        for (page, kind, rect, props) in fields {
            let name = props.name.clone();
            let id = doc.place_form_field(page, kind, rect, &props, true)?;
            made.insert(name, id);
        }
        doc.commit()?;
        for (name, calc) in calcs {
            if let Some(id) = made.get(&name) {
                doc.set_field_calculation(*id, Some(&calc))?;
            }
        }
        doc.set_path(None);
        doc.clear_history();
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendars() {
        // 1 January 2026 was a Thursday; 1 March 2024 a Friday; 1 February 2027 a Monday.
        assert_eq!(first_weekday(2026, 1), 3);
        assert_eq!(first_weekday(2024, 3), 4);
        assert_eq!(first_weekday(2027, 2), 0);
        assert_eq!(first_weekday(2026, 11), 6);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2026, 4), 30);
    }

    #[test]
    fn words_fall_back_to_english() {
        let mut labels = BTreeMap::new();
        labels.insert("date".to_string(), "Data".to_string());
        assert_eq!(word(&labels, "date"), "Data");
        assert_eq!(word(&labels, "total"), "Total");
        assert_eq!(Template::from_id("invoice"), Some(Template::Invoice));
        assert!(
            Template::ALL
                .iter()
                .all(|t| Template::from_id(t.id()) == Some(*t))
        );
    }
}
