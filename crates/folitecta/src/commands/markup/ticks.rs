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

/// A saved set of tick styles: all but the place in the sequence.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedSet {
    name: String,
    shape: String,
    sequence: String,
    size: f32,
    color: String,
}

/// The sets written in `text` (the setting `tick.sets`), in their order;
/// the place in the sequence of each is 1.
fn read_sets(text: &str) -> Vec<(String, TickSettings)> {
    serde_json::from_str::<Vec<SavedSet>>(text)
        .unwrap_or_default()
        .into_iter()
        .map(|s| {
            (
                s.name,
                TickSettings {
                    shape: TickShape::from_id(&s.shape),
                    sequence: Sequence::from_id(&s.sequence),
                    next: 1,
                    size: s.size.clamp(4.0, 200.0),
                    color: crate::util::parse_color(&s.color),
                },
            )
        })
        .collect()
}

fn write_sets(sets: &[(String, TickSettings)]) -> String {
    let list: Vec<SavedSet> = sets
        .iter()
        .map(|(name, s)| SavedSet {
            name: name.clone(),
            shape: s.shape.id().to_string(),
            sequence: s.sequence.id().to_string(),
            size: s.size,
            color: crate::util::color_hex(s.color),
        })
        .collect();
    serde_json::to_string(&list).unwrap_or_default()
}

/// The saved sets of tick styles, by name.
pub fn sets(st: AppState) -> Vec<(String, TickSettings)> {
    read_sets(&st.value("tick.sets"))
}

/// Save `s` as the set `name`, in place of a set of that name.
pub fn save_set(st: AppState, name: &str, s: &TickSettings) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let mut all = sets(st);
    match all.iter_mut().find(|(n, _)| n == name) {
        Some(set) => set.1 = s.clone(),
        None => all.push((name.to_string(), s.clone())),
    }
    st.set_value("tick.sets", write_sets(&all));
}

/// Forget the set `name`.
pub fn delete_set(st: AppState, name: &str) {
    let mut all = sets(st);
    all.retain(|(n, _)| n != name);
    st.set_value("tick.sets", write_sets(&all));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_of_tick_styles_are_written_and_read_back() {
        let set = TickSettings {
            shape: TickShape::Circled,
            sequence: Sequence::Roman,
            next: 7,
            size: 24.0,
            color: [1.0, 0.0, 0.0],
        };
        let text = write_sets(&[("Review".into(), set.clone())]);
        let back = read_sets(&text);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].0, "Review");
        // All but the place in the sequence.
        assert_eq!(back[0].1, TickSettings { next: 1, ..set });
        assert!(read_sets("").is_empty());
        assert!(read_sets("not json").is_empty());
    }
}
