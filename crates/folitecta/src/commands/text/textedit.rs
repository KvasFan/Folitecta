//! Changing the text of the pages: text inserted, deleted or replaced in the
//! page content (not a comment about it), and the tools that do it.

use dioxus::prelude::*;

use crate::commands::{handlers, no_doc, select_tool};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, TextSel, ToastKind, Tool};

/// Text typed on the page before the dialog that takes it opened.
pub static TYPED: GlobalSignal<String> = Signal::global(String::new);

/// Replace characters `start..end` of a page with `text` in the page content.
pub fn edit_text(st: AppState, page: usize, start: usize, end: usize, text: &str) -> bool {
    let r = st.with_active(|t| t.doc.edit_page_text(page, start, end, text));
    match r {
        Some(Ok(report)) => {
            crate::files::set_window_title(st);
            if !report.substituted.is_empty() {
                st.toast(
                    ToastKind::Info,
                    tf(
                        st.lang(),
                        "toast.text_substituted",
                        &[("chars", &report.substituted)],
                    ),
                    "",
                );
            }
            true
        }
        Some(Err(e)) => {
            st.error(e);
            false
        }
        None => false,
    }
}

fn selection(st: AppState) -> Option<TextSel> {
    st.read_active(|t| t.text_sel).flatten()
}

/// Delete the selected text from the page.
pub fn delete_selection(st: AppState) {
    if no_doc(st) {
        return;
    }
    match selection(st).filter(|s| !s.is_empty()) {
        Some(sel) => {
            let (a, b) = sel.range();
            edit_text(st, sel.page, a, b, "");
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_text_first"), ""),
    }
}

/// Replace the selected text: a dialog with the text to put instead.
pub fn replace_selection(st: AppState) {
    if no_doc(st) {
        return;
    }
    match selection(st).filter(|s| !s.is_empty()) {
        Some(sel) => {
            let (start, end) = sel.range();
            st.set_dialog(Dialog::ReplaceText {
                page: sel.page,
                start,
                end,
                note: false,
            });
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_text_first"), ""),
    }
}

/// A key pressed with the edit text tool: Delete and Backspace delete,
/// characters start the text that goes in. Returns whether it was used.
pub fn key(st: AppState, key: &Key) -> bool {
    if *st.tool.peek() != Tool::EditText {
        return false;
    }
    let Some(sel) = selection(st) else { return false };
    let (a, b) = sel.range();
    match key {
        Key::Delete | Key::Backspace => {
            let range = if a < b {
                Some((a, b))
            } else if *key == Key::Delete {
                Some((a, a + 1))
            } else {
                a.checked_sub(1).map(|p| (p, a))
            };
            if let Some((x, y)) = range {
                if edit_text(st, sel.page, x, y, "") {
                    // The caret stays where the text was.
                    st.with_active(|t| {
                        t.text_sel = Some(TextSel {
                            page: sel.page,
                            anchor: x,
                            head: x,
                        })
                    });
                }
            }
            true
        }
        Key::Character(c) if !c.is_empty() => {
            *TYPED.write() = c.clone();
            st.set_dialog(if a < b {
                Dialog::ReplaceText {
                    page: sel.page,
                    start: a,
                    end: b,
                    note: false,
                }
            } else {
                Dialog::InsertText {
                    page: sel.page,
                    index: a,
                    after: false,
                    note: false,
                }
            });
            true
        }
        _ => false,
    }
}

fn text_tool(st: AppState, tool: Tool) {
    if no_doc(st) {
        return;
    }
    select_tool(st, tool);
}

/// Insert text in the page at the caret (or before the selection); without
/// one, the edit text tool waits for a click in the text.
fn insert_at_caret(st: AppState) {
    if no_doc(st) {
        return;
    }
    match selection(st) {
        Some(sel) => {
            let (a, b) = sel.range();
            st.set_dialog(if a < b {
                Dialog::ReplaceText {
                    page: sel.page,
                    start: a,
                    end: b,
                    note: false,
                }
            } else {
                Dialog::InsertText {
                    page: sel.page,
                    index: a,
                    after: false,
                    note: false,
                }
            });
        }
        None => {
            select_tool(st, Tool::EditText);
            st.toast(ToastKind::Info, t(st.lang(), "toast.click_on_text"), "");
        }
    }
}

/// Replace or delete the selected text; without a selection, the edit text
/// tool is chosen so text can be selected.
fn on_selection(st: AppState, f: fn(AppState)) {
    if no_doc(st) {
        return;
    }
    if selection(st).is_some_and(|s| !s.is_empty()) {
        f(st);
    } else {
        select_tool(st, Tool::EditText);
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_text_first"), "");
    }
}

/// Check states of the text editing tools.
pub fn textedit_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "tool.editText" | "tool.editContent.text" => Some((st.tool)() == Tool::EditText),
        "tool.textCorrectionMarkup" => Some((st.tool)() == Tool::TextCorrection),
        _ => None,
    }
}

handlers! {
    "tool.editText" => |st| text_tool(st, Tool::EditText),
    "tool.editContent.text" => |st| text_tool(st, Tool::EditText),
    "tool.textCorrectionMarkup" => |st| text_tool(st, Tool::TextCorrection),
    "pageText.insert" => insert_at_caret,
    "pageText.replace" => |st| on_selection(st, replace_selection),
    "pageText.delete" => |st| on_selection(st, delete_selection),
}
