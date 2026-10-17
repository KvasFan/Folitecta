//! Comparison pane and the dialog choosing the documents to compare.

use dioxus::prelude::*;
use folitecta_core::compare::DiffKind;
use folitecta_core::registry;

use crate::commands::compare::{goto, run};
use crate::commands::dispatch;
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;

fn kind_label(lang: Lang, k: DiffKind) -> String {
    t(lang, &format!("compare.kind.{}", k.name()))
}

#[component]
pub fn ComparisonPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(c) = (st.comparison)() else {
        return rsx! {
            div { class: "pane-empty",
                "{t(lang, \"compare.none\")}"
                div { class: "row", style: "justify-content:center;margin-top:8px",
                    button { class: "tbtn", onclick: move |_| dispatch(st, "compareDocs"), Icon { name: "swap" } "{t(lang, \"compare.start\")}" }
                }
            }
        };
    };
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let n = c.result.differences.len();
    let counts = tf(
        lang,
        "compare.counts",
        &[
            ("changed", &c.result.count(DiffKind::Changed).to_string()),
            ("inserted", &c.result.count(DiffKind::Inserted).to_string()),
            ("deleted", &c.result.count(DiffKind::Deleted).to_string()),
        ],
    );
    let tools: [(&str, &str); 6] = [
        ("comparisonView.first", "first"),
        ("comparisonView.prev", "prev"),
        ("comparisonView.next", "next"),
        ("comparisonView.last", "last"),
        ("cmpDocs.show.summary", "export"),
        ("compareDocs", "swap"),
    ];
    let filters = [
        ("all", "cmpDocs.filter.all"),
        ("list", "cmpDocs.filter.list"),
        ("none", "cmpDocs.filter.none"),
    ];
    let pos = if n == 0 {
        String::new()
    } else {
        format!("{}/{}", c.current + 1, n)
    };
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon) in tools {
                button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: n == 0 && id != "compareDocs", onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
            span { class: "muted", style: "margin-left:auto;font-size:12px", "{pos}" }
        }
        div { class: "compare-head",
            div { class: "compare-docs",
                button { class: "linkish", title: "{label(\"cmpDocs.show.old\")}", onclick: move |_| dispatch(st, "cmpDocs.show.old"), span { class: "chip del", "{t(lang, \"compare.old\")}" } span { class: "ellipsis", "{c.old_name}" } }
                button { class: "linkish", title: "{label(\"cmpDocs.show.new\")}", onclick: move |_| dispatch(st, "cmpDocs.show.new"), span { class: "chip ins", "{t(lang, \"compare.new\")}" } span { class: "ellipsis", "{c.new_name}" } }
            }
            div { class: "hint", "{counts}" }
            div { class: "row", style: "gap:4px;flex-wrap:wrap",
                for (k, id) in filters {
                    button { class: if c.filter == k { "tbtn on" } else { "tbtn" }, key: "{k}", onclick: move |_| dispatch(st, id), "{label(id)}" }
                }
                button { class: if c.rows { "tbtn on" } else { "tbtn" }, onclick: move |_| dispatch(st, "cmpDocs.show.rowHighlights"), "{label(\"cmpDocs.show.rowHighlights\")}" }
            }
        }
        if n == 0 {
            div { class: "pane-empty", "{t(lang, \"toast.compare_same\")}" }
        }
        div { class: "search-results",
            for (i, d) in c.result.differences.iter().enumerate() {
                {
                    let class = if i == c.current { "list-item result selected" } else { "list-item result" };
                    let kind = kind_label(lang, d.kind);
                    let kclass = match d.kind { DiffKind::Inserted => "chip ins", DiffKind::Deleted => "chip del", DiffKind::Changed => "chip chg" };
                    let pages = format!("{} {} → {}", t(lang, "status.page"), d.old_page + 1, d.new_page + 1);
                    rsx! {
                        div { class: "{class}", key: "{i}", onclick: move |_| goto(st, i),
                            div { class: "title", span { class: "{kclass}", "{kind}" } span { class: "muted", "{pages}" } }
                            if !d.old_text.is_empty() { div { class: "body snippet cmp-old", "{d.old_text}" } }
                            if !d.new_text.is_empty() { div { class: "body snippet cmp-new", "{d.new_text}" } }
                        }
                    }
                }
            }
        }
    }
}

/// Choose the old and the new document.
#[component]
pub fn CompareDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let tabs: Vec<(u64, String)> = st
        .docs
        .read()
        .iter()
        .map(|t| (t.id, t.doc.display_name()))
        .collect();
    let active = st.read_active(|t| t.id).unwrap_or(0);
    let other = tabs
        .iter()
        .map(|(id, _)| *id)
        .find(|id| *id != active)
        .unwrap_or(active);
    let mut old = use_signal(|| active);
    let mut new = use_signal(|| other);
    let mut ignore_case = use_signal(|| false);
    let pick = move |for_old: bool| {
        spawn(async move {
            let Some(f) = rfd::AsyncFileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_file()
                .await
            else {
                return;
            };
            crate::files::open_path(st, f.path().to_path_buf(), None);
            if let Some(id) = st.read_active(|t| t.id) {
                if for_old { old.set(id) } else { new.set(id) }
            }
            st.set_dialog(Dialog::Compare);
        });
    };
    let same = old() == new();
    rsx! {
        Frame { title: t(lang, "compare.title"), ok_label: Some(t(lang, "compare.run")),
            on_ok: move |_| {
                if old() == new() {
                    st.toast(crate::state::ToastKind::Info, t(lang, "compare.same_doc"), "");
                    return;
                }
                st.set_dialog(Dialog::None);
                run(st, old(), new(), ignore_case());
            },
            div { class: "field",
                label { "{t(lang, \"compare.old_doc\")}" }
                div { class: "row", style: "gap:6px",
                    select { style: "flex:1;min-width:0", onchange: move |e| { if let Ok(v) = e.value().parse() { old.set(v) } },
                        for (id, name) in tabs.iter().cloned() { option { key: "{id}", value: "{id}", selected: old() == id, "{name}" } }
                    }
                    button { class: "tbtn", onclick: move |_| pick(true), Icon { name: "open" } "{t(lang, \"launch.browse\")}" }
                }
            }
            div { class: "field",
                label { "{t(lang, \"compare.new_doc\")}" }
                div { class: "row", style: "gap:6px",
                    select { style: "flex:1;min-width:0", onchange: move |e| { if let Ok(v) = e.value().parse() { new.set(v) } },
                        for (id, name) in tabs.iter().cloned() { option { key: "{id}", value: "{id}", selected: new() == id, "{name}" } }
                    }
                    button { class: "tbtn", onclick: move |_| pick(false), Icon { name: "open" } "{t(lang, \"launch.browse\")}" }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: ignore_case(), onchange: move |_| ignore_case.set(!ignore_case()) } span { "{t(lang, \"compare.ignore_case\")}" } }
            if same { div { class: "hint warn", "{t(lang, \"compare.same_doc\")}" } }
            div { class: "hint", "{t(lang, \"compare.hint\")}" }
        }
    }
}
