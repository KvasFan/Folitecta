//! Properties pane (right side).

use dioxus::prelude::*;

use crate::commands::dispatch;
use crate::i18n::{annot_name, t};
use crate::state::{AppState, Dialog};
use crate::util::human_size;

/// One "label: value" row that wraps the value under the label when narrow.
fn prop(label: String, value: String) -> Element {
    rsx! {
        div { class: "prop",
            span { class: "k", title: "{label}", "{label}" }
            span { class: "v", title: "{value}", "{value}" }
        }
    }
}

#[component]
pub fn RightPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let yes_no = move |b: bool| {
        if b {
            t(lang, "label.yes")
        } else {
            t(lang, "label.no")
        }
    };
    let data = st.read_active(|tab| {
        let info = tab.doc.info();
        let size = tab.page_sizes.get(tab.page).copied();
        let annots = tab.doc.annotations(tab.page).map(|a| a.len()).unwrap_or(0);
        let links = tab.doc.links(tab.page).map(|l| l.len()).unwrap_or(0);
        let images = tab.doc.image_count(tab.page);
        let marks = tab.doc.marks_on_page(tab.page);
        let label = tab.doc.page_labels().get(tab.page).cloned().unwrap_or_default();
        let selected = tab.selected_annot.and_then(|(p, i)| {
            tab.doc
                .annotations(p)
                .ok()
                .and_then(|a| a.into_iter().find(|x| x.index == i))
        });
        (
            info,
            tab.page,
            size,
            annots,
            links,
            images,
            marks,
            selected,
            tab.doc.is_dirty(),
            label,
        )
    });
    rsx! {
        div { class: "right-pane", onclick: move |e| e.stop_propagation(),
            div { class: "pane-header", span { class: "pane-title", "{t(lang, \"pane.properties\")}" } div { class: "spacer" }
                button { class: "ibtn", title: "{t(lang, \"dialog.close\")}", onclick: move |_| { st.ui_mut().show_right_pane = false; st.save_prefs(); }, "×" }
            }
            div { class: "pane-body props",
                if let Some((info, page, size, annots, links, images, marks, selected, dirty, label)) = data {
                    div { class: "prop-head", "{t(lang, \"props.document\")}" }
                    {prop(t(lang, "dialog.props.title_field"), if info.title.is_empty() { "—".into() } else { info.title.clone() })}
                    {prop(t(lang, "dialog.props.author"), if info.author.is_empty() { "—".into() } else { info.author.clone() })}
                    {prop(t(lang, "dialog.props.pages"), info.page_count.to_string())}
                    {prop(t(lang, "dialog.props.size"), human_size(info.file_size))}
                    {prop(t(lang, "dialog.props.version"), format!("PDF {}", info.version))}
                    {prop(t(lang, "dialog.props.form"), yes_no(info.has_form))}
                    {prop(t(lang, "props.unsaved"), yes_no(dirty))}
                    button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::DocProps), "{t(lang, \"cmd.docProps\")}" }
                    div { class: "prop-head", "{t(lang, \"props.page\")} {page + 1}" }
                    if !label.is_empty() && label != (page + 1).to_string() {
                        {prop(t(lang, "dialog.labels.title"), label.clone())}
                    }
                    if let Some(s) = size {
                        {prop(t(lang, "props.size"), format!("{:.0} × {:.0} pt", s.width, s.height))}
                        {prop(String::new(), format!("{:.0} × {:.0} mm", s.width / 72.0 * 25.4, s.height / 72.0 * 25.4))}
                        {prop(t(lang, "props.rotation"), format!("{}°", s.rotation))}
                    }
                    {prop(t(lang, "props.annotations"), annots.to_string())}
                    {prop(t(lang, "props.links"), links.to_string())}
                    {prop(t(lang, "props.images"), images.to_string())}
                    {prop(t(lang, "props.marks"), if marks.is_empty() { t(lang, "label.none") } else { marks.join(", ") })}
                    if let Some(a) = selected {
                        div { class: "prop-head", "{t(lang, \"props.selection\")}" }
                        {prop(t(lang, "props.type"), annot_name(lang, &a.subtype))}
                        {prop(t(lang, "dialog.note.author"), a.author.clone())}
                        {prop(t(lang, "dialog.props.modified"), a.modified.clone())}
                        if !a.contents.is_empty() {
                            div { class: "prop-text", "{a.contents}" }
                        }
                        div { class: "row",
                            button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::AnnotationText { page: a.page, index: a.index, contents: a.contents.clone() }), "{t(lang, \"label.edit\")}" }
                            button { class: "tbtn", onclick: move |_| dispatch(st, "comments.flattenSel"), "{t(lang, \"label.flatten\")}" }
                            button { class: "tbtn danger", onclick: move |_| dispatch(st, "comments.deleteSel"), "{t(lang, \"label.delete\")}" }
                        }
                    }
                } else {
                    div { class: "pane-empty", "{t(lang, \"toast.no_document\")}" }
                }
            }
        }
    }
}
