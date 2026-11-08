//! The Find Form Fields dialog: the places a flat form leaves to write in,
//! each with its kind, the words that will name it and its page, to choose;
//! the chosen ones become fields in one undo step.

use dioxus::prelude::*;
use folitecta_core::{FieldProposal, NewField};

use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};
use crate::ui::dialogs::Frame;

/// The proposals of the active document on its pages `all` or the current one.
async fn find(st: AppState, all: bool) -> Vec<FieldProposal> {
    let Some((copy, page, n)) = st.read_active(|t| (t.doc.duplicate(), t.page, t.doc.page_count())) else {
        return Vec::new();
    };
    let Ok(copy) = copy else { return Vec::new() };
    let range = if all { 0..n } else { page..(page + 1).min(n) };
    tokio::task::spawn_blocking(move || copy.find_form_fields(range))
        .await
        .unwrap_or_default()
}

/// Add the chosen proposals to the active document.
pub fn add(st: AppState, chosen: Vec<FieldProposal>) {
    st.set_dialog(Dialog::None);
    let lang = st.lang();
    match st.with_active(|t| t.doc.add_found_fields(&chosen)) {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(lang, "fields.find.added", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.core_error(&e),
        None => {}
    }
}

#[component]
pub fn FindFieldsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut all = use_signal(|| true);
    let mut chosen = use_signal(Vec::<bool>::new);
    let found = use_resource(move || async move {
        let list = find(st, all()).await;
        chosen.set(vec![true; list.len()]);
        list
    });
    let list = found.read().clone();
    let picked: Vec<FieldProposal> = list
        .iter()
        .flatten()
        .zip(chosen.read().iter())
        .filter(|(_, c)| **c)
        .map(|(p, _)| p.clone())
        .collect();
    let none_picked = picked.is_empty();
    rsx! {
        Frame { title: t(lang, "fields.find.title"), ok_label: t(lang, "fields.find.add"), wide: true,
            ok_disabled: none_picked,
            on_ok: move |_| add(st, picked.clone()),
            div { class: "field", label { "{t(lang, \"office.pages\")}" }
                select { class: "find-fields-pages", onchange: move |e| all.set(e.value() == "all"),
                    option { value: "all", selected: all(), "{t(lang, \"office.pages.all\")}" }
                    option { value: "current", selected: !all(), "{t(lang, \"office.pages.current\")}" }
                }
            }
            match list {
                None => rsx! { p { class: "hint", "{t(lang, \"fields.find.looking\")}" } },
                Some(list) if list.is_empty() => rsx! { p { class: "hint find-fields-none", "{t(lang, \"fields.find.none\")}" } },
                Some(list) => rsx! {
                    div { class: "list find-fields",
                        for (i, p) in list.iter().enumerate() {
                            label { key: "{i}", class: "check find-field", "data-page": "{p.page}",
                                input { r#type: "checkbox",
                                    checked: chosen.read().get(i).copied().unwrap_or(false),
                                    onchange: move |_| {
                                        let mut c = chosen.write();
                                        if let Some(v) = c.get_mut(i) { *v = !*v; }
                                    }
                                }
                                span {
                                    b { {match (p.kind, p.multiline) {
                                        _ if p.comb > 1 => tf(lang, "fields.find.comb", &[("count", &p.comb.to_string())]),
                                        (NewField::CheckBox, _) => t(lang, "fieldkind.check"),
                                        (_, true) => t(lang, "fields.find.text_lines"),
                                        _ => t(lang, "fieldkind.text"),
                                    }} }
                                    {format!(" {} · ", if p.label.is_empty() { t(lang, "fields.find.unnamed") } else { p.label.clone() })}
                                    {tf(lang, "fields.find.page", &[("page", &(p.page + 1).to_string())])}
                                }
                            }
                        }
                    }
                },
            }
            p { class: "hint", "{t(lang, \"fields.find.hint\")}" }
        }
    }
}
