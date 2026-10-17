//! Stamps pane, stamp dialogs and the stamp designer.

use base64::Engine;
use dioxus::prelude::*;
use folitecta_core::registry;
use folitecta_core::stamp_design::{DATE_FORMATS, FIELD_KINDS, StampDesign, StampElement, TEMPLATES};

use crate::commands::stamps::{
    DESIGNER, LIBRARY_REV, SELECTED, SELECTED_COLLECTION, StampCollection, StampItem, add_stamp, find,
    library, preview_values, remove_selected, save_library, standard, update_stamp, use_stamp,
};
use crate::commands::{dispatch, option_on};
use crate::i18n::{Lang, cmd_label, t, tf, ui_label};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;
use crate::util::{color_hex, parse_color};

/// Picture of a stamp for `img` elements.
fn preview_src(item: &StampItem) -> Option<String> {
    if item.kind == "image" {
        let mime = if item.image.starts_with("/9j/") {
            "image/jpeg"
        } else {
            "image/png"
        };
        return Some(format!("data:{mime};base64,{}", item.image));
    }
    item.preview_svg().map(|svg| {
        format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(svg)
        )
    })
}

fn design_src(d: &StampDesign) -> String {
    format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(d.svg(&preview_values()))
    )
}

fn kind_name(lang: Lang, kind: &str) -> String {
    ui_label(lang, "stampel", kind, kind)
}

fn field_name(lang: Lang, field: &str) -> String {
    ui_label(lang, "stampfield", field, field)
}

fn align_name(lang: Lang, align: &str) -> String {
    t(lang, &format!("stamps.align.{align}"))
}

/// A group of stamps in the pane.
struct Group {
    key: String,
    title: String,
    collection: Option<usize>,
    stamps: Vec<StampItem>,
}

#[component]
pub fn StampsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let _ = LIBRARY_REV();
    let lib = library();
    let selected = SELECTED();
    let sel_collection = SELECTED_COLLECTION();
    let mut collapsed: Signal<std::collections::HashSet<String>> = use_signal(Default::default);
    let size: f32 = st
        .ui
        .read()
        .value("stamps.size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(120.0);
    let mut groups: Vec<Group> = Vec::new();
    let resolve = |ids: &[u64]| ids.iter().filter_map(|id| find(&lib, *id)).collect::<Vec<_>>();
    if option_on(st, "stamps.favorites") && !lib.favorites.is_empty() {
        groups.push(Group {
            key: "fav".into(),
            title: t(lang, "stamps.favorites"),
            collection: None,
            stamps: resolve(&lib.favorites),
        });
    }
    if option_on(st, "stamps.recents") && !lib.recents.is_empty() {
        groups.push(Group {
            key: "rec".into(),
            title: t(lang, "stamps.recents"),
            collection: None,
            stamps: resolve(&lib.recents),
        });
    }
    groups.push(Group {
        key: "std".into(),
        title: t(lang, "stamps.standard"),
        collection: None,
        stamps: standard(),
    });
    for (i, c) in lib.collections.iter().enumerate() {
        groups.push(Group {
            key: format!("c{i}"),
            title: c.name.clone(),
            collection: Some(i),
            stamps: c.stamps.clone(),
        });
    }
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let has_sel = selected.is_some();
    let tools: [(&str, &str, bool); 9] = [
        ("stampsView.addNew", "new", true),
        ("stampsView.addCustom", "text", true),
        ("stampsView.addNewFromCb", "clipboard", true),
        ("stampsView.addGroup", "folder", true),
        ("stampsView.selection.edit", "pencil", has_sel),
        ("stampsView.virtual.favorites.add", "star", has_sel),
        ("stampsView.sel.stamp.duplicate", "copy", has_sel),
        ("stampsView.zoomIn", "zoom-in", true),
        ("stampsView.zoomOut", "zoom-out", true),
    ];
    let tile_style = format!(
        "grid-template-columns: repeat(auto-fill, minmax({}px, 1fr));",
        size.round()
    );
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon, enabled) in tools {
                button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
            button { class: "ibtn danger", title: "{t(lang, \"label.delete\")}", disabled: !has_sel, onclick: move |_| remove_selected(st), Icon { name: "delete" } }
        }
        div { class: "history-list",
            for g in groups {
                {
                    let open = !collapsed().contains(&g.key);
                    let k = g.key.clone();
                    let coll = g.collection;
                    let header_class = if coll.is_some() && coll == sel_collection { "result-group selected" } else { "result-group" };
                    let n = g.stamps.len();
                    rsx! {
                        div { key: "{g.key}",
                            div { class: "{header_class}",
                                onclick: move |_| {
                                    if coll.is_some() { *SELECTED_COLLECTION.write() = coll; }
                                    collapsed.with_mut(|c| { if !c.remove(&k) { c.insert(k.clone()); } });
                                },
                                span { class: "twisty", if open { "▾" } else { "▸" } }
                                span { class: "ellipsis", "{g.title}" }
                                span { class: "chip", "{n}" }
                            }
                            if open {
                                if g.stamps.is_empty() {
                                    div { class: "hint", style: "padding: 4px 10px", "{t(lang, \"stamps.empty_group\")}" }
                                }
                                div { class: "stamp-tiles", style: "{tile_style}",
                                    for s in g.stamps.into_iter() {
                                        {
                                            let id = s.id;
                                            let src = preview_src(&s);
                                            let is_sel = selected == Some(id);
                                            let item = s.clone();
                                            rsx! {
                                                div { class: if is_sel { "stamp-tile selected" } else { "stamp-tile" }, key: "{g.key}-{id}", title: "{s.name}",
                                                    onclick: move |_| { *SELECTED.write() = Some(id); if coll.is_some() { *SELECTED_COLLECTION.write() = coll; } },
                                                    ondoubleclick: move |_| use_stamp(st, item.clone()),
                                                    if let Some(src) = src { img { src: "{src}", alt: "" } }
                                                    span { class: "ellipsis", "{s.name}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        div { class: "hint", style: "padding: 6px 10px", "{t(lang, \"stamps.hint\")}" }
    }
}

/// Values of the custom fields of a stamp, asked before placing it.
#[component]
pub fn StampValuesDialog(id: u64) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(item) = find(&library(), id) else {
        return rsx! {};
    };
    let fields = item
        .design
        .as_ref()
        .map(|d| d.custom_fields())
        .unwrap_or_default();
    let mut values: Signal<Vec<String>> = use_signal(|| vec![String::new(); fields.len()]);
    let names = fields.clone();
    rsx! {
        Frame { title: tf(lang, "stamps.values_title", &[("name", &item.name)]),
            on_ok: move |_| {
                let custom = names.iter().cloned().zip(values()).collect();
                st.set_dialog(Dialog::None);
                crate::commands::stamps::arm(st, crate::commands::stamps::PendingStamp { item: item.clone(), custom });
            },
            for (i, f) in fields.into_iter().enumerate() {
                div { class: "field", key: "{f}",
                    label { "{f}" }
                    input { autofocus: i == 0, value: "{values().get(i).cloned().unwrap_or_default()}", oninput: move |e| values.with_mut(|v| v[i] = e.value()) }
                }
            }
        }
    }
}

/// A new text stamp, or a stamp's name, text and colour.
#[component]
pub fn StampTextDialog(id: Option<u64>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let existing = id.and_then(|i| find(&library(), i));
    let mut name = use_signal(|| existing.as_ref().map(|s| s.name.clone()).unwrap_or_default());
    let mut text = use_signal(|| existing.as_ref().map(|s| s.text.clone()).unwrap_or_default());
    let mut color =
        use_signal(|| color_hex(existing.as_ref().map(|s| s.color).unwrap_or([0.78, 0.12, 0.12])));
    let is_text = existing.as_ref().is_none_or(|s| s.kind == "text");
    let shown = text().trim().to_string();
    let preview = crate::commands::stamps::text_design(
        if shown.is_empty() { "TEXT" } else { &shown },
        parse_color(&color()),
    );
    let src = design_src(&preview);
    let title = if id.is_some() {
        t(lang, "stamps.edit")
    } else {
        t(lang, "stamps.new_text")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let txt = text().trim().to_string();
                if is_text && txt.is_empty() { return; }
                let nm = if name().trim().is_empty() { txt.clone() } else { name().trim().to_string() };
                match existing.clone() {
                    Some(mut s) => {
                        s.name = nm;
                        if s.kind == "text" { s.text = txt; s.color = parse_color(&color()); }
                        update_stamp(s);
                    }
                    None => {
                        add_stamp(st, StampItem { name: nm, kind: "text".into(), text: txt, color: parse_color(&color()), ..Default::default() });
                    }
                }
                st.set_dialog(Dialog::None);
            },
            div { class: "field",
                label { "{t(lang, \"layers.name\")}" }
                input { value: "{name}", oninput: move |e| name.set(e.value()) }
            }
            if is_text {
                div { class: "field-grid",
                    div { class: "field",
                        label { "{t(lang, \"stamps.text\")}" }
                        input { value: "{text}", autofocus: true, placeholder: "PAID", oninput: move |e| text.set(e.value()) }
                    }
                    div { class: "field",
                        label { "{t(lang, \"dialog.watermark.color\")}" }
                        input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) }
                    }
                }
                div { class: "stamp-big-preview", img { src: "{src}", alt: "" } }
            }
        }
    }
}

/// A new collection, or a collection's name.
#[component]
pub fn StampGroupDialog(index: Option<usize>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let initial = index
        .and_then(|i| library().collections.get(i).map(|c| c.name.clone()))
        .unwrap_or_default();
    let mut name = use_signal(|| initial);
    rsx! {
        Frame { title: t(lang, "stamps.collection"),
            on_ok: move |_| {
                let n = name().trim().to_string();
                if n.is_empty() { return; }
                let mut lib = library();
                match index {
                    Some(i) => { if let Some(c) = lib.collections.get_mut(i) { c.name = n; } }
                    None => {
                        lib.collections.push(StampCollection { name: n, stamps: Vec::new() });
                        *SELECTED_COLLECTION.write() = Some(lib.collections.len() - 1);
                    }
                }
                save_library(&lib);
                *LIBRARY_REV.write() += 1;
                st.set_dialog(Dialog::None);
            },
            div { class: "field",
                label { "{t(lang, \"layers.name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) }
            }
        }
    }
}

/// Options of the stamps pane and its collections.
#[component]
pub fn StampOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let _ = LIBRARY_REV();
    let lib = library();
    let fav = option_on(st, "stamps.favorites");
    let rec = option_on(st, "stamps.recents");
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let edit = move |op: &'static str, i: usize| {
        let mut lib = library();
        match op {
            "up" if i > 0 => lib.collections.swap(i, i - 1),
            "down" if i + 1 < lib.collections.len() => lib.collections.swap(i, i + 1),
            "delete" => {
                lib.collections.remove(i);
                *SELECTED_COLLECTION.write() = None;
            }
            _ => {}
        }
        save_library(&lib);
        *LIBRARY_REV.write() += 1;
    };
    rsx! {
        Frame { title: t(lang, "stamps.options"),
            label { class: "check", input { r#type: "checkbox", checked: fav, onchange: move |_| st.set_option("stamps.favorites", !fav) } span { "{t(lang, \"stamps.favorites\")}" } }
            label { class: "check", input { r#type: "checkbox", checked: rec, onchange: move |_| st.set_option("stamps.recents", !rec) } span { "{t(lang, \"stamps.recents\")}" } }
            div { class: "row", style: "gap:6px;flex-wrap:wrap",
                button { class: "tbtn", onclick: move |_| dispatch(st, "stampsView.virtual.favorites.clear"), "{label(\"stampsView.virtual.favorites.clear\")}" }
                button { class: "tbtn", onclick: move |_| dispatch(st, "stampsView.virtual.recents.clear"), "{label(\"stampsView.virtual.recents.clear\")}" }
            }
            div { class: "field",
                label { "{t(lang, \"stamps.collections\")}" }
                if lib.collections.is_empty() { div { class: "hint", "{t(lang, \"stamps.no_collections\")}" } }
                for (i, c) in lib.collections.iter().enumerate() {
                    div { class: "pf-col", key: "{i}",
                        span { class: "ellipsis", "{c.name} ({c.stamps.len()})" }
                        div { class: "row", style: "gap:2px",
                            button { class: "ibtn", title: "{t(lang, \"stamps.rename\")}", onclick: move |_| st.set_dialog(Dialog::StampGroup(Some(i))), Icon { name: "pencil" } }
                            button { class: "ibtn", disabled: i == 0, onclick: move |_| edit("up", i), Icon { name: "up" } }
                            button { class: "ibtn", onclick: move |_| edit("down", i), Icon { name: "down" } }
                            button { class: "ibtn danger", onclick: move |_| edit("delete", i), Icon { name: "delete" } }
                        }
                    }
                }
            }
        }
    }
}

/// New colour of a stamp (texts and lines of a design).
#[component]
pub fn StampRecolorDialog(id: u64) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(item) = find(&library(), id) else {
        return rsx! {};
    };
    let mut color = use_signal(|| color_hex(item.color));
    let is_image = item.kind == "image";
    rsx! {
        Frame { title: t(lang, "stamps.recolor"),
            on_ok: move |_| {
                let c = parse_color(&color());
                let mut s = item.clone();
                s.color = c;
                if let Some(d) = s.design.as_mut() {
                    for e in &mut d.elements { e.color = c; }
                }
                if s.is_standard() {
                    s.name = tf(lang, "stamps.copy_of", &[("name", &s.name)]);
                    add_stamp(st, s);
                } else {
                    update_stamp(s);
                }
                st.set_dialog(Dialog::None);
            },
            div { class: "field",
                label { "{t(lang, \"dialog.watermark.color\")}" }
                input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) }
            }
            if is_image { div { class: "hint", "{t(lang, \"stamps.recolor_image\")}" } }
        }
    }
}

fn num(v: &str, d: f32) -> f32 {
    v.trim().replace(',', ".").parse().unwrap_or(d)
}

fn points_text(p: &[[f32; 2]]) -> String {
    p.iter()
        .map(|q| format!("{},{}", q[0], q[1]))
        .collect::<Vec<_>>()
        .join("; ")
}

fn parse_points(s: &str) -> Vec<[f32; 2]> {
    s.split(';')
        .filter_map(|pair| {
            let (a, b) = pair.split_once(',')?;
            Some([a.trim().parse().ok()?, b.trim().parse().ok()?])
        })
        .collect()
}

/// Date formats offered: built-in ones and the user's.
pub fn date_formats(st: AppState) -> Vec<String> {
    let mut v: Vec<String> = DATE_FORMATS.iter().map(|s| s.to_string()).collect();
    if let Some(custom) = st.ui.peek().value("stamps.dateFormats") {
        v.extend(
            custom
                .split('|')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_string()),
        );
    }
    v
}

#[component]
pub fn StampDesignerDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let d = DESIGNER();
    let design = d.design.clone();
    let src = design_src(&design);
    let sel = d.selected.filter(|i| *i < design.elements.len());
    let el = sel.map(|i| design.elements[i].clone());
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let adds: [(&str, &str); 8] = [
        ("stampCustom.add.text", "text"),
        ("stampCustom.add.rect", "rect"),
        ("stampCustom.add.oval", "ellipse"),
        ("stampCustom.add.line", "line"),
        ("stampCustom.add.polyline", "arrow"),
        ("stampCustom.add.polygon", "pencil"),
        ("stampCustom.add.curve", "signature"),
        ("stampCustom.add.file", "image"),
    ];
    let fields: [(&str, &str); 8] = [
        ("date", "stampCustom.add.field.date"),
        ("name", "stampCustom.add.field.identity.name"),
        ("email", "stampCustom.add.field.identity.email"),
        ("corporation", "stampCustom.add.field.identity.corporation"),
        ("login", "stampCustom.add.field.identity.loginName"),
        ("counter", "stampCustom.add.field.counter"),
        ("custom", "stampCustom.add.field.custom"),
        ("macros", "stampCustom.add.field.macros"),
    ];
    let ops: [(&str, &str); 5] = [
        ("stampCustom.openFromTemplate", "grid"),
        ("stampCustom.cropWhiteMargins", "crop"),
        ("stampCustom.clear", "delete"),
        ("stampCustom.save", "save"),
        ("stampCustom.saveAs", "export"),
    ];
    let preview_w = if d.zoom > 0.0 {
        design.width * d.zoom
    } else {
        (design.width * 2.0).min(420.0)
    };
    let formats = date_formats(st);
    let bg = design.background.map(color_hex);
    let set = move |f: Box<dyn FnOnce(&mut StampElement)>| {
        if let Some(i) = DESIGNER.peek().selected {
            if let Some(e) = DESIGNER.write().design.elements.get_mut(i) {
                f(e);
            }
        }
    };
    rsx! {
        Frame { title: t(lang, "stamps.designer"), wide: true, ok_label: Some(label("stampCustom.addStamp")),
            on_ok: move |_| dispatch(st, "stampCustom.addStamp"),
            div { class: "stamp-designer",
                div { class: "row", style: "gap:4px;flex-wrap:wrap;align-items:center",
                    input { class: "stamp-name", value: "{d.name}", title: "{t(lang, \"layers.name\")}", oninput: move |e| DESIGNER.write().name = e.value() }
                    for (id, icon) in adds {
                        button { class: "ibtn", key: "{id}", title: "{label(id)}", onclick: move |_| dispatch(st, id), Icon { name: icon } }
                    }
                    select { title: "{label(\"stampCustom.add.field\")}",
                        onchange: move |e| { let v = e.value(); if let Some((_, id)) = fields.iter().find(|(k, _)| *k == v) { dispatch(st, id); } },
                        option { value: "", selected: true, "{label(\"stampCustom.add.field\")}…" }
                        for (k, _) in fields { option { key: "{k}", value: "{k}", "{field_name(lang, k)}" } }
                    }
                    for (id, icon) in ops {
                        button { class: "ibtn", key: "{id}", title: "{label(id)}", onclick: move |_| dispatch(st, id), Icon { name: icon } }
                    }
                }
                if d.templates {
                    div { class: "stamp-templates",
                        for name in TEMPLATES.iter() {
                            {
                                let tpl = StampDesign::template(name).unwrap_or_default();
                                let tsrc = design_src(&tpl);
                                rsx! {
                                    button { class: "stamp-template", key: "{name}", title: "{name}",
                                        onclick: move |_| { let mut w = DESIGNER.write(); w.design = tpl.clone(); w.selected = None; w.templates = false; },
                                        img { src: "{tsrc}", alt: "" }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "stamp-designer-body",
                    div { class: "stamp-canvas",
                        img { src: "{src}", alt: "", style: "width:{preview_w}px" }
                        div { class: "field-grid three",
                            div { class: "field", label { "{label(\"stampCustom.pageWidth\")}" }
                                input { id: "stamp-width", r#type: "number", min: "10", value: "{design.width}", oninput: move |e| { let v = num(&e.value(), 200.0).max(10.0); DESIGNER.write().design.width = v; } } }
                            div { class: "field", label { "{label(\"stampCustom.pageHeight\")}" }
                                input { id: "stamp-height", r#type: "number", min: "10", value: "{design.height}", oninput: move |e| { let v = num(&e.value(), 70.0).max(10.0); DESIGNER.write().design.height = v; } } }
                            div { class: "field", label { "{label(\"stampCustom.pageBgColor\")}" }
                                div { class: "row", style: "gap:4px",
                                    input { r#type: "checkbox", checked: bg.is_some(), onchange: move |_| dispatch(st, "stampCustom.pageBgColor") }
                                    if let Some(b) = bg.clone() {
                                        input { r#type: "color", value: "{b}", oninput: move |e| DESIGNER.write().design.background = Some(parse_color(&e.value())) }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "stamp-side",
                        div { class: "stamp-elements",
                            if design.elements.is_empty() { div { class: "hint", "{t(lang, \"stamps.designer_empty\")}" } }
                            for (i, e) in design.elements.iter().enumerate() {
                                {
                                    let desc = match e.kind.as_str() {
                                        "text" => e.text.clone(),
                                        "field" => field_name(lang, &e.field),
                                        _ => String::new(),
                                    };
                                    let n = design.elements.len();
                                    rsx! {
                                        div { class: if sel == Some(i) { "stamp-el selected" } else { "stamp-el" }, key: "{i}",
                                            onclick: move |_| DESIGNER.write().selected = Some(i),
                                            span { class: "ellipsis", "{kind_name(lang, &e.kind)} {desc}" }
                                            div { class: "row", style: "gap:2px",
                                                button { class: "ibtn", disabled: i == 0, onclick: move |ev| { ev.stop_propagation(); let mut w = DESIGNER.write(); w.design.elements.swap(i, i - 1); w.selected = Some(i - 1); }, Icon { name: "up" } }
                                                button { class: "ibtn", disabled: i + 1 >= n, onclick: move |ev| { ev.stop_propagation(); let mut w = DESIGNER.write(); w.design.elements.swap(i, i + 1); w.selected = Some(i + 1); }, Icon { name: "down" } }
                                                button { class: "ibtn danger", onclick: move |ev| { ev.stop_propagation(); let mut w = DESIGNER.write(); w.design.elements.remove(i); w.selected = None; }, Icon { name: "delete" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(e) = el {
                            div { class: "stamp-props",
                                if e.kind == "text" {
                                    div { class: "field", label { "{t(lang, \"stamps.text\")}" }
                                        textarea { value: "{e.text}", oninput: move |ev| set(Box::new(move |x| x.text = ev.value())) } }
                                }
                                if e.kind == "field" {
                                    div { class: "field-grid",
                                        div { class: "field", label { "{t(lang, \"stamps.field\")}" }
                                            select { onchange: move |ev| set(Box::new(move |x| x.field = ev.value())),
                                                for k in FIELD_KINDS.iter() { option { key: "{k}", value: "{k}", selected: e.field == *k, "{field_name(lang, k)}" } }
                                            }
                                        }
                                        div { class: "field", label { "{t(lang, \"stamps.label\")}" }
                                            input { value: "{e.text}", oninput: move |ev| set(Box::new(move |x| x.text = ev.value())) } }
                                    }
                                    if e.field == "date" {
                                        div { class: "field", label { "{t(lang, \"stamps.format\")}" }
                                            select { onchange: move |ev| set(Box::new(move |x| x.format = ev.value())),
                                                for f in formats.iter().cloned() { option { key: "{f}", value: "{f}", selected: e.format == f, "{f}" } }
                                            }
                                        }
                                    }
                                    if e.field == "counter" {
                                        div { class: "field", label { "{t(lang, \"stamps.digits\")}" }
                                            input { r#type: "number", min: "1", max: "12", value: "{e.format}", oninput: move |ev| set(Box::new(move |x| x.format = ev.value())) } }
                                    }
                                }
                                if e.kind == "text" || e.kind == "field" {
                                    div { class: "field-grid three",
                                        div { class: "field", label { "{t(lang, \"stamps.size\")}" }
                                            input { r#type: "number", min: "2", value: "{e.font_size}", oninput: move |ev| { let v = num(&ev.value(), 12.0); set(Box::new(move |x| x.font_size = v)) } } }
                                        div { class: "field", label { "{t(lang, \"stamps.align\")}" }
                                            select { onchange: move |ev| set(Box::new(move |x| x.align = ev.value())),
                                                for a in ["left", "center", "right"] { option { key: "{a}", value: "{a}", selected: e.align == a, "{align_name(lang, a)}" } }
                                            }
                                        }
                                        label { class: "check", input { r#type: "checkbox", checked: e.bold, onchange: move |_| set(Box::new(|x| x.bold = !x.bold)) } span { "{t(lang, \"stamps.bold\")}" } }
                                    }
                                }
                                if matches!(e.kind.as_str(), "line" | "polyline" | "polygon" | "curve") {
                                    div { class: "field", label { "{t(lang, \"stamps.points\")}" }
                                        input { value: "{points_text(&e.points)}", onchange: move |ev| { let p = parse_points(&ev.value()); if p.len() >= 2 { set(Box::new(move |x| x.points = p)); } } } }
                                } else {
                                    div { class: "field-grid four",
                                        for (k, idx) in [("X", 0usize), ("Y", 1), ("W", 2), ("H", 3)] {
                                            div { class: "field", key: "{k}", label { "{k}" }
                                                input { r#type: "number", value: "{e.rect[idx]}", oninput: move |ev| { let v = num(&ev.value(), 0.0); set(Box::new(move |x| x.rect[idx] = v)) } } }
                                        }
                                    }
                                }
                                div { class: "field-grid three",
                                    div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" }
                                        input { r#type: "color", value: "{color_hex(e.color)}", oninput: move |ev| { let c = parse_color(&ev.value()); set(Box::new(move |x| x.color = c)) } } }
                                    if matches!(e.kind.as_str(), "rect" | "oval" | "polygon") {
                                        div { class: "field", label { "{t(lang, \"stamps.fill\")}" }
                                            div { class: "row", style: "gap:4px",
                                                input { r#type: "checkbox", checked: e.fill.is_some(), onchange: move |_| set(Box::new(|x| x.fill = if x.fill.is_some() { None } else { Some([1.0, 1.0, 1.0]) })) }
                                                if let Some(fl) = e.fill {
                                                    input { r#type: "color", value: "{color_hex(fl)}", oninput: move |ev| { let c = parse_color(&ev.value()); set(Box::new(move |x| x.fill = Some(c))) } }
                                                }
                                            }
                                        }
                                    }
                                    if !matches!(e.kind.as_str(), "text" | "field" | "image") {
                                        div { class: "field", label { "{t(lang, \"stamps.line_width\")}" }
                                            input { r#type: "number", min: "0", step: "0.5", value: "{e.line_width}", oninput: move |ev| { let v = num(&ev.value(), 1.0); set(Box::new(move |x| x.line_width = v)) } } }
                                    }
                                    if e.kind == "rect" {
                                        div { class: "field", label { "{t(lang, \"stamps.radius\")}" }
                                            input { r#type: "number", min: "0", value: "{e.radius}", oninput: move |ev| { let v = num(&ev.value(), 0.0); set(Box::new(move |x| x.radius = v)) } } }
                                    }
                                    div { class: "field", label { "{t(lang, \"dialog.watermark.opacity\")}" }
                                        input { r#type: "number", min: "5", max: "100", value: "{(e.opacity * 100.0).round()}", oninput: move |ev| { let v = (num(&ev.value(), 100.0) / 100.0).clamp(0.05, 1.0); set(Box::new(move |x| x.opacity = v)) } } }
                                }
                            }
                        }
                        if d.formats {
                            div { class: "stamp-props",
                                div { class: "field", label { "{label(\"stampCustom.manage.dateFormats\")}" }
                                    div { class: "hint", "{t(lang, \"stamps.formats_hint\")}" }
                                    input { value: "{st.ui.read().value(\"stamps.dateFormats\").unwrap_or(\"\")}", placeholder: "DD MMM YYYY|YYYY/MM/DD",
                                        onchange: move |ev| st.set_value("stamps.dateFormats", ev.value()) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
