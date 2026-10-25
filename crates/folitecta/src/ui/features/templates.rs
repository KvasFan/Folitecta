//! The gallery of templates (File › From Template…): papers, planners and
//! office forms, each with the picture of its first page on the paper
//! chosen; the paper, its orientation, how many pages (or months), the
//! first month and the first day of the week.

use dioxus::prelude::*;
use folitecta_core::templates::this_month;
use folitecta_core::{Template, TemplateGroup, TemplateOptions, registry};

use crate::commands::templates::{self, PICTURES};
use crate::i18n::{cmd_label, t};
use crate::state::AppState;
use crate::ui::dialogs::{Frame, PAPER, paper_size};

fn number(text: &str, default: i64, min: i64, max: i64) -> i64 {
    text.trim().parse::<i64>().unwrap_or(default).clamp(min, max)
}

#[component]
pub fn TemplateGallery() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let title = registry()
        .get("newDocTemplate")
        .map(|c| cmd_label(lang, c).trim_end_matches('…').to_string())
        .unwrap_or_default();
    let mut chosen = use_signal(|| Template::from_id(&st.value("templates.last")).unwrap_or(Template::Ruled));
    let mut paper = use_signal(|| {
        let v = st.value("templates.paper");
        if paper_size(&v).is_some() {
            v
        } else {
            "a4".to_string()
        }
    });
    let mut landscape = use_signal(|| false);
    let mut pages = use_signal(|| "1".to_string());
    let (this_year, this) = this_month();
    let mut month = use_signal(move || this);
    let mut year = use_signal(move || this_year.to_string());
    let mut monday = use_signal(|| st.value("templates.sunday_first") != "1");

    let (mut w, mut h) = paper_size(&paper()).unwrap_or((595.28, 841.89));
    if landscape() {
        std::mem::swap(&mut w, &mut h);
    }
    let tpl = chosen();
    let opts = TemplateOptions {
        width: w,
        height: h,
        pages: number(&pages(), 1, 1, 366) as usize,
        month: month(),
        year: number(&year(), this_year as i64, 1600, 9999) as i32,
        monday_first: monday(),
        labels: templates::labels(lang),
    };
    let pictures = PICTURES.read();
    let groups = [
        (TemplateGroup::Paper, "templates.group.paper"),
        (TemplateGroup::Planner, "templates.group.planner"),
        (TemplateGroup::Office, "templates.group.office"),
    ];
    let pages_label = if tpl == Template::Month {
        t(lang, "templates.months")
    } else {
        t(lang, "templates.pages")
    };
    let one_page = matches!(tpl, Template::Letter | Template::Meeting | Template::Invoice);
    let weeks = matches!(tpl, Template::Week | Template::Month);
    let hint = t(lang, &format!("templates.hint.{}", tpl.id()));
    let create_opts = opts.clone();
    let months: Vec<(u8, String)> = (1..=12u8)
        .map(|m| {
            (
                m,
                folitecta_core::templates::word(&opts.labels, &format!("month_{m}")),
            )
        })
        .collect();
    rsx! {
        Frame { title, wide: true, ok_label: t(lang, "templates.create"),
            on_ok: move |_| {
                st.set_value("templates.paper", paper());
                st.set_value("templates.sunday_first", if monday() { "0" } else { "1" });
                templates::create(st, chosen(), create_opts.clone());
            },
            div { class: "tpl-gallery",
                for (group, label) in groups {
                    div { key: "{label}", class: "tpl-group",
                        h4 { class: "tpl-group-title", "{t(lang, label)}" }
                        div { class: "tpl-cards",
                            for item in Template::ALL.into_iter().filter(|x| x.group() == group) {
                                {
                                    templates::want_picture(item, &opts, lang);
                                    let key = templates::picture_key(item, &opts, lang);
                                    let picture = pictures.get(&key).cloned();
                                    let name = t(lang, &format!("templates.name.{}", item.id()));
                                    let class = if item == tpl { "tpl-card on" } else { "tpl-card" };
                                    let ratio = format!("aspect-ratio: {} / {};", w.round(), h.round());
                                    let double_opts = opts.clone();
                                    rsx! {
                                        button { key: "{item.id()}", class, title: "{name}",
                                            "data-template": item.id(),
                                            onclick: move |_| chosen.set(item),
                                            ondoubleclick: move |_| {
                                                st.set_value("templates.paper", paper());
                                                templates::create(st, item, double_opts.clone());
                                            },
                                            if let Some(src) = picture {
                                                img { class: "tpl-picture", style: "{ratio}", src: "{src}", alt: "" }
                                            } else {
                                                div { class: "tpl-picture empty", style: "{ratio}" }
                                            }
                                            span { class: "tpl-name", "{name}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p { class: "hint tpl-hint", "{hint}" }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.insert.size\")}" }
                    select { class: "tpl-paper", onchange: move |e| paper.set(e.value()),
                        for (id, label, _, _) in PAPER.iter() { option { key: "{id}", value: "{id}", selected: *id == paper(), "{label}" } }
                    }
                }
                if !one_page {
                    div { class: "field", label { "{pages_label}" }
                        input { class: "tpl-pages", r#type: "number", min: "1", max: "366", value: "{pages}",
                            oninput: move |e| pages.set(e.value()) }
                    }
                }
                if tpl == Template::Month {
                    div { class: "field", label { "{t(lang, \"templates.first_month\")}" }
                        select { class: "tpl-month", onchange: move |e| month.set(e.value().parse().unwrap_or(1)),
                            for (m, name) in months.iter() { option { key: "{m}", value: "{m}", selected: *m == month(), "{name}" } }
                        }
                    }
                    div { class: "field", label { "{t(lang, \"templates.year\")}" }
                        input { class: "tpl-year", r#type: "number", min: "1600", max: "9999", value: "{year}",
                            oninput: move |e| year.set(e.value()) }
                    }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: landscape(), onchange: move |_| landscape.set(!landscape()) } span { "{t(lang, \"dialog.size.landscape\")}" } }
            if weeks {
                label { class: "check", input { class: "tpl-monday", r#type: "checkbox", checked: monday(), onchange: move |_| monday.set(!monday()) } span { "{t(lang, \"templates.monday_first\")}" } }
            }
        }
    }
}
