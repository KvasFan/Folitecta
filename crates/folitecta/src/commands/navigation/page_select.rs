//! Advanced Selection of the Thumbnails pane: the pages chosen by what they
//! are (odd or even, within a range, upright or lying) and by what they hold
//! (text, pictures, shapes, comments, links, form fields, or nothing).

use crate::state::{AppState, DocTab, ToastKind};

/// Which pages by their number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parity {
    All,
    Odd,
    Even,
}

/// What a page can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holds {
    Text,
    Pictures,
    Shapes,
    Comments,
    Links,
    Fields,
}

impl Holds {
    pub const ALL: [Holds; 6] = [
        Holds::Text,
        Holds::Pictures,
        Holds::Shapes,
        Holds::Comments,
        Holds::Links,
        Holds::Fields,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Holds::Text => "text",
            Holds::Pictures => "pictures",
            Holds::Shapes => "shapes",
            Holds::Comments => "comments",
            Holds::Links => "links",
            Holds::Fields => "fields",
        }
    }
}

/// What is known of a page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageFacts {
    pub landscape: bool,
    /// What it holds, in the order of [`Holds::ALL`].
    pub holds: [bool; 6],
    /// Nothing on it (see `Document::empty_pages`).
    pub empty: bool,
}

/// The conditions of an Advanced Selection.
#[derive(Debug, Clone, PartialEq)]
pub struct Criteria {
    pub parity: Parity,
    /// Pages from a range typed (0-based); `None`: every page.
    pub range: Option<Vec<usize>>,
    /// `Some(true)`: lying pages only; `Some(false)`: upright ones only.
    pub landscape: Option<bool>,
    /// What the page must hold.
    pub holds: Vec<Holds>,
    /// One of `holds` is enough (else all of them).
    pub any: bool,
    /// Only pages with nothing on them.
    pub empty: bool,
}

impl Default for Criteria {
    fn default() -> Self {
        Criteria {
            parity: Parity::All,
            range: None,
            landscape: None,
            holds: Vec::new(),
            any: false,
            empty: false,
        }
    }
}

/// Whether page `page` (0-based) with `facts` meets `c`.
pub fn matches(c: &Criteria, page: usize, facts: &PageFacts) -> bool {
    let number = page + 1;
    let parity = match c.parity {
        Parity::All => true,
        Parity::Odd => !number.is_multiple_of(2),
        Parity::Even => number.is_multiple_of(2),
    };
    let in_range = c.range.as_ref().is_none_or(|r| r.contains(&page));
    let orientation = c.landscape.is_none_or(|l| l == facts.landscape);
    let held = |h: &Holds| {
        Holds::ALL
            .iter()
            .position(|x| x == h)
            .is_some_and(|i| facts.holds[i])
    };
    let holds = if c.holds.is_empty() {
        true
    } else if c.any {
        c.holds.iter().any(held)
    } else {
        c.holds.iter().all(held)
    };
    parity && in_range && orientation && holds && (!c.empty || facts.empty)
}

/// What page `page` of a tab holds; `fields` are the pages with form fields
/// and `empty` the pages with nothing on them.
fn facts(t: &DocTab, page: usize, fields: &[usize], empty: &[usize]) -> PageFacts {
    let size = t.display_size(page);
    let comments = t
        .doc
        .annotations(page)
        .map(|a| {
            a.iter()
                .any(|x| !["Link", "Widget", "Popup"].contains(&x.subtype.as_str()))
        })
        .unwrap_or(false);
    PageFacts {
        landscape: size.0 > size.1,
        holds: [
            !t.doc.page_text(page).trim().is_empty(),
            t.doc.image_count(page) > 0,
            t.doc.page_paths(page).is_ok_and(|p| !p.is_empty()),
            comments,
            t.doc.links(page).is_ok_and(|l| !l.is_empty()),
            fields.contains(&page),
        ],
        empty: empty.contains(&page),
    }
}

/// Select the pages of the active document that meet `c`, added to the
/// pages selected already when `add`; says how many are selected.
pub fn select(st: AppState, c: &Criteria, add: bool) {
    let count = st.with_active(|t| {
        let n = t.doc.page_count();
        let fields: Vec<usize> = t.doc.form_fields().into_iter().filter_map(|f| f.page).collect();
        let empty = if c.empty { t.doc.empty_pages() } else { Vec::new() };
        let mut chosen: Vec<usize> = if add { t.selected_pages.clone() } else { Vec::new() };
        for p in 0..n {
            if matches(c, p, &facts(t, p, &fields, &empty)) && !chosen.contains(&p) {
                chosen.push(p);
            }
        }
        chosen.sort_unstable();
        let count = chosen.len();
        t.selected_pages = chosen;
        count
    });
    if let Some(count) = count {
        st.toast(
            ToastKind::Info,
            crate::i18n::tf(
                st.lang(),
                "toast.pages_selected",
                &[("count", &count.to_string())],
            ),
            "",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(landscape: bool, holds: [bool; 6], empty: bool) -> PageFacts {
        PageFacts {
            landscape,
            holds,
            empty,
        }
    }

    #[test]
    fn pages_by_number_range_and_orientation() {
        let upright = page(false, [true, false, false, false, false, false], false);
        let lying = page(true, [true, false, false, false, false, false], false);
        let odd = Criteria {
            parity: Parity::Odd,
            ..Criteria::default()
        };
        assert!(matches(&odd, 0, &upright), "page 1 is odd");
        assert!(!matches(&odd, 1, &upright));
        let even_lying_in_range = Criteria {
            parity: Parity::Even,
            range: Some(vec![1, 2, 3]),
            landscape: Some(true),
            ..Criteria::default()
        };
        assert!(matches(&even_lying_in_range, 3, &lying));
        assert!(!matches(&even_lying_in_range, 3, &upright));
        assert!(!matches(&even_lying_in_range, 5, &lying), "out of range");
    }

    #[test]
    fn pages_by_what_they_hold() {
        // Text and a picture; text alone; nothing.
        let both = page(false, [true, true, false, false, false, false], false);
        let text = page(false, [true, false, false, false, false, false], false);
        let blank = page(false, [false; 6], true);
        let all_of = Criteria {
            holds: vec![Holds::Text, Holds::Pictures],
            ..Criteria::default()
        };
        assert!(matches(&all_of, 0, &both));
        assert!(!matches(&all_of, 0, &text));
        let any_of = Criteria {
            any: true,
            ..all_of.clone()
        };
        assert!(matches(&any_of, 0, &text));
        assert!(!matches(&any_of, 0, &blank));
        let empty = Criteria {
            empty: true,
            ..Criteria::default()
        };
        assert!(matches(&empty, 0, &blank));
        assert!(!matches(&empty, 0, &text));
        // No conditions: every page.
        assert!(matches(&Criteria::default(), 7, &blank));
    }
}
