//! Comment styles: styles kept by name for each kind of comment, next to the
//! default style of the kind; applied to comments, made the default of their
//! tool, cloned, renamed, deleted and reset; the styles pane and palette.

use dioxus::prelude::*;

use super::{handlers, show_pane};
use crate::i18n::{Lang, annot_name, t, tf};
use crate::state::{AppState, Dialog, Pane, ToastKind};
use crate::styles::{
    NamedStyle, builtin, default_style, kind_of, named_styles, reset_default_style, save_named_styles,
    set_default_style, tool_subtype,
};

/// What the styles pane shows: a kind of comment and the style chosen
/// (None: the default style of the kind, Some(i): the i-th named style).
#[derive(Debug, Clone, PartialEq)]
pub struct StylesView {
    pub kind: String,
    pub chosen: Option<Option<usize>>,
}

impl Default for StylesView {
    fn default() -> Self {
        StylesView {
            kind: "Square".into(),
            chosen: None,
        }
    }
}

pub static STYLES: GlobalSignal<StylesView> = Signal::global(StylesView::default);

/// The style edited in the properties pane: kind and the named style (None:
/// the default style of the kind).
pub static EDITING: GlobalSignal<Option<(String, Option<usize>)>> = Signal::global(|| None);

/// Name of the default style of a kind.
pub fn default_name(lang: Lang, kind: &str) -> String {
    tf(lang, "styles.default_of", &[("kind", &annot_name(lang, kind))])
}

/// The kind the pane shows: the selected comment's, the tool's, or the one chosen.
pub fn current_kind(st: AppState) -> String {
    let of_comment = st
        .read_active(|t| {
            let (p, i) = t.selected_annot?;
            let a = t.doc.annotations(p).ok()?.into_iter().find(|a| a.index == i)?;
            let style = t.doc.annotation_style(p, i).ok()?;
            Some(kind_of(&a.subtype, &style))
        })
        .flatten();
    of_comment
        .or_else(|| tool_subtype(*st.tool.peek()).map(str::to_string))
        .unwrap_or_else(|| STYLES.peek().kind.clone())
}

/// The style chosen in the pane: its name and look.
fn chosen(st: AppState) -> Option<(Option<usize>, String, folitecta_core::AnnotStyle)> {
    let view = STYLES.peek().clone();
    let kind = view.kind.clone();
    match view.chosen? {
        None => Some((None, kind.clone(), default_style(st, &kind))),
        Some(i) => {
            let n = named_styles(st).get(i).cloned()?;
            Some((Some(i), n.kind.clone(), n.style()))
        }
    }
}

fn need_choice(st: AppState) -> Option<(Option<usize>, String, folitecta_core::AnnotStyle)> {
    let c = chosen(st);
    if c.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.choose_style"), "");
        show_pane(st, Pane::Other("comment-styles".into()));
    }
    c
}

/// Apply the chosen style to the selected comment, or make it the default
/// of its kind when no comment is selected.
fn apply_style(st: AppState) {
    let Some((_, kind, style)) = need_choice(st) else {
        return;
    };
    let selected = st.read_active(|t| t.selected_annot).flatten();
    match selected {
        Some((p, i)) => {
            let r = st.with_active(|t| {
                let r = t.doc.set_annotation_style(p, i, &style);
                t.selected_annot = Some((p, i));
                r
            });
            match r {
                Some(Err(e)) => st.error(e),
                _ => {
                    st.with_active(|t| t.selected_annot = Some((p, i)));
                    crate::files::set_window_title(st);
                }
            }
        }
        None => {
            set_default_style(st, &kind, &style);
            st.toast(ToastKind::Ok, t(st.lang(), "style.default_saved"), "");
        }
    }
}

/// A new named style from the chosen one.
fn clone_style(st: AppState) {
    let Some((index, kind, style)) = need_choice(st) else {
        return;
    };
    let lang = st.lang();
    let base = match index {
        Some(i) => named_styles(st)
            .get(i)
            .map(|n| n.name.clone())
            .unwrap_or_default(),
        None => default_name(lang, &kind),
    };
    let mut list = named_styles(st);
    list.push(NamedStyle::new(
        tf(lang, "styles.copy_of", &[("name", &base)]),
        &kind,
        &style,
    ));
    save_named_styles(st, &list);
    let i = list.len() - 1;
    STYLES.write().chosen = Some(Some(i));
    st.set_dialog(Dialog::StyleName(i));
}

fn delete_style(st: AppState) {
    match need_choice(st) {
        Some((Some(i), _, _)) => {
            let mut list = named_styles(st);
            if i < list.len() {
                list.remove(i);
                save_named_styles(st, &list);
            }
            STYLES.write().chosen = None;
            *EDITING.write() = None;
        }
        Some((None, _, _)) => st.toast(ToastKind::Info, t(st.lang(), "toast.default_style_stays"), ""),
        None => {}
    }
}

fn rename_style(st: AppState) {
    match need_choice(st) {
        Some((Some(i), _, _)) => st.set_dialog(Dialog::StyleName(i)),
        Some((None, _, _)) => st.toast(ToastKind::Info, t(st.lang(), "toast.default_style_stays"), ""),
        None => {}
    }
}

/// Every style back to the built-in ones: named styles go.
fn reset_all(st: AppState) {
    save_named_styles(st, &[]);
    for kind in crate::styles::KINDS {
        reset_default_style(st, kind);
    }
    STYLES.write().chosen = None;
    *EDITING.write() = None;
    st.toast(ToastKind::Ok, t(st.lang(), "toast.styles_reset"), "");
}

/// The chosen style back to the built-in style of its kind.
fn reset_style(st: AppState) {
    match need_choice(st) {
        Some((None, kind, _)) => reset_default_style(st, &kind),
        Some((Some(i), kind, _)) => {
            let mut list = named_styles(st);
            if let Some(n) = list.get_mut(i) {
                *n = NamedStyle::new(n.name.clone(), &kind, &builtin(&kind));
                save_named_styles(st, &list);
            }
        }
        None => {}
    }
}

/// Edit the chosen style in the properties pane.
fn show_props(st: AppState) {
    let Some((index, kind, _)) = need_choice(st) else {
        return;
    };
    st.with_active(|t| t.selected_annot = None);
    *EDITING.write() = Some((kind, index));
    st.ui_mut().show_right_pane = true;
    st.save_prefs();
}

/// Make a style the default of the current tool (from the palette).
pub fn use_for_tool(st: AppState, kind: &str, index: Option<usize>) {
    let style = match index {
        Some(i) => match named_styles(st).get(i) {
            Some(n) => n.style(),
            None => return,
        },
        None => builtin(kind),
    };
    set_default_style(st, kind, &style);
}

handlers! {
    "commentStyles.applyStyle" => apply_style,
    "commentStyles.clone" => clone_style,
    "commentStyles.delete" => delete_style,
    "commentStyles.rename" => rename_style,
    "commentStyles.reset" => reset_all,
    "commentStyles.resetStyle" => reset_style,
    "commentStyles.showProps" => show_props,
}
