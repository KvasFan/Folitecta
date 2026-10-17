//! Page dialogs: inserting, rotating, deleting, extracting, cropping, resizing, moving and splitting pages, new documents, labels, transitions, overlays and guides.

use super::*;

#[component]
pub(super) fn InsertBlank() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let count = use_signal(|| "1".to_string());
    let mut position = use_signal(|| "after".to_string());
    let mut size = use_signal(|| "a4".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.insert.title"),
            on_ok: move |_| {
                let n: usize = count().trim().parse().unwrap_or(1);
                let (w, h) = match size().as_str() { "letter" => (612.0, 792.0), "a3" => (842.0, 1191.0), "a5" => (420.0, 595.0), "current" => st.read_active(|t| { let s = t.page_sizes[t.page]; (s.width, s.height) }).unwrap_or((595.0, 842.0)), _ => (595.0, 842.0) };
                let at = match position().as_str() { "before" => st.current_page(), "end" => st.page_count(), _ => st.current_page() + 1 };
                apply(st, |tab| tab.doc.insert_blank_pages(at, n, w, h));
            },
            div { class: "field-grid",
                Num { label: t(lang, "dialog.insert.count"), value: count, min: "1", max: "500" }
                div { class: "field", label { "{t(lang, \"dialog.insert.position\")}" }
                    select { onchange: move |e| position.set(e.value()),
                        option { value: "before", "{t(lang, \"dialog.insert.before\")}" }
                        option { value: "after", selected: true, "{t(lang, \"dialog.insert.after\")}" }
                        option { value: "end", "{t(lang, \"dialog.insert.end\")}" }
                    }
                }
                div { class: "field", label { "{t(lang, \"dialog.insert.size\")}" }
                    select { onchange: move |e| size.set(e.value()),
                        option { value: "a4", selected: true, "A4 (210 × 297 mm)" }
                        option { value: "a3", "A3" }
                        option { value: "a5", "A5" }
                        option { value: "letter", "US Letter" }
                        option { value: "current", "{t(lang, \"dialog.pages.current\")}" }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn RotatePages() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    let mut angle = use_signal(|| "90".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.rotate.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => { let deg: i32 = angle().parse().unwrap_or(90); apply(st, |tab| tab.doc.rotate_pages(&pages, deg)); }
                Err(e) => st.error(e),
            },
            div { class: "field", label { "{t(lang, \"dialog.rotate.angle\")}" }
                div { class: "radio-row",
                    label { input { r#type: "radio", checked: angle() == "90", onchange: move |_| angle.set("90".into()) } "90° ↻" }
                    label { input { r#type: "radio", checked: angle() == "-90", onchange: move |_| angle.set("-90".into()) } "90° ↺" }
                    label { input { r#type: "radio", checked: angle() == "180", onchange: move |_| angle.set("180".into()) } "180°" }
                }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn DeletePages() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    rsx! {
        Frame { title: t(lang, "dialog.delete.title"), danger: true,
            on_ok: move |_| match scope_pages(st, mode(), &range()) { Ok(pages) => apply(st, |tab| tab.doc.delete_pages(&pages)), Err(e) => st.error(e) },
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn ExtractPages() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    rsx! {
        Frame { title: t(lang, "dialog.extract.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) { Ok(pages) => { st.set_dialog(Dialog::None); files::extract_pages(st, pages); } Err(e) => st.error(e) },
            div { class: "hint", "{t(lang, \"dialog.extract.hint\")}" }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn CropPages() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    let left = use_signal(|| "0".to_string());
    let top = use_signal(|| "0".to_string());
    let right = use_signal(|| "0".to_string());
    let bottom = use_signal(|| "0".to_string());
    let mut reset = use_signal(|| false);
    let f = |s: Signal<String>| s().trim().parse::<f32>().unwrap_or(0.0);
    rsx! {
        Frame { title: t(lang, "dialog.crop.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => {
                    if reset() { apply(st, |tab| tab.doc.reset_crop(&pages)); } else { let (l, tp, r, b) = (f(left), f(top), f(right), f(bottom)); apply(st, |tab| tab.doc.crop_pages_by_margins(&pages, l, tp, r, b)); }
                }
                Err(e) => st.error(e),
            },
            div { class: "hint", "{t(lang, \"dialog.crop.margins\")}" }
            div { class: "field-grid",
                Num { label: t(lang, "dialog.hf.left"), value: left, min: "0", step: "1" }
                Num { label: t(lang, "dialog.margin.top"), value: top, min: "0", step: "1" }
                Num { label: t(lang, "dialog.hf.right"), value: right, min: "0", step: "1" }
                Num { label: t(lang, "dialog.margin.bottom"), value: bottom, min: "0", step: "1" }
            }
            label { class: "check", input { r#type: "checkbox", checked: reset(), onchange: move |_| reset.set(!reset()) } span { "{t(lang, \"dialog.crop.reset\")}" } }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn ResizePages() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    let width = use_signal(|| "595".to_string());
    let height = use_signal(|| "842".to_string());
    let mut scale = use_signal(|| true);
    rsx! {
        Frame { title: t(lang, "dialog.resize.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => { let w: f32 = width().trim().parse().unwrap_or(595.0); let h: f32 = height().trim().parse().unwrap_or(842.0); let s = scale(); apply(st, |tab| tab.doc.resize_pages(&pages, w, h, s)); }
                Err(e) => st.error(e),
            },
            div { class: "field-grid",
                Num { label: t(lang, "dialog.size.width"), value: width, min: "10", step: "1" }
                Num { label: t(lang, "dialog.size.height"), value: height, min: "10", step: "1" }
            }
            div { class: "hint", "{t(lang, \"dialog.resize.hint\")}" }
            label { class: "check", input { r#type: "checkbox", checked: scale(), onchange: move |_| scale.set(!scale()) } span { "{t(lang, \"dialog.resize.scale\")}" } }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn AddMargins() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    let left = use_signal(|| "36".to_string());
    let top = use_signal(|| "36".to_string());
    let right = use_signal(|| "36".to_string());
    let bottom = use_signal(|| "36".to_string());
    let f = |s: Signal<String>| s().trim().parse::<f32>().unwrap_or(0.0);
    rsx! {
        Frame { title: t(lang, "dialog.margins.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => { let (l, tp, r, b) = (f(left), f(top), f(right), f(bottom)); apply(st, |tab| tab.doc.add_margins(&pages, l, tp, r, b)); }
                Err(e) => st.error(e),
            },
            div { class: "field-grid",
                Num { label: t(lang, "dialog.hf.left"), value: left, step: "1" }
                Num { label: t(lang, "dialog.margin.top"), value: top, step: "1" }
                Num { label: t(lang, "dialog.hf.right"), value: right, step: "1" }
                Num { label: t(lang, "dialog.margin.bottom"), value: bottom, step: "1" }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn MovePages() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    let to = use_signal(|| "1".to_string());
    let count = st.page_count();
    rsx! {
        Frame { title: cmd_label(lang, registry().get("document.movePages").unwrap()),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => { let target: usize = to().trim().parse::<usize>().unwrap_or(1).saturating_sub(1); apply(st, |tab| tab.doc.move_pages(&pages, target)); }
                Err(e) => st.error(e),
            },
            PageScope { mode, range }
            Num { label: tf(lang, "dialog.move.before", &[("count", &(count + 1).to_string())]), value: to, min: "1", max: (count + 1).to_string() }
        }
    }
}

#[component]
pub(super) fn SwapPages() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let a = use_signal(|| (st.current_page() + 1).to_string());
    let b = use_signal(|| "1".to_string());
    rsx! {
        Frame { title: cmd_label(lang, registry().get("document.swapPages").unwrap()),
            on_ok: move |_| { let pa: usize = a().trim().parse().unwrap_or(1); let pb: usize = b().trim().parse().unwrap_or(1); apply(st, |tab| tab.doc.swap_pages(pa.saturating_sub(1), pb.saturating_sub(1))); },
            div { class: "field-grid", Num { label: t(lang, "dialog.swap.a"), value: a, min: "1" } Num { label: t(lang, "dialog.swap.b"), value: b, min: "1" } }
        }
    }
}

#[component]
pub(super) fn ExportImages() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    let dpi = use_signal(|| "150".to_string());
    let mut format = use_signal(|| "png".to_string());
    let mut images_only = use_signal(|| false);
    rsx! {
        Frame { title: t(lang, "dialog.export.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => {
                    st.set_dialog(Dialog::None);
                    if images_only() { files::extract_images(st, pages); } else if format() == "txt" { files::export_text(st); } else {
                        let d: f32 = dpi().trim().parse().unwrap_or(150.0);
                        let fmt = if format() == "jpg" { ImageFormat::Jpeg { quality: 88 } } else { ImageFormat::Png };
                        files::export_images(st, pages, d, fmt);
                    }
                }
                Err(e) => st.error(e),
            },
            div { class: "field-grid",
                Num { label: t(lang, "dialog.export.dpi"), value: dpi, min: "36", max: "600" }
                div { class: "field", label { "{t(lang, \"dialog.export.format\")}" }
                    select { onchange: move |e| format.set(e.value()), option { value: "png", "PNG" } option { value: "jpg", "JPEG" } option { value: "txt", "{t(lang, \"dialog.export.text\")}" } }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: images_only(), onchange: move |_| images_only.set(!images_only()) } span { "{t(lang, \"dialog.export.embedded\")}" } }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn Split() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let every = use_signal(|| "1".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.split.title"),
            on_ok: move |_| { let n: usize = every().trim().parse().unwrap_or(1); st.set_dialog(Dialog::None); files::split_document(st, n.max(1)); },
            Num { label: t(lang, "dialog.split.every"), value: every, min: "1" }
        }
    }
}

#[component]
pub(super) fn MergeFile(path: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut position = use_signal(|| "after".to_string());
    let p = std::path::PathBuf::from(path.clone());
    rsx! {
        Frame { title: t(lang, "dialog.merge.title"),
            on_ok: move |_| {
                let at = match position().as_str() { "before" => st.current_page(), "end" => st.page_count(), "start" => 0, _ => st.current_page() + 1 };
                st.set_dialog(Dialog::None);
                files::insert_from_file(st, p.clone(), at);
            },
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.file\")}" } span { class: "v", "{path}" } }
            div { class: "field", label { "{t(lang, \"dialog.insert.position\")}" }
                select { onchange: move |e| position.set(e.value()),
                    option { value: "start", "{t(lang, \"dialog.insert.start\")}" }
                    option { value: "before", "{t(lang, \"dialog.insert.before\")}" }
                    option { value: "after", selected: true, "{t(lang, \"dialog.insert.after\")}" }
                    option { value: "end", "{t(lang, \"dialog.insert.end\")}" }
                }
            }
        }
    }
}

/// Standard paper sizes in points (portrait).
pub(super) const PAPER: &[(&str, &str, f32, f32)] = &[
    ("a4", "A4 (210 × 297 mm)", 595.28, 841.89),
    ("a3", "A3 (297 × 420 mm)", 841.89, 1190.55),
    ("a5", "A5 (148 × 210 mm)", 419.53, 595.28),
    ("b5", "B5 (176 × 250 mm)", 498.9, 708.66),
    ("letter", "Letter (8.5 × 11 in)", 612.0, 792.0),
    ("legal", "Legal (8.5 × 14 in)", 612.0, 1008.0),
    ("tabloid", "Tabloid (11 × 17 in)", 792.0, 1224.0),
];

pub(super) fn paper_size(id: &str) -> Option<(f32, f32)> {
    PAPER.iter().find(|p| p.0 == id).map(|p| (p.2, p.3))
}

pub(super) fn num_or(s: &Signal<String>, default: f32) -> f32 {
    s.read().trim().replace(',', ".").parse().unwrap_or(default)
}

#[component]
pub(super) fn NewBlank() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let count = use_signal(|| "1".to_string());
    let mut size = use_signal(|| "a4".to_string());
    let mut landscape = use_signal(|| false);
    let width = use_signal(|| "595".to_string());
    let height = use_signal(|| "842".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.newblank.title"),
            on_ok: move |_| {
                let n = num_or(&count, 1.0).clamp(1.0, 2000.0) as usize;
                let (mut w, mut h) = paper_size(&size()).unwrap_or((num_or(&width, 595.0).clamp(36.0, 14400.0), num_or(&height, 842.0).clamp(36.0, 14400.0)));
                if landscape() { std::mem::swap(&mut w, &mut h); }
                st.set_dialog(Dialog::None);
                files::new_blank(st, n, w, h);
            },
            div { class: "field-grid",
                Num { label: t(lang, "dialog.insert.count"), value: count, min: "1", max: "2000" }
                div { class: "field", label { "{t(lang, \"dialog.insert.size\")}" }
                    select { onchange: move |e| size.set(e.value()),
                        for (id, label, _, _) in PAPER.iter() { option { key: "{id}", value: "{id}", selected: *id == size(), "{label}" } }
                        option { value: "custom", selected: size() == "custom", "{t(lang, \"dialog.size.custom\")}" }
                    }
                }
            }
            if size() == "custom" {
                div { class: "field-grid",
                    Num { label: t(lang, "dialog.size.width"), value: width, min: "36" }
                    Num { label: t(lang, "dialog.size.height"), value: height, min: "36" }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: landscape(), onchange: move |_| landscape.set(!landscape()) } span { "{t(lang, \"dialog.size.landscape\")}" } }
        }
    }
}

#[component]
pub(super) fn PasteSpecial() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let has_clip = st.clip.read().is_some();
    let has_doc = st.has_doc();
    let go = move |id: &'static str| {
        move |_| {
            st.set_dialog(Dialog::None);
            crate::commands::dispatch(st, id);
        }
    };
    rsx! {
        Frame { title: t(lang, "dialog.paste.title"),
            div { class: "choice-list",
                button { class: "choice", disabled: !has_clip || !has_doc, onclick: go("pasteAnnots"),
                    Icon { name: "comment" }
                    div { b { "{t(lang, \"dialog.paste.comments\")}" } span { class: "hint", "{t(lang, \"dialog.paste.comments_hint\")}" } }
                }
                button { class: "choice", disabled: !has_doc, onclick: go("pasteImage"),
                    Icon { name: "image" }
                    div { b { "{t(lang, \"dialog.paste.image\")}" } span { class: "hint", "{t(lang, \"dialog.paste.image_hint\")}" } }
                }
                button { class: "choice", disabled: !has_doc, onclick: go("pastePlainText"),
                    Icon { name: "text" }
                    div { b { "{t(lang, \"dialog.paste.text\")}" } span { class: "hint", "{t(lang, \"dialog.paste.text_hint\")}" } }
                }
                button { class: "choice", disabled: !has_doc, onclick: go("document.insertPagesFromClipboard"),
                    Icon { name: "insert" }
                    div { b { "{t(lang, \"dialog.paste.page\")}" } span { class: "hint", "{t(lang, \"dialog.paste.page_hint\")}" } }
                }
                button { class: "choice", onclick: go("newDocClipboard"),
                    Icon { name: "page" }
                    div { b { "{t(lang, \"dialog.paste.newdoc\")}" } span { class: "hint", "{t(lang, \"dialog.paste.newdoc_hint\")}" } }
                }
            }
        }
    }
}

#[component]
pub(super) fn Rasterize() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    let dpi = use_signal(|| "150".to_string());
    let mut annots = use_signal(|| true);
    rsx! {
        Frame { title: cmd_label(lang, registry().get("document.rasterizePages").unwrap()),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => { let d = num_or(&dpi, 150.0).clamp(36.0, 600.0); let a = annots(); apply(st, |tab| tab.doc.rasterize_pages(&pages, d, a)); }
                Err(e) => st.error(e),
            },
            div { class: "callout warn", "{t(lang, \"dialog.rasterize.hint\")}" }
            Num { label: t(lang, "dialog.export.dpi"), value: dpi, min: "36", max: "600" }
            label { class: "check", input { r#type: "checkbox", checked: annots(), onchange: move |_| annots.set(!annots()) } span { "{t(lang, \"dialog.rasterize.annots\")}" } }
            PageScope { mode, range }
        }
    }
}

/// Parse a list of numbers separated by spaces or semicolons (decimal comma allowed).
pub(super) fn parse_numbers(s: &str) -> Result<Vec<f32>, String> {
    s.split(|c: char| c == ';' || c.is_whitespace())
        .filter(|x| !x.trim().is_empty())
        .map(|x| {
            x.trim()
                .replace(',', ".")
                .parse::<f32>()
                .map_err(|_| format!("\"{x}\""))
        })
        .collect()
}

#[component]
pub(super) fn AddGuides() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let units = st.ui.read().value("units").unwrap_or("mm").to_string();
    let (unit_pt, _, _, unit_name) = crate::ui::canvas::unit_info(&units);
    let horizontal = use_signal(String::new);
    let vertical = use_signal(String::new);
    let mut every_page = use_signal(|| true);
    rsx! {
        Frame { title: t(lang, "dialog.guides.title"),
            on_ok: move |_| {
                let (hs, vs) = match (parse_numbers(&horizontal()), parse_numbers(&vertical())) {
                    (Ok(h), Ok(v)) => (h, v),
                    (Err(e), _) | (_, Err(e)) => { st.error(tf(st.lang(), "dialog.guides.bad", &[("value", &e)])); return; }
                };
                let page = if every_page() { None } else { Some(st.current_page()) };
                st.with_active(|t| {
                    t.guides.extend(hs.iter().map(|y| Guide { page, horizontal: true, pos: y * unit_pt }));
                    t.guides.extend(vs.iter().map(|x| Guide { page, horizontal: false, pos: x * unit_pt }));
                });
                if !option_on(st, "view.showGuides") {
                    st.set_option("view.showGuides", true);
                }
                st.set_dialog(Dialog::None);
            },
            div { class: "hint", "{tf(lang, \"dialog.guides.hint\", &[(\"unit\", unit_name)])}" }
            Text { label: t(lang, "dialog.guides.horizontal"), value: horizontal, placeholder: "20; 50", autofocus: true }
            Text { label: t(lang, "dialog.guides.vertical"), value: vertical, placeholder: "15; 105" }
            label { class: "check", input { r#type: "checkbox", checked: every_page(), onchange: move |_| every_page.set(!every_page()) } span { "{t(lang, \"dialog.guides.every_page\")}" } }
        }
    }
}

#[component]
pub(super) fn SplitTiles() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    let cols = use_signal(|| "2".to_string());
    let rows = use_signal(|| "1".to_string());
    rsx! {
        Frame { title: cmd_label(lang, registry().get("document.splitPages").unwrap()),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => { let c = num_or(&cols, 2.0).clamp(1.0, 20.0) as usize; let r = num_or(&rows, 1.0).clamp(1.0, 20.0) as usize; apply(st, |tab| tab.doc.split_pages_grid(&pages, c, r)); }
                Err(e) => st.error(e),
            },
            div { class: "hint", "{t(lang, \"dialog.tiles.hint\")}" }
            div { class: "field-grid",
                Num { label: t(lang, "dialog.tiles.cols"), value: cols, min: "1", max: "20" }
                Num { label: t(lang, "dialog.tiles.rows"), value: rows, min: "1", max: "20" }
            }
            PageScope { mode, range }
        }
    }
}

/// One page-label range being edited: start page, style, prefix, first number.
pub(super) type LabelRow = (String, String, String, String);

pub(super) const LABEL_STYLES: &[(&str, &str)] = &[
    ("D", "1, 2, 3"),
    ("r", "i, ii, iii"),
    ("R", "I, II, III"),
    ("a", "a, b, c"),
    ("A", "A, B, C"),
    ("", "—"),
];

#[component]
pub(super) fn PageLabels() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let labels = st.read_active(|t| t.doc.page_labels()).unwrap_or_default();
    let count = labels.len();
    let mut ranges: Signal<Vec<LabelRow>> =
        use_signal(|| vec![("1".into(), "D".into(), String::new(), "1".into())]);
    let preview = labels.iter().take(14).cloned().collect::<Vec<_>>().join(", ");
    rsx! {
        Frame { title: t(lang, "dialog.labels.title"), wide: true,
            on_ok: move |_| {
                let mut parsed: Vec<(usize, String, String, i64)> = Vec::new();
                for (s, style, prefix, first) in ranges() {
                    let start = s.trim().parse::<usize>().unwrap_or(0);
                    if start == 0 || start > count {
                        st.error(tf(st.lang(), "dialog.labels.bad_start", &[("value", s.trim()), ("count", &count.to_string())]));
                        return;
                    }
                    parsed.push((start - 1, style, prefix, first.trim().parse().unwrap_or(1)));
                }
                let refs: Vec<(usize, &str, &str, i64)> = parsed.iter().map(|(a, b, c, d)| (*a, b.as_str(), c.as_str(), *d)).collect();
                apply(st, |tab| tab.doc.set_page_labels(&refs));
            },
            div { class: "hint", "{t(lang, \"dialog.labels.hint\")}" }
            div { class: "label-rows",
                div { class: "label-row head",
                    span { "{t(lang, \"dialog.labels.start\")}" } span { "{t(lang, \"dialog.labels.style\")}" } span { "{t(lang, \"dialog.labels.prefix\")}" } span { "{t(lang, \"dialog.labels.first\")}" } span {}
                }
                for (i, (s, style, prefix, first)) in ranges().into_iter().enumerate() {
                    div { class: "label-row", key: "{i}",
                        input { r#type: "number", min: "1", max: "{count}", value: "{s}", oninput: move |e| ranges.write()[i].0 = e.value() }
                        select { onchange: move |e| ranges.write()[i].1 = e.value(),
                            for (code, label) in LABEL_STYLES.iter() { option { key: "{code}", value: "{code}", selected: *code == style, "{label}" } }
                        }
                        input { value: "{prefix}", oninput: move |e| ranges.write()[i].2 = e.value() }
                        input { r#type: "number", min: "1", value: "{first}", oninput: move |e| ranges.write()[i].3 = e.value() }
                        button { class: "ibtn", title: "{t(lang, \"dialog.labels.remove\")}", disabled: ranges.read().len() < 2, onclick: move |_| { ranges.write().remove(i); }, "×" }
                    }
                }
            }
            div { class: "row", style: "gap:8px",
                button { class: "tbtn", onclick: move |_| { let next = (st.current_page() + 1).to_string(); ranges.write().push((next, "D".into(), String::new(), "1".into())); }, Icon { name: "insert" } "{t(lang, \"dialog.labels.add\")}" }
                button { class: "tbtn danger", onclick: move |_| apply(st, |tab| tab.doc.set_page_labels(&[])), "{t(lang, \"dialog.labels.clear\")}" }
            }
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.labels.current\")}" } span { class: "v", "{preview}" } }
        }
    }
}

#[component]
pub(super) fn Transitions() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let current = st.read_active(|t| t.doc.page_transition(t.page)).flatten();
    let mut style = use_signal(|| {
        current
            .as_ref()
            .map(|c| c.0.clone())
            .unwrap_or_else(|| "Dissolve".to_string())
    });
    let duration = use_signal(|| {
        current
            .as_ref()
            .map(|c| c.1.to_string())
            .unwrap_or_else(|| "1".to_string())
    });
    let advance = use_signal(|| "0".to_string());
    let name = |s: &str| {
        if s == "R" {
            "Replace".to_string()
        } else {
            s.to_string()
        }
    };
    rsx! {
        Frame { title: t(lang, "dialog.transitions.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => {
                    let s = style();
                    let chosen = if s == "none" { None } else { Some(s) };
                    let d = num_or(&duration, 1.0).clamp(0.0, 60.0);
                    let a = num_or(&advance, 0.0);
                    let auto = (a > 0.0).then_some(a);
                    apply(st, |tab| tab.doc.set_page_transition(&pages, chosen.as_deref(), d, auto));
                }
                Err(e) => st.error(e),
            },
            div { class: "field-grid three",
                div { class: "field", label { "{t(lang, \"dialog.transitions.style\")}" }
                    select { onchange: move |e| style.set(e.value()),
                        option { value: "none", selected: style() == "none", "{t(lang, \"label.none\")}" }
                        for s in TRANSITIONS.iter() { option { key: "{s}", value: "{s}", selected: *s == style(), "{name(s)}" } }
                    }
                }
                Num { label: t(lang, "dialog.transitions.duration"), value: duration, min: "0", max: "60", step: "0.5" }
                Num { label: t(lang, "dialog.transitions.advance"), value: advance, min: "0", max: "3600", step: "1" }
            }
            div { class: "hint", "{t(lang, \"dialog.transitions.hint\")}" }
            PageScope { mode, range }
        }
    }
}

/// Open another PDF once per dialog instance.
pub(super) fn use_other_document(path: &str) -> Rc<Result<Document, String>> {
    let p = path.to_string();
    use_hook(move || Rc::new(Document::open(&p).map_err(|e| e.to_string())))
}

#[component]
pub(super) fn ReplacePages(path: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let other = use_other_document(&path);
    let src_count = other.as_ref().as_ref().map(|d| d.page_count()).unwrap_or(0);
    let count = st.page_count();
    let cur = (st.current_page() + 1).to_string();
    let first = use_signal(|| cur.clone());
    let last = use_signal(|| cur.clone());
    let src = use_signal(|| {
        if src_count > 0 {
            format!("1-{src_count}")
        } else {
            "1".to_string()
        }
    });
    let other_ok = other.clone();
    rsx! {
        Frame { title: cmd_label(lang, registry().get("document.replacePages").unwrap()),
            on_ok: move |_| {
                let Ok(doc) = other_ok.as_ref() else { return };
                let a = num_or(&first, 0.0) as usize;
                let b = num_or(&last, 0.0) as usize;
                if a < 1 || b < a || b > count {
                    st.error(tf(st.lang(), "dialog.replace.bad_range", &[("count", &count.to_string())]));
                    return;
                }
                match parse_page_range(&src(), src_count) {
                    Ok(src_pages) => apply(st, |tab| tab.doc.replace_pages(a - 1, b - a + 1, doc, &src_pages)),
                    Err(e) => st.error(e),
                }
            },
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.file\")}" } span { class: "v", "{path}" } }
            match other.as_ref() {
                Ok(_) => rsx! {
                    div { class: "field-grid",
                        Num { label: t(lang, "dialog.replace.first"), value: first, min: "1", max: count.to_string() }
                        Num { label: t(lang, "dialog.replace.last"), value: last, min: "1", max: count.to_string() }
                    }
                    Text { label: tf(lang, "dialog.replace.source", &[("count", &src_count.to_string())]), value: src }
                },
                Err(e) => rsx! { div { class: "callout warn", "{e}" } },
            }
        }
    }
}

#[component]
pub(super) fn OverlayFile(path: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let other = use_other_document(&path);
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let mut behind = use_signal(|| false);
    let other_ok = other.clone();
    rsx! {
        Frame { title: cmd_label(lang, registry().get("document.overlayPages").unwrap()),
            on_ok: move |_| {
                let Ok(doc) = other_ok.as_ref() else { return };
                match scope_pages(st, mode(), &range()) {
                    Ok(pages) => { let b = behind(); apply(st, |tab| tab.doc.overlay_document(doc, &pages, b)); }
                    Err(e) => st.error(e),
                }
            },
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.file\")}" } span { class: "v", "{path}" } }
            match other.as_ref() {
                Ok(d) => rsx! {
                    div { class: "hint", "{tf(lang, \"dialog.overlay.hint\", &[(\"count\", &d.page_count().to_string())])}" }
                    div { class: "field",
                        div { class: "radio-row",
                            label { input { r#type: "radio", checked: !behind(), onchange: move |_| behind.set(false) } "{t(lang, \"dialog.overlay.front\")}" }
                            label { input { r#type: "radio", checked: behind(), onchange: move |_| behind.set(true) } "{t(lang, \"dialog.overlay.behind\")}" }
                        }
                    }
                    PageScope { mode, range }
                },
                Err(e) => rsx! { div { class: "callout warn", "{e}" } },
            }
        }
    }
}
