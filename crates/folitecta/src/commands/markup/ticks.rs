//! Tick marks: the mark the tool places (shape, sequence, next text, size,
//! colour, kept in the settings), placed with a click, the sequence going
//! on by one after each mark that shows it.

use dioxus::prelude::ReadableExt;
use folitecta_core::{Sequence, TickMark, TickShape};

use crate::commands::{handlers, no_doc};
use crate::state::{AppState, Dialog};

/// What the tool places, from the settings.
#[derive(Debug, Clone, PartialEq)]
pub struct TickSettings {
    pub shape: TickShape,
    pub sequence: Sequence,
    /// The place in the sequence of the next mark (from 1).
    pub next: u32,
    pub size: f32,
    pub color: [f32; 3],
}

pub fn settings(st: AppState) -> TickSettings {
    let get = |k: &str| st.value(k);
    TickSettings {
        shape: TickShape::from_id(&get("tick.shape")),
        sequence: Sequence::from_id(&get("tick.sequence")),
        next: get("tick.next").trim().parse().unwrap_or(1).max(1),
        size: get("tick.size")
            .trim()
            .parse()
            .unwrap_or(18.0f32)
            .clamp(4.0, 200.0),
        color: {
            let c = get("tick.color");
            if c.trim().is_empty() {
                [0.1, 0.55, 0.2]
            } else {
                crate::util::parse_color(&c)
            }
        },
    }
}

pub fn save(st: AppState, s: &TickSettings) {
    st.set_value("tick.shape", s.shape.id());
    st.set_value("tick.sequence", s.sequence.id());
    st.set_value("tick.next", s.next.to_string());
    st.set_value("tick.size", format!("{}", s.size));
    st.set_value("tick.color", crate::util::color_hex(s.color));
}

/// The mark the tool places next.
pub fn next_mark(s: &TickSettings) -> TickMark {
    TickMark {
        shape: s.shape,
        text: if s.shape.has_text() {
            s.sequence.label(s.next)
        } else {
            String::new()
        },
        size: s.size,
        color: s.color,
    }
}

/// Place the next mark centred on `at` (PDF user space) of `page`.
pub fn place(st: AppState, page: usize, at: (f32, f32)) -> folitecta_core::Result<()> {
    let s = settings(st);
    let mark = next_mark(&s);
    let author = st.ui.peek().author.clone();
    let placed = st
        .with_active(|t| t.doc.add_tick_mark(page, at, &mark, &author).map(|_| ()))
        .unwrap_or(Ok(()));
    if placed.is_ok() && s.shape.has_text() {
        st.set_value("tick.next", (s.next + 1).to_string());
    }
    placed
}

fn open(st: AppState) {
    if no_doc(st) {
        return;
    }
    st.set_dialog(Dialog::TickMarks);
}

handlers! {
    "tool.annot.tickMark" => open,
}
