//! Page mark dialogs: watermarks, headers and footers, page and Bates numbers, backgrounds and removing marks.

use super::*;

/// Localized name of a mark position.
pub(super) fn pos_label(lang: Lang, p: MarkPosition) -> String {
    t(
        lang,
        &format!("pos.{}", p.label().to_lowercase().replace(' ', "_")),
    )
}

#[component]
pub(super) fn Watermark() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let text = use_signal(|| "DRAFT".to_string());
    let size = use_signal(|| "72".to_string());
    let opacity = use_signal(|| "40".to_string());
    let rotation = use_signal(|| "45".to_string());
    let mut position = use_signal(|| "Center".to_string());
    let mut color = use_signal(|| "#9ca3af".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.watermark.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => {
                    let opts = TextMarkOptions {
                        text: text(),
                        font_size: size().trim().parse().unwrap_or(72.0),
                        color: parse_color(&color()),
                        opacity: opacity().trim().parse::<f32>().unwrap_or(40.0) / 100.0,
                        rotation: rotation().trim().parse().unwrap_or(45.0),
                        position: MarkPosition::from_label(&position()).unwrap_or_default(),
                        margin: 24.0,
                    };
                    apply(st, |tab| tab.doc.add_text_watermark(&pages, &opts));
                }
                Err(e) => st.error(e),
            },
            Text { label: t(lang, "dialog.watermark.text"), value: text, autofocus: true }
            crate::ui::macros::MacroPicker {}
            div { class: "field-grid three",
                Num { label: t(lang, "dialog.watermark.size"), value: size, min: "6", max: "400" }
                Num { label: t(lang, "dialog.watermark.opacity"), value: opacity, min: "5", max: "100" }
                Num { label: t(lang, "dialog.watermark.rotation"), value: rotation, min: "-180", max: "180" }
            }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.watermark.position\")}" }
                    select { onchange: move |e| position.set(e.value()),
                        for p in MarkPosition::ALL { option { value: "{p.label()}", selected: p == MarkPosition::Center, "{pos_label(lang, p)}" } }
                    }
                }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn HeaderFooter() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let hl = use_signal(|| "{title}".to_string());
    let hc = use_signal(String::new);
    let hr = use_signal(|| "{date}".to_string());
    let fl = use_signal(String::new);
    let fc = use_signal(|| "Page {page} of {pages}".to_string());
    let fr = use_signal(String::new);
    let size = use_signal(|| "10".to_string());
    let margin = use_signal(|| "28".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.hf.title"), wide: true,
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => {
                    let (a, b, c, d, e, f) = (hl(), hc(), hr(), fl(), fc(), fr());
                    let fs: f32 = size().trim().parse().unwrap_or(10.0);
                    let m: f32 = margin().trim().parse().unwrap_or(28.0);
                    apply(st, |tab| tab.doc.add_header_footer(&pages, [&a, &b, &c], [&d, &e, &f], fs, m, [0.0, 0.0, 0.0]));
                }
                Err(e) => st.error(e),
            },
            div { class: "hint", "{t(lang, \"dialog.hf.hint\")}" }
            b { "{t(lang, \"dialog.hf.header\")}" }
            div { class: "field-grid three", Text { label: t(lang, "dialog.hf.left"), value: hl } Text { label: t(lang, "dialog.hf.center"), value: hc } Text { label: t(lang, "dialog.hf.right"), value: hr } }
            b { "{t(lang, \"dialog.hf.footer\")}" }
            div { class: "field-grid three", Text { label: t(lang, "dialog.hf.left"), value: fl } Text { label: t(lang, "dialog.hf.center"), value: fc } Text { label: t(lang, "dialog.hf.right"), value: fr } }
            div { class: "field-grid", Num { label: t(lang, "dialog.watermark.size"), value: size, min: "5", max: "72" } Num { label: t(lang, "dialog.margin.pt"), value: margin, min: "0", max: "200" } }
            crate::ui::macros::MacroPicker {}
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn PageNumbers(bates: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let format = use_signal(|| {
        if bates {
            crate::commands::macros::bates_format(st)
        } else {
            "{page}".to_string()
        }
    });
    // Bates numbering continues from the previous run.
    let start = use_signal(|| {
        if bates {
            crate::commands::macros::bates_next(st).to_string()
        } else {
            "1".to_string()
        }
    });
    let size = use_signal(|| "10".to_string());
    let mut position = use_signal(|| {
        if bates {
            "Bottom right".to_string()
        } else {
            "Bottom center".to_string()
        }
    });
    let title = if bates {
        t(lang, "dialog.bates.title")
    } else {
        t(lang, "dialog.numbers.title")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => {
                    let fmt = format();
                    let s: i64 = start().trim().parse().unwrap_or(1);
                    let fs: f32 = size().trim().parse().unwrap_or(10.0);
                    let pos = MarkPosition::from_label(&position()).unwrap_or(MarkPosition::BottomCenter);
                    let ok = st.with_active(|tab| tab.doc.add_page_numbers(&pages, &fmt, s, pos, fs, 28.0, [0.0, 0.0, 0.0]));
                    if bates && matches!(ok, Some(Ok(()))) {
                        st.set_value("bates.format", fmt.clone());
                        st.set_value("bates.next", (s + pages.len() as i64).to_string());
                    }
                    apply(st, |_| ok.unwrap_or(Ok(())));
                }
                Err(e) => st.error(e),
            },
            Text { label: t(lang, "dialog.numbers.format"), value: format, autofocus: true }
            div { class: "hint", "{t(lang, \"dialog.hf.hint\")}" }
            crate::ui::macros::MacroPicker {}
            div { class: "field-grid three",
                Num { label: t(lang, "dialog.numbers.start"), value: start, min: "0" }
                Num { label: t(lang, "dialog.watermark.size"), value: size, min: "5", max: "72" }
                div { class: "field", label { "{t(lang, \"dialog.watermark.position\")}" }
                    select { onchange: move |e| position.set(e.value()),
                        for p in MarkPosition::ALL.iter().filter(|p| **p != MarkPosition::Center) { option { value: "{p.label()}", selected: p.label() == position(), "{pos_label(lang, *p)}" } }
                    }
                }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn RemoveMarks() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let mut kind = use_signal(|| "all".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.removemarks.title"), danger: true,
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => {
                    let k = kind();
                    apply_count(st, "toast.marks_removed", |tab| match k.as_str() {
                        "all" => tab.doc.remove_marks(&pages, None),
                        "Overlay" => tab.doc.remove_marks_of(&pages, &["Overlay", "Underlay"]),
                        other => tab.doc.remove_marks(&pages, Some(other)),
                    });
                }
                Err(e) => st.error(e),
            },
            div { class: "field", label { "{t(lang, \"dialog.removemarks.kind\")}" }
                select { onchange: move |e| kind.set(e.value()),
                    option { value: "all", "{t(lang, \"dialog.removemarks.all\")}" } option { value: "Watermark", "{t(lang, \"dialog.removemarks.watermarks\")}" } option { value: "HeaderFooter", "{t(lang, \"dialog.removemarks.hf\")}" } option { value: "PageNumber", "{t(lang, \"dialog.removemarks.numbers\")}" } option { value: "Background", "{t(lang, \"dialog.removemarks.backgrounds\")}" } option { value: "Image", "{t(lang, \"dialog.removemarks.images\")}" } option { value: "Overlay", "{t(lang, \"dialog.removemarks.overlays\")}" }
                }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn Background() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let mut color = use_signal(|| "#fff8dc".to_string());
    let opacity = use_signal(|| "100".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.background.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => { let c = parse_color(&color()); let o = num_or(&opacity, 100.0).clamp(1.0, 100.0) / 100.0; apply(st, |tab| tab.doc.add_background_color(&pages, c, o)); }
                Err(e) => st.error(e),
            },
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
                Num { label: t(lang, "dialog.watermark.opacity"), value: opacity, min: "1", max: "100" }
            }
            div { class: "hint", "{t(lang, \"dialog.background.hint\")}" }
            PageScope { mode, range }
        }
    }
}
