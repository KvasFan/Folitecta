//! Document tools: stamps from pages, bookmark navigation, splitting at
//! guides, cropped content, page-mark management, space audit, image
//! recompression, inserted text, highlights from a file, PDF/A, repeated
//! signatures and document actions.

use base64::Engine;
use dioxus::prelude::*;
use folitecta_core::Document;

use crate::commands::{core_result, doc_dialog, handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, Pane, ToastKind, Tool};

/// Pages a tool works on: the selected pages, or every page.
pub fn target_pages(st: AppState) -> Vec<usize> {
    st.read_active(|t| {
        if t.selected_pages.is_empty() {
            (0..t.doc.page_count()).collect()
        } else {
            t.selected_pages.clone()
        }
    })
    .unwrap_or_default()
}

/// Add a picture to the stamp library and show it.
pub fn picture_stamp(st: AppState, png: Vec<u8>, name: String) {
    crate::commands::stamps::add_stamp(
        st,
        crate::commands::stamps::StampItem {
            name,
            kind: "image".into(),
            image: base64::engine::general_purpose::STANDARD.encode(png),
            ..Default::default()
        },
    );
    crate::commands::show_pane(st, Pane::Stamps);
    st.toast(ToastKind::Ok, t(st.lang(), "toast.stamp_saved"), "");
}

fn stamp_from_page(st: AppState) {
    if no_doc(st) {
        return;
    }
    let r = st.read_active(|t| {
        let size = t.doc.page_size(t.page)?;
        let scale = (600.0 / size.width.max(size.height)).clamp(0.2, 3.0);
        let png = t
            .doc
            .render_page_png_with_view(t.page, scale, &t.view_layer_states())?
            .png;
        Ok::<_, folitecta_core::Error>((png, format!("{} {}", t.doc.display_name(), t.page + 1)))
    });
    match r {
        Some(Ok((png, name))) => picture_stamp(st, png, name),
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// The snapshot tool makes a stamp instead of copying to the clipboard.
fn stamp_from_selection(st: AppState) {
    if no_doc(st) {
        return;
    }
    st.set_value("snapshot.target", "stamp");
    st.set_tool(Tool::Snapshot);
    st.toast(ToastKind::Info, t(st.lang(), "toast.stamp_from_area"), "");
}

fn bookmark_step(st: AppState, forward: bool) {
    if no_doc(st) {
        return;
    }
    let target = st
        .read_active(|t| {
            let mut pages: Vec<usize> = t
                .doc
                .outline_flat()
                .into_iter()
                .filter_map(|(_, _, p)| p)
                .collect();
            pages.sort_unstable();
            pages.dedup();
            if forward {
                pages.into_iter().find(|p| *p > t.page)
            } else {
                pages.into_iter().rev().find(|p| *p < t.page)
            }
        })
        .flatten();
    match target {
        Some(p) => st.navigate(p, true),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_more_bookmarks"), ""),
    }
}

fn split_by_guides(st: AppState) {
    if no_doc(st) {
        return;
    }
    let (xs, ys, pages) = st
        .read_active(|t| {
            let on_page = |g: &&crate::state::Guide| g.page.is_none() || g.page == Some(t.page);
            let xs: Vec<f32> = t
                .guides
                .iter()
                .filter(on_page)
                .filter(|g| !g.horizontal)
                .map(|g| g.pos)
                .collect();
            let ys: Vec<f32> = t
                .guides
                .iter()
                .filter(on_page)
                .filter(|g| g.horizontal)
                .map(|g| g.pos)
                .collect();
            let pages = if t.selected_pages.is_empty() {
                vec![t.page]
            } else {
                t.selected_pages.clone()
            };
            (xs, ys, pages)
        })
        .unwrap_or_default();
    if xs.is_empty() && ys.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.add_guides_first"), "");
        st.set_dialog(Dialog::AddGuides);
        return;
    }
    match st.with_active(|t| t.doc.split_pages_at(&pages, &xs, &ys)) {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.pages_split", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn remove_cropped(st: AppState) {
    if no_doc(st) {
        return;
    }
    let pages = target_pages(st);
    match st.with_active(|t| t.doc.remove_cropped_content(&pages)) {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.cropped_removed", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn insert_rtf(st: AppState) {
    if no_doc(st) {
        return;
    }
    spawn(async move {
        let Some(f) = rfd::AsyncFileDialog::new()
            .add_filter("RTF", &["rtf"])
            .pick_file()
            .await
        else {
            return;
        };
        let data = f.read().await;
        let text = folitecta_core::doctools::rtf_to_text(&String::from_utf8_lossy(&data));
        insert_text_pages(st, &text);
    });
}

/// Insert pages with `text` after the current page.
pub fn insert_text_pages(st: AppState, text: &str) {
    if text.trim().is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.text_empty"), "");
        return;
    }
    let size = st.read_active(|t| t.doc.page_size(t.page).ok()).flatten();
    let mut layout = folitecta_core::TextLayout::default();
    if let Some(s) = size {
        layout.page_width = s.width;
        layout.page_height = s.height;
    }
    let other = match Document::from_text(text, layout) {
        Ok(d) => d,
        Err(e) => return st.error(e),
    };
    let n = other.page_count();
    let r = st.with_active(|t| t.doc.insert_document(t.page + 1, &other, None));
    core_result(st, r);
    st.toast(
        ToastKind::Ok,
        tf(st.lang(), "toast.pages_inserted", &[("count", &n.to_string())]),
        "",
    );
}

/// Highlight every phrase listed (one per line) in a text file.
fn highlight_from_file(st: AppState) {
    if no_doc(st) {
        return;
    }
    spawn(async move {
        let Some(f) = rfd::AsyncFileDialog::new()
            .add_filter("Text", &["txt", "csv"])
            .pick_file()
            .await
        else {
            return;
        };
        let data = f.read().await;
        let text = String::from_utf8_lossy(&data).to_string();
        let phrases: Vec<String> = text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let (opts, _, _) = crate::commands::search::search_settings(st);
        let author = st.ui.peek().author.clone();
        let color =
            crate::util::parse_color(st.ui.peek().value("style.Highlight.color").unwrap_or("#ffeb3b"));
        let r = st.with_active(|t| {
            let mut hits = Vec::new();
            for p in &phrases {
                hits.extend(t.doc.search(p, opts));
            }
            if hits.is_empty() {
                return Ok(0);
            }
            t.doc
                .markup_hits(&hits, folitecta_core::Markup::Highlight, color, &author)
        });
        match r {
            Some(Ok(n)) => {
                crate::files::set_window_title(st);
                st.toast(
                    ToastKind::Ok,
                    tf(
                        st.lang(),
                        "toast.phrases_highlighted",
                        &[("count", &n.to_string()), ("phrases", &phrases.len().to_string())],
                    ),
                    "",
                );
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

fn discard_pdfa(st: AppState) {
    if no_doc(st) {
        return;
    }
    match st.with_active(|t| t.doc.discard_pdfa()) {
        Some(Ok(true)) => {
            crate::files::set_window_title(st);
            st.toast(ToastKind::Ok, t(st.lang(), "toast.pdfa_discarded"), "");
        }
        Some(Ok(false)) => st.toast(ToastKind::Info, t(st.lang(), "toast.not_pdfa"), ""),
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn multi_place(st: AppState) {
    if no_doc(st) {
        return;
    }
    let ok = st
        .read_active(|t| {
            t.selected_annot
                .is_some_and(|s| t.doc.signature_marks().contains(&s))
        })
        .unwrap_or(false);
    if ok {
        st.set_dialog(Dialog::MultiPlace);
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_signature_mark"), "");
    }
}

handlers! {
    "document.createStampFromActiveDoc" => stamp_from_page,
    "document.createStampFromSel" => stamp_from_selection,
    "document.nextBookm" => |st| bookmark_step(st, true),
    "document.prevBookm" => |st| bookmark_step(st, false),
    "document.splitPagesByGuides" => split_by_guides,
    "document.removeCroppedContent" => remove_cropped,
    "document.watermarks.manage" => |st| doc_dialog(st, Dialog::ManageMarks("Watermark".into())),
    "document.headerAndFooter.manage" => |st| doc_dialog(st, Dialog::ManageMarks("HeaderFooter".into())),
    "document.batesNumbering.manage" => |st| doc_dialog(st, Dialog::ManageMarks("PageNumber".into())),
    "document.audit" => |st| doc_dialog(st, Dialog::SpaceAudit),
    "document.recompressImages" => |st| doc_dialog(st, Dialog::Recompress(false)),
    "document.recompressImage" => |st| doc_dialog(st, Dialog::Recompress(true)),
    "document.insertText" => |st| doc_dialog(st, Dialog::InsertTextPages),
    "document.insertRTF" => insert_rtf,
    "document.highlightFile" => highlight_from_file,
    "document.discardPDFA" => discard_pdfa,
    "document.signature.multiPlace" => multi_place,
    "document.actions" => |st| doc_dialog(st, Dialog::DocumentActions),
}
