//! Comment format: properties pane, style copy/paste, defaults, text alignment and size.

use dioxus::prelude::ReadableExt;
use folitecta_core::AnnotStyle;

use crate::commands::{handlers, no_doc};
use crate::files;
use crate::i18n::t;
use crate::state::comment_styles::{default_style, kind_of, set_default_style, tool_subtype};
use crate::state::{AppState, ToastKind};

/// Open the properties pane, where the style editor lives.
fn show_properties(st: AppState) {
    if !st.ui.peek().show_right_pane {
        st.ui_mut().show_right_pane = true;
        st.save_prefs();
    }
}

/// The selected comment and its style.
fn selected(st: AppState) -> Option<(usize, usize, String, AnnotStyle)> {
    st.read_active(|t| {
        let (p, i) = t.selected_annot?;
        let a = t.doc.annotations(p).ok()?.into_iter().find(|a| a.index == i)?;
        let style = t.doc.annotation_style(p, i).ok()?;
        Some((p, i, a.subtype, style))
    })
    .flatten()
}

/// Change the selected comment's style, or the active tool's defaults.
fn edit_style(st: AppState, change: impl FnOnce(&mut AnnotStyle)) {
    if let Some((p, i, _, mut style)) = selected(st) {
        change(&mut style);
        let r = st.with_active(|t| {
            let r = t.doc.set_annotation_style(p, i, &style);
            t.selected_annot = Some((p, i));
            r
        });
        match r {
            Some(Err(e)) => st.error(e),
            _ => {
                st.with_active(|t| t.selected_annot = Some((p, i)));
                files::set_window_title(st);
            }
        }
    } else if let Some(kind) = tool_subtype((st.tool)()) {
        let mut style = default_style(st, kind);
        change(&mut style);
        set_default_style(st, kind, &style);
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), "");
    }
}

fn copy_style(st: AppState) {
    match selected(st) {
        Some((.., style)) => {
            st.set_style_clip(Some(style));
            st.toast(ToastKind::Ok, t(st.lang(), "style.copied"), "");
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), ""),
    }
}

fn paste_style(st: AppState, appearance_only: bool) {
    let Some(clip) = st.style_clip.peek().clone() else {
        st.toast(ToastKind::Info, t(st.lang(), "style.nothing_copied"), "");
        return;
    };
    edit_style(st, |s| {
        let keep_text = (s.font_size, s.text_color, s.align);
        *s = clip;
        if appearance_only {
            // Keep the text formatting of the target.
            (s.font_size, s.text_color, s.align) = keep_text;
        }
    });
}

fn make_default(st: AppState) {
    match selected(st) {
        Some((_, _, subtype, style)) => {
            set_default_style(st, &kind_of(&subtype, &style), &style);
            st.toast(ToastKind::Ok, t(st.lang(), "style.default_saved"), "");
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), ""),
    }
}

fn apply_default(st: AppState) {
    match selected(st) {
        Some((_, _, subtype, style)) => {
            let d = default_style(st, &kind_of(&subtype, &style));
            edit_style(st, |s| *s = d);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), ""),
    }
}

fn align(st: AppState, a: u8) {
    if !no_doc(st) {
        edit_style(st, |s| s.align = a);
    }
}

fn resize_text(st: AppState, delta: f32) {
    if !no_doc(st) {
        edit_style(st, |s| s.font_size = (s.font_size + delta).clamp(4.0, 200.0));
    }
}

/// Comments › Note Icon: the icon of the selected note, or of the new notes.
fn note_icon(st: AppState) {
    let target = selected(st)
        .filter(|(_, _, subtype, _)| subtype == "Text")
        .map(|(p, i, _, _)| (p, i));
    st.set_dialog(crate::state::Dialog::NoteIcon(target));
}

handlers! {
    "noteIcon" => note_icon,
    "props" => show_properties,
    "props.makeDefault" => make_default,
    "props.applyDefault" => apply_default,
    "annotStyle.copy" => copy_style,
    "annotStyle.paste" => |st| paste_style(st, false),
    "annotStyle.pasteAppOnly" => |st| paste_style(st, true),
    "textStyle.copy" => copy_style,
    "textStyle.paste" => |st| paste_style(st, false),
    "textStyle.makeDefault" => make_default,
    "textStyle.applyDefault" => apply_default,
    "textStyle.clear" => apply_default,
    "textStyle.color" => show_properties,
    "textStyle.sizeIn" => |st| resize_text(st, 2.0),
    "textStyle.sizeOut" => |st| resize_text(st, -2.0),
    "textAlign.left" => |st| align(st, 0),
    "textAlign.center" => |st| align(st, 1),
    "textAlign.right" => |st| align(st, 2),
    "scolor" => show_properties,
    "fcolor" => show_properties,
    "opacity" => show_properties,
    "borderWidth" => show_properties,
    "borderStyle" => |st| { if !no_doc(st) { edit_style(st, |s| s.dashed = !s.dashed) } },
    "lineStart" => show_properties,
    "lineEnd" => show_properties,
    "fontSize" => show_properties,
    "fontName" => show_properties,
}
