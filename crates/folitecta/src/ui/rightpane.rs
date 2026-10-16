//! Properties pane (right side).

use dioxus::prelude::*;

use crate::commands::dispatch;
use crate::i18n::t;
use crate::state::{AppState, Dialog};
use crate::util::human_size;

#[component]
pub fn RightPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let data = st.read_active(|tab| {
        let info = tab.doc.info();
        let size = tab.page_sizes.get(tab.page).copied();
        let annots = tab.doc.annotations(tab.page).map(|a| a.len()).unwrap_or(0);
        let links = tab.doc.links(tab.page).map(|l| l.len()).unwrap_or(0);
        let images = tab.doc.image_count(tab.page);
        let marks = tab.doc.marks_on_page(tab.page);
        let selected = tab
            .selected_annot
            .and_then(|(p, i)| tab.doc.annotations(p).ok().and_then(|a| a.get(i).cloned()));
        (
            info,
            tab.page,
            size,
            annots,
            links,
            images,
            marks,
            selected,
            tab.doc.can_undo(),
            tab.doc.can_redo(),
        )
    });
    rsx! {
        div { class: "right-pane",
            div { class: "pane-header", "{t(lang, \"pane.properties\")}" div { class: "spacer" }
                button { class: "ibtn", onclick: move |_| { st.ui_mut().show_right_pane = false; st.save_prefs(); }, "×" }
            }
            div { class: "pane-body",
                if let Some((info, page, size, annots, links, images, marks, selected, can_undo, can_redo)) = data {
                    div { class: "pane-header", style: "border:none;padding:4px 8px", "{t(lang, \"props.document\")}" }
                    div { class: "kv",
                        span { class: "k", "{t(lang, \"dialog.props.title_field\")}" } span { class: "v", "{info.title}" }
                        span { class: "k", "{t(lang, \"dialog.props.author\")}" } span { class: "v", "{info.author}" }
                        span { class: "k", "{t(lang, \"dialog.props.pages\")}" } span { class: "v", "{info.page_count}" }
                        span { class: "k", "{t(lang, \"dialog.props.size\")}" } span { class: "v", "{human_size(info.file_size)}" }
                        span { class: "k", "{t(lang, \"dialog.props.version\")}" } span { class: "v", "PDF {info.version}" }
                        span { class: "k", "{t(lang, \"dialog.props.form\")}" } span { class: "v", if info.has_form { "{t(lang, \"label.yes\")}" } else { "{t(lang, \"label.no\")}" } }
                        span { class: "k", "Undo / Redo" } span { class: "v", "{can_undo} / {can_redo}" }
                    }
                    button { class: "tbtn", style: "margin:4px 8px", onclick: move |_| st.set_dialog(Dialog::DocProps), "{t(lang, \"cmd.docProps\")}" }
                    div { class: "pane-header", style: "border:none;padding:10px 8px 4px", "{t(lang, \"props.page\")} {page + 1}" }
                    if let Some(s) = size {
                        div { class: "kv",
                            span { class: "k", "{t(lang, \"props.size\")}" }
                            span { class: "v", "{s.width:.0} × {s.height:.0} pt ({s.width / 72.0 * 25.4:.0} × {s.height / 72.0 * 25.4:.0} mm)" }
                            span { class: "k", "{t(lang, \"props.rotation\")}" } span { class: "v", "{s.rotation}°" }
                            span { class: "k", "{t(lang, \"props.annotations\")}" } span { class: "v", "{annots}" }
                            span { class: "k", "{t(lang, \"props.links\")}" } span { class: "v", "{links}" }
                            span { class: "k", "{t(lang, \"props.images\")}" } span { class: "v", "{images}" }
                            span { class: "k", "{t(lang, \"props.marks\")}" } span { class: "v", if marks.is_empty() { "{t(lang, \"label.none\")}" } else { "{marks.join(\", \")}" } }
                        }
                    }
                    if let Some(a) = selected {
                        div { class: "pane-header", style: "border:none;padding:10px 8px 4px", "{t(lang, \"props.selection\")}" }
                        div { class: "kv",
                            span { class: "k", "Type" } span { class: "v", "{a.subtype}" }
                            span { class: "k", "{t(lang, \"dialog.note.author\")}" } span { class: "v", "{a.author}" }
                            span { class: "k", "{t(lang, \"dialog.props.modified\")}" } span { class: "v", "{a.modified}" }
                            span { class: "k", "Text" } span { class: "v", "{a.contents}" }
                        }
                        div { class: "row", style: "padding: 4px 8px; gap: 6px",
                            button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::AnnotationText { page: a.page, index: a.index, contents: a.contents.clone() }), "Edit text…" }
                            button { class: "tbtn danger", onclick: move |_| dispatch(st, "comments.deleteSel"), "{t(lang, \"cmd.delete\")}" }
                        }
                    }
                } else {
                    div { class: "pane-empty", "{t(lang, \"toast.no_document\")}" }
                }
            }
        }
    }
}
