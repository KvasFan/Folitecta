//! The picture editor, editing in another app, its settings and the toolbar
//! of the selected picture on the page.

use std::time::Duration;

use dioxus::prelude::*;
use folitecta_core::imaging::Format;
use folitecta_core::registry;

use crate::commands::dispatch;
use crate::commands::option_on;
use crate::commands::pictures::{
    EDIT_TYPES, EDITOR, EXTERNAL, SESSION, ZOOMS, apply_editor, edit_type, editor_choice, external_changed,
    fit_zoom, undo_in_editor, update_from_outside,
};
use crate::i18n::{Lang, cmd_label, t};
use crate::icons::Icon;
use crate::state::{AppState, Dialog, ToastKind};
use crate::ui::dialogs::Frame;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

/// The convert command of a pixel format.
fn format_command(f: Format) -> &'static str {
    match f {
        Format::Bilevel => "imageEditor.convert.1bpp_indexed",
        Format::Indexed => "imageEditor.convert.8bpp_indexed",
        Format::Gray => "imageEditor.convert.8bpp_grayscale",
        Format::GrayAlpha => "imageEditor.convert.grayscaleAlpha",
        Format::Rgb => "imageEditor.convert.24bpp",
        Format::Rgba => "imageEditor.convert.32bpp",
    }
}

const FILTERS: &[&str] = &[
    "imageEditor.filters.smooth",
    "imageEditor.filters.gaussianBlur",
    "imageEditor.filters.sharpen",
    "imageEditor.filters.meanRemoval",
    "imageEditor.filters.despeckle",
    "imageEditor.filters.edgeDetect",
    "imageEditor.filters.embossLaplascian",
    "imageEditor.filters.embossHorzVert",
    "imageEditor.filters.embossAllDirections",
    "imageEditor.filters.embossLossy",
];

const FORMATS: &[Format] = &[
    Format::Rgb,
    Format::Rgba,
    Format::Gray,
    Format::GrayAlpha,
    Format::Indexed,
    Format::Bilevel,
];

/// Edits of the toolbar: command and icon.
const TOOLS: &[(&str, &str)] = &[
    ("imageEditor.rotate90CCW", "rotate-ccw"),
    ("imageEditor.rotate90CW", "rotate"),
    ("imageEditor.flipHorizontal", "flip-h"),
    ("imageEditor.flipVertical", "flip-v"),
    ("imageEditor.grayscale", "gray"),
    ("imageEditor.crop", "crop"),
    ("imageEditor.deskew", "deskew"),
];

#[component]
pub fn ImageEditorDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let v = EDITOR();
    // The size of the visible area, once it is shown.
    use_effect(move || {
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(40)).await;
            let js = "const s = document.querySelector('.imged-stage'); return s ? [s.clientWidth, s.clientHeight] : [0, 0];";
            if let Ok(r) = document::eval(js).await {
                if let Some(a) = r.as_array() {
                    let w = a.first().and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    let h = a.get(1).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    EDITOR.write().stage = (w, h);
                }
            }
        });
    });
    // However the editor closes, the picture it held goes.
    use_drop(|| {
        if let Ok(mut s) = SESSION.lock() {
            *s = None;
        }
    });
    // Where the drag that draws the crop area started (picture pixels).
    let mut drag_from = use_signal(|| None::<(f32, f32)>);
    let (pw, ph) = v.size;
    let (w, h) = (pw as f32, ph as f32);
    let z = v.zoom.unwrap_or_else(|| fit_zoom(v.size, v.stage));
    let (cw, ch) = (w * z, h * z);
    let pct = (z * 100.0).round();
    let format_name = v
        .format
        .map(|f| label_of(lang, format_command(f)))
        .unwrap_or_default();
    let busy = v.busy;
    let point = move |e: &Event<MouseData>| {
        let p = e.element_coordinates();
        ((p.x as f32 / z).clamp(0.0, w), (p.y as f32 / z).clamp(0.0, h))
    };
    rsx! {
        Frame { title: label_of(lang, "imageEditor"), wide: true, ok_label: Some(t(lang, "picture.apply")),
            on_ok: move |_| apply_editor(st),
            div { class: "pane-toolbar icon-toolbar imged-toolbar",
                button { class: "ibtn", title: "{label_of(lang, \"undo\")}", disabled: !v.can_undo || busy, onclick: move |_| undo_in_editor(), Icon { name: "undo" } }
                span { class: "gap" }
                for (id, icon) in TOOLS.iter().copied() {
                    button { class: if id == "imageEditor.crop" && v.cropping { "ibtn on" } else { "ibtn" }, key: "{id}", title: "{label_of(lang, id)}", disabled: busy,
                        onclick: move |_| dispatch(st, id), Icon { name: icon } }
                }
                span { class: "gap" }
                button { class: if v.menu == Some("filters") { "tbtn on" } else { "tbtn" }, disabled: busy, onclick: move |_| dispatch(st, "imageEditor.filters.rcm"),
                    Icon { name: "filter" } span { "{label_of(lang, \"imageEditor.filters\")}" } }
                button { class: if v.menu == Some("convert") { "tbtn on" } else { "tbtn" }, disabled: busy, onclick: move |_| dispatch(st, "imageEditor.convert.rcm"),
                    Icon { name: "palette" } span { "{label_of(lang, \"imageEditor.convert\")}" } }
                div { class: "spacer" }
                button { class: "ibtn", title: "{label_of(lang, \"imageEditor.zoomOut\")}", onclick: move |_| dispatch(st, "imageEditor.zoomOut"), Icon { name: "zoom-out" } }
                select { id: "imged-zoom", class: "imged-zoom", title: "{label_of(lang, \"imageEditor.zoom.combo\")}",
                    onchange: move |e| {
                        let value = e.value();
                        EDITOR.write().zoom = if value == "fit" { None } else { value.parse::<f32>().ok().map(|p| p / 100.0) };
                    },
                    option { value: "fit", selected: v.zoom.is_none(), "{label_of(lang, \"imageEditor.fitVisible\")}" }
                    for level in ZOOMS.iter().copied() {
                        option { key: "{level}", value: "{level * 100.0}", selected: v.zoom == Some(level), "{(level * 100.0).round()}%" }
                    }
                }
                button { class: "ibtn", title: "{label_of(lang, \"imageEditor.zoomIn\")}", onclick: move |_| dispatch(st, "imageEditor.zoomIn"), Icon { name: "zoom-in" } }
                button { class: if v.zoom.is_none() { "ibtn on" } else { "ibtn" }, title: "{label_of(lang, \"imageEditor.fitVisible\")}", onclick: move |_| dispatch(st, "imageEditor.fitVisible"), Icon { name: "fit" } }
            }
            if v.menu == Some("filters") {
                div { class: "imged-menu",
                    for id in FILTERS.iter().copied() {
                        button { class: "tbtn", key: "{id}", disabled: busy, onclick: move |_| dispatch(st, id), "{label_of(lang, id)}" }
                    }
                }
            }
            if v.menu == Some("convert") {
                div { class: "imged-menu",
                    for f in FORMATS.iter().copied() {
                        {
                            let id = format_command(f);
                            rsx! {
                                button { class: if v.format == Some(f) { "tbtn on" } else { "tbtn" }, key: "{id}", disabled: busy, onclick: move |_| dispatch(st, id), "{label_of(lang, id)}" }
                            }
                        }
                    }
                }
            }
            if v.menu == Some("zoom") {
                div { class: "imged-menu",
                    for level in ZOOMS.iter().copied() {
                        button { class: if v.zoom == Some(level) { "tbtn on" } else { "tbtn" }, key: "{level}", onclick: move |_| EDITOR.write().zoom = Some(level), "{(level * 100.0).round()}%" }
                    }
                }
            }
            div { class: if v.cropping { "imged-stage cropping" } else { "imged-stage" },
                div { class: "imged-canvas", style: "width:{cw}px;height:{ch}px",
                    onmousedown: move |e| {
                        if !EDITOR.peek().cropping || busy {
                            return;
                        }
                        e.prevent_default();
                        // The area drawn so far stays until the pointer moves.
                        drag_from.set(Some(point(&e)));
                    },
                    onmousemove: move |e| {
                        let Some((x0, y0)) = drag_from() else { return };
                        let (x, y) = point(&e);
                        EDITOR.write().crop = Some([x0, y0, x, y]);
                    },
                    onmouseup: move |_| drag_from.set(None),
                    onmouseleave: move |_| drag_from.set(None),
                    ondoubleclick: move |_| {
                        if EDITOR.peek().cropping && EDITOR.peek().crop.is_some() {
                            dispatch(st, "imageEditor.crop");
                        }
                    },
                    img { class: "imged-img", src: "/imgedit/{v.generation}.png", alt: "", draggable: "false" }
                    if let Some(r) = v.crop {
                        div { class: "imged-crop",
                            style: "left:{r[0].min(r[2]) * z}px;top:{r[1].min(r[3]) * z}px;width:{(r[2] - r[0]).abs() * z}px;height:{(r[3] - r[1]).abs() * z}px" }
                    }
                }
                if busy {
                    div { class: "imged-busy", "{t(lang, \"picture.working\")}" }
                }
            }
            p { class: "hint",
                "{pw} × {ph} px · {format_name} · {pct}%"
                if v.cropping { " · {t(lang, \"picture.crop_hint\")}" }
            }
        }
    }
}

/// The toolbar above the picture selected on the page.
#[component]
pub fn PictureToolbar(left: f32, top: f32) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let act = move |id: &'static str| {
        move |e: Event<MouseData>| {
            e.stop_propagation();
            dispatch(st, id);
        }
    };
    rsx! {
        div { class: "sel-toolbar picture-toolbar", style: "left:{left}px;top:{top}px", onmousedown: move |e| e.stop_propagation(),
            button { title: "{label_of(lang, \"editImage\")}", onclick: move |e| { e.stop_propagation(); crate::commands::pictures::edit_selected(st); }, Icon { name: "pencil" } }
            for (id, icon) in TOOLS.iter().copied().filter(|(id, _)| *id != "imageEditor.crop") {
                button { key: "{id}", title: "{label_of(lang, id)}", onclick: act(id), Icon { name: icon } }
            }
            button { title: "{label_of(lang, \"imageEditor.crop\")}", onclick: act("imageEditor.crop"), Icon { name: "crop" } }
        }
    }
}

/// A picture being edited in another app.
#[component]
pub fn ExternalPictureDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut tick = use_signal(|| 0u32);
    // Look for the saved picture every second.
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            *tick.write() += 1;
        }
    });
    let _ = tick();
    let x = EXTERNAL();
    let path = x.as_ref().map(|x| x.path.clone());
    let shown = path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let changed = external_changed();
    let reopen = path.clone();
    let kept = path.clone();
    use_drop(move || {
        if let Some(p) = &kept {
            let _ = std::fs::remove_file(p);
        }
    });
    rsx! {
        Frame { title: label_of(lang, "editImage"), ok_label: Some(t(lang, "picture.update")),
            on_ok: move |_| {
                if external_changed() {
                    update_from_outside(st);
                } else {
                    st.toast(ToastKind::Info, t(st.lang(), "picture.external_waiting"), "");
                }
            },
            p { class: "hint", "{t(lang, \"picture.external_hint\")}" }
            div { class: "kv",
                span { class: "k", "{t(lang, \"picture.file\")}" }
                span { class: "v", "{shown}" }
            }
            p { class: if changed { "picture-state changed" } else { "picture-state" },
                if changed { "{t(lang, \"picture.external_saved\")}" } else { "{t(lang, \"picture.external_waiting\")}" }
            }
            div { class: "row",
                button { class: "tbtn", onclick: move |_| {
                        if let Some(p) = &reopen {
                            crate::commands::pictures::launch(st, p);
                        }
                    },
                    "{t(lang, \"picture.open_again\")}"
                }
            }
        }
    }
}

/// How pictures are edited.
#[component]
pub fn PictureSettingsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut choice = use_signal(|| editor_choice(st));
    let mut app = use_signal(|| st.value("image.editor.app"));
    let mut kind = use_signal(|| edit_type(st));
    let mut all = use_signal(|| option_on(st, "editImage.applyToAll"));
    let choices = [
        ("builtin", t(lang, "picture.editor.builtin")),
        ("default", t(lang, "picture.editor.default")),
        ("app", t(lang, "picture.editor.app")),
    ];
    rsx! {
        Frame { title: label_of(lang, "editImage"),
            on_ok: move |_| {
                let chosen = if choice() == "app" && app().trim().is_empty() { "builtin".to_string() } else { choice() };
                st.set_value("image.editor", chosen);
                st.set_value("image.editor.app", app().trim().to_string());
                st.set_value("image.editType", kind());
                st.set_option("editImage.applyToAll", all());
                st.set_dialog(Dialog::None);
            },
            div { class: "radio-col",
                for (key, text) in choices {
                    label { class: "check", key: "{key}",
                        input { r#type: "radio", name: "picture-editor", checked: choice() == key, onchange: move |_| choice.set(key.to_string()) }
                        span { "{text}" }
                    }
                }
            }
            div { class: "field", label { "{t(lang, \"picture.app\")}" }
                div { class: "row",
                    input { value: "{app}", oninput: move |e| app.set(e.value()) }
                    button { class: "tbtn", onclick: move |_| {
                            spawn(async move {
                                if let Some(f) = rfd::AsyncFileDialog::new().pick_file().await {
                                    app.set(f.path().display().to_string());
                                    choice.set("app".into());
                                }
                            });
                        },
                        "{t(lang, \"picture.browse\")}"
                    }
                }
            }
            div { class: "field", label { "{t(lang, \"picture.file_type\")}" }
                select { onchange: move |e| kind.set(e.value()),
                    for e in EDIT_TYPES.iter().copied() {
                        option { key: "{e}", value: "{e}", selected: kind() == e, "{e.to_uppercase()}" }
                    }
                }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: all(), onchange: move |_| all.set(!all()) }
                span { "{label_of(lang, \"editImage.applyToAll\")}" }
            }
            p { class: "hint", "{t(lang, \"picture.apply_all_hint\")}" }
        }
    }
}
