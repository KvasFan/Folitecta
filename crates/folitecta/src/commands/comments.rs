//! Comments: annotation tools, markup on text selections and search results,
//! comment management, links and stamps.

use super::{confirm, core_result, doc_dialog, doc_op, handlers, no_doc, show_pane};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, ConfirmAction, Dialog, Pane, ToastKind, Tool};
use dioxus::prelude::ReadableExt;
use folitecta_core::Markup;

fn tool(st: AppState, tool: Tool) {
    super::select_tool(st, tool);
}

/// The link tool, the links drawn next doing `kind` (web, page, file, script).
fn link_tool(st: AppState, kind: &str) {
    st.set_value("link.kind", kind);
    tool(st, Tool::Link);
}

fn export_measures(st: AppState) {
    if no_doc(st) {
        return;
    }
    let rows = st.read_active(|t| t.doc.measurements()).unwrap_or_default();
    if rows.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_measurements"), "");
        return;
    }
    let mut csv = String::from("page,kind,value\n");
    for (page, kind, value) in rows {
        csv.push_str(&format!(
            "{},{},\"{}\"\n",
            page + 1,
            kind,
            value.replace('"', "'")
        ));
    }
    files::save_text_file(st, "measurements.csv", csv);
}

/// Insert mark after the text selection (or replacement of the selection).
fn selection_mark(st: AppState, replace: bool) {
    if no_doc(st) {
        return;
    }
    match st.read_active(|t| t.text_sel).flatten().filter(|s| !s.is_empty()) {
        Some(sel) => {
            let (start, end) = sel.range();
            st.set_dialog(if replace {
                Dialog::ReplaceText {
                    page: sel.page,
                    start,
                    end,
                    note: true,
                }
            } else {
                Dialog::InsertText {
                    page: sel.page,
                    index: end.saturating_sub(1),
                    after: true,
                    note: true,
                }
            });
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_text_first"), ""),
    }
}

fn step_comment(st: AppState, forward: bool) {
    if no_doc(st) {
        return;
    }
    let target = st
        .read_active(|t| {
            let comments = t.doc.comments();
            if comments.is_empty() {
                return None;
            }
            let pos = t
                .selected_annot
                .and_then(|(p, i)| comments.iter().position(|a| a.page == p && a.index == i));
            let next = match (pos, forward) {
                (Some(i), true) => (i + 1) % comments.len(),
                (Some(i), false) => (i + comments.len() - 1) % comments.len(),
                (None, true) => comments.iter().position(|a| a.page >= t.page).unwrap_or(0),
                (None, false) => comments.len() - 1,
            };
            Some((comments[next].page, comments[next].index))
        })
        .flatten();
    match target {
        Some((page, index)) => {
            st.with_active(|t| t.selected_annot = Some((page, index)));
            st.go_to_page(page);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_comments"), ""),
    }
}

fn summarize(st: AppState) {
    if no_doc(st) {
        return;
    }
    if st.read_active(|t| t.doc.comments().is_empty()).unwrap_or(true) {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_comments"), "");
        return;
    }
    st.set_dialog(Dialog::SummarizeComments);
}

fn export_selected(st: AppState) {
    if no_doc(st) {
        return;
    }
    match st.read_active(|t| t.selected_annot).flatten() {
        Some(sel) => files::export_comments(st, Some(vec![sel])),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), ""),
    }
}

fn set_all_hidden(st: AppState, hidden: Option<bool>) {
    doc_op(st, |t| {
        let items = t.doc.comments();
        let hide = hidden.unwrap_or_else(|| items.iter().all(|a| !a.hidden));
        for a in items {
            t.doc.set_annotation_hidden(a.page, a.index, hide)?;
        }
        Ok(())
    });
}

/// Colour of new text markups of a kind (the user's default style).
pub(crate) fn markup_color(st: AppState, k: Markup) -> [f32; 3] {
    let kind = match k {
        Markup::Highlight => "Highlight",
        Markup::Underline => "Underline",
        Markup::StrikeOut => "StrikeOut",
        Markup::Squiggly => "Squiggly",
    };
    crate::state::comment_styles::default_style(st, kind)
        .color
        .unwrap_or([1.0, 0.9, 0.2])
}

/// Apply a markup to the current text selection.
pub(crate) fn markup_selection(st: AppState, kind: Option<Markup>) {
    if no_doc(st) {
        return;
    }
    let Some(sel) = st.read_active(|t| t.text_sel).flatten().filter(|s| !s.is_empty()) else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_text_first"), "");
        return;
    };
    let author = st.ui.read().author.clone();
    let (start, end) = sel.range();
    let r = st.with_active(|t| {
        let r = match kind {
            Some(k) => {
                let color = markup_color(st, k);
                t.doc.markup_text_range(sel.page, start, end, k, color, &author)
            }
            None => t.doc.redact_text_range(sel.page, start, end, &author).map(|_| ()),
        };
        t.text_sel = None;
        r
    });
    core_result(st, r);
}

/// Apply a markup (or redaction) to every result of the current search.
pub(crate) fn markup_results(st: AppState, kind: Option<Markup>) {
    if no_doc(st) {
        return;
    }
    let query = st.read_active(|t| t.search.query.clone()).unwrap_or_default();
    if query.trim().is_empty() {
        show_pane(st, Pane::Search);
        st.toast(ToastKind::Info, t(st.lang(), "toast.search_first"), "");
        return;
    }
    let (opts, proximity, _) = super::search::search_settings(st);
    let author = st.ui.read().author.clone();
    let color = kind.map(|k| markup_color(st, k));
    let r = st.with_active(|t| {
        // The results shown, found again when the document changed since.
        let hits = if t.search.hits.is_empty() {
            t.doc.search_words(&query, opts, proximity)
        } else {
            t.search.hits.clone()
        };
        match (kind, color) {
            (Some(k), Some(c)) => t.doc.markup_hits(&hits, k, c, &author),
            _ => t.doc.redact_hits(&hits, &author),
        }
    });
    match r {
        Some(Ok(n)) => {
            files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.marked_results", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn create_links(st: AppState) {
    if no_doc(st) {
        return;
    }
    let r = st.with_active(|t| {
        let n = t.doc.page_count();
        t.doc.create_links_from_urls(&(0..n).collect::<Vec<_>>())
    });
    match r {
        Some(Ok(n)) => {
            files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.links_created", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn copy_selection_text(st: AppState) {
    let text = st
        .read_active(|t| {
            t.text_sel.filter(|s| !s.is_empty()).map(|s| {
                let (a, b) = s.range();
                t.doc.text_range(s.page, a, b)
            })
        })
        .flatten();
    match text {
        Some(s) => {
            crate::clipboard::set_text(&s);
            st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), "");
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_text_first"), ""),
    }
}

fn flatten_selected(st: AppState) {
    match st.read_active(|t| t.selected_annot).flatten() {
        Some((p, i)) => doc_op(st, |t| t.doc.flatten_annotation(p, i)),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), ""),
    }
}

handlers! {
    // ---- Tools ------------------------------------------------------------
    "tool.hand" => |st| tool(st, Tool::Hand),
    "tool.selectText" => |st| tool(st, Tool::Select),
    "tool.select" => |st| tool(st, Tool::Select),
    "tool.hand.selTextImages" => |st| tool(st, Tool::Select),
    "tool.editAnnots" => |st| tool(st, Tool::EditAnnots),
    "tool.zoom" => |st| tool(st, Tool::Zoom),
    "tool.snapshot" => |st| tool(st, Tool::Snapshot),
    "addStickyNote" => |st| tool(st, Tool::Note),
    "tool.annot.stickyNote" => |st| tool(st, Tool::Note),
    "tool.annot.square" => |st| tool(st, Tool::Rect),
    "tool.annot.circle" => |st| tool(st, Tool::Ellipse),
    "tool.annot.line" => |st| tool(st, Tool::Line),
    "tool.annot.arrow" => |st| tool(st, Tool::Arrow),
    "tool.annot.highlightArea" => |st| tool(st, Tool::Highlight),
    "tool.annot.textBox" => |st| tool(st, Tool::TextBox),
    "tool.annot.typeWriter" => |st| tool(st, Tool::Typewriter),
    "tool.addText" => |st| tool(st, Tool::Typewriter),
    "tool.annot.pencil" => |st| tool(st, Tool::Ink),
    "tool.annot.editLink" => |st| tool(st, Tool::Link),
    "addLink" => |st| tool(st, Tool::Link),
    "addLink.uri" => |st| link_tool(st, "web"),
    "addLink.goto" => |st| link_tool(st, "page"),
    "addLink.launch" => |st| link_tool(st, "file"),
    "addLink.custom" => |st| link_tool(st, "script"),
    "tool.annot.stamp" => |st| doc_dialog(st, Dialog::StampPicker),
    "stampsList" => |st| doc_dialog(st, Dialog::StampPicker),
    "tool.annot.highlight" => |st| tool(st, Tool::TextHighlight),
    "tool.annot.polygon" => |st| tool(st, Tool::Polygon),
    "tool.annot.polyline" => |st| tool(st, Tool::Polyline),
    "tool.annot.cloud" => |st| tool(st, Tool::Cloud),
    "tool.annot.callout" => |st| tool(st, Tool::Callout),
    "tool.annot.distance" => |st| tool(st, Tool::Distance),
    "tool.annot.perimeter" => |st| tool(st, Tool::Perimeter),
    "tool.annot.area" => |st| tool(st, Tool::Area),
    "tool.annot.freeHighlight" => |st| tool(st, Tool::FreeHighlight),
    "tool.annot.eraser" => |st| tool(st, Tool::Eraser),
    "tool.annot.insertText" => |st| tool(st, Tool::InsertText),
    "tool.annot.replaceText" => |st| tool(st, Tool::ReplaceText),
    "tool.annot.fileAttachment" => |st| tool(st, Tool::FileAttachment),
    "tool.calibrate" => |st| doc_dialog(st, Dialog::Calibrate),
    "annot.measure.setScale" => |st| doc_dialog(st, Dialog::Calibrate),
    "measure.precision" => |st| doc_dialog(st, Dialog::Calibrate),
    "measure.decimalSeparator" => |st| doc_dialog(st, Dialog::Calibrate),
    "measure.groupSeparator" => |st| doc_dialog(st, Dialog::Calibrate),
    "exportMeasuresToCSV" => export_measures,
    "textsel.addInsertTextNote" => |st| selection_mark(st, false),
    "textsel.addReplaceTextNote" => |st| selection_mark(st, true),
    "tool.annot.underline" => |st| tool(st, Tool::TextUnderline),
    "tool.annot.strikeout" => |st| tool(st, Tool::TextStrike),
    // ---- Markup on the text selection ---------------------------------------
    "textsel.addHighlight" => |st| markup_selection(st, Some(Markup::Highlight)),
    "textsel.addUnderline" => |st| markup_selection(st, Some(Markup::Underline)),
    "textsel.addStrikeout" => |st| markup_selection(st, Some(Markup::StrikeOut)),
    "addAnnotBySelect.highlight" => |st| markup_selection(st, Some(Markup::Highlight)),
    "addAnnotBySelect.underline" => |st| markup_selection(st, Some(Markup::Underline)),
    "addAnnotBySelect.strikeout" => |st| markup_selection(st, Some(Markup::StrikeOut)),
    "addAnnotBySelect.redaction" => |st| markup_selection(st, None),
    "tool.selectText.copyAsRich" => copy_selection_text,
    // ---- Markup on search results -------------------------------------------
    "addAnnotsBySearchResult.highlight" => |st| markup_results(st, Some(Markup::Highlight)),
    "addAnnotsBySearchResult.underline" => |st| markup_results(st, Some(Markup::Underline)),
    "addAnnotsBySearchResult.strikeout" => |st| markup_results(st, Some(Markup::StrikeOut)),
    "addAnnotsBySearchResult.redaction" => |st| markup_results(st, None),
    "addAnnotsByFindResult.highlight" => |st| markup_results(st, Some(Markup::Highlight)),
    "addAnnotsByFindResult.underline" => |st| markup_results(st, Some(Markup::Underline)),
    "addAnnotsByFindResult.strikeout" => |st| markup_results(st, Some(Markup::StrikeOut)),
    "addAnnotsByFindResult.redaction" => |st| markup_results(st, None),
    // ---- Manage comments ----------------------------------------------------
    "comments.deleteAllDoc" => |st| { if !no_doc(st) { confirm(st, "cmd.comments.deleteAllDoc", "dialog.confirm.delete_comments", ConfirmAction::DeleteAllComments) } },
    "comments.deleteAllShown" => |st| { if !no_doc(st) { confirm(st, "cmd.comments.deleteAllDoc", "dialog.confirm.delete_comments", ConfirmAction::DeleteAllComments) } },
    "comments.deleteSel" => |st| {
        if no_doc(st) { return; }
        match st.read_active(|t| t.selected_annot).flatten() {
            Some((p, i)) => core_result(st, st.with_active(|t| t.doc.delete_annotation(p, i))),
            None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), ""),
        }
    },
    "comments.next" => |st| step_comment(st, true),
    "comments.previous" => |st| step_comment(st, false),
    "commentsView.next" => |st| step_comment(st, true),
    "commentsView.previous" => |st| step_comment(st, false),
    "comments.hideAll" => |st| set_all_hidden(st, Some(true)),
    "comments.showAll" => |st| set_all_hidden(st, Some(false)),
    "comments.showhideAll" => |st| set_all_hidden(st, None),
    "comments.summarize" => summarize,
    "comments.export" => |st| { if !no_doc(st) { files::export_comments(st, None) } },
    "comments.import" => |st| { if !no_doc(st) { files::import_comments(st) } },
    "commentsView.exportSelected" => export_selected,
    "comments.flatten" => |st| doc_dialog(st, Dialog::Flatten),
    "comments.flattenGroup" => |st| doc_dialog(st, Dialog::Flatten),
    "comments.flattenSel" => flatten_selected,
    // ---- Links --------------------------------------------------------------
    "removeAllWebLinks" => |st| { if !no_doc(st) { confirm(st, "cmd.removeAllWebLinks", "dialog.confirm.delete_links", ConfirmAction::RemoveLinks) } },
    "createWebLinksFromText" => create_links,
    "linksView.exportToCSV" => |st| {
        if no_doc(st) { return; }
        let csv = st.read_active(|t| { let mut s = String::from("page,target,x0,y0,x1,y1\n"); for l in t.doc.all_links() { s.push_str(&format!("{},\"{}\",{},{},{},{}\n", l.page + 1, l.target.describe().replace('"', "'"), l.rect[0], l.rect[1], l.rect[2], l.rect[3])); } s }).unwrap_or_default();
        files::save_text_file(st, "links.csv", csv);
    },
}
