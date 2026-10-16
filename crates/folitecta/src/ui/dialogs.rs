//! Modal dialogs.

use dioxus::prelude::*;
use folitecta_core::commands::{MENUS, RIBBON_GROUPS, RIBBON_TABS};
use folitecta_core::marks::{MarkPosition, TextMarkOptions};
use folitecta_core::{ImageFormat, registry};

use crate::app::icon_data_url;
use crate::commands::{implemented_ids, is_implemented, is_partial, partial_ids};
use crate::files;
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::state::{AppState, ConfirmAction, Dialog, Theme, ToastKind, Zoom};
use crate::util::{color_hex, format_pages, parse_color, parse_page_range};

#[component]
fn Frame(
    title: String,
    children: Element,
    on_ok: Option<EventHandler<()>>,
    #[props(default = false)] wide: bool,
    #[props(default = None)] ok_label: Option<String>,
    #[props(default = false)] danger: bool,
) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let ok_text = ok_label.unwrap_or_else(|| t(lang, "dialog.ok"));
    let cancel_text = if on_ok.is_some() {
        t(lang, "dialog.cancel")
    } else {
        t(lang, "dialog.close")
    };
    let ok2 = on_ok;
    rsx! {
        div { class: "backdrop", onclick: move |_| st.set_dialog(Dialog::None),
            div { class: if wide { "dialog wide" } else { "dialog" }, onclick: move |e| e.stop_propagation(),
                onkeydown: move |e| {
                    e.stop_propagation();
                    if e.key() == Key::Enter {
                        if let Some(h) = ok2 { h.call(()); }
                    } else if e.key() == Key::Escape {
                        st.set_dialog(Dialog::None);
                    }
                },
                div { class: "dialog-head", "{title}" div { class: "spacer" } button { class: "ibtn", onclick: move |_| st.set_dialog(Dialog::None), "×" } }
                div { class: "dialog-body", {children} }
                div { class: "dialog-foot",
                    button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::None), "{cancel_text}" }
                    if let Some(h) = on_ok {
                        button { class: if danger { "tbtn danger" } else { "tbtn primary" }, onclick: move |_| h.call(()), "{ok_text}" }
                    }
                }
            }
        }
    }
}

#[component]
fn Text(
    label: String,
    value: Signal<String>,
    #[props(default = String::new())] placeholder: String,
    #[props(default = false)] autofocus: bool,
) -> Element {
    let mut value = value;
    rsx! {
        div { class: "field",
            label { "{label}" }
            input { value: "{value}", placeholder: "{placeholder}", autofocus: autofocus, oninput: move |e| value.set(e.value()) }
        }
    }
}

#[component]
fn Num(
    label: String,
    value: Signal<String>,
    #[props(default = String::new())] min: String,
    #[props(default = String::new())] max: String,
    #[props(default = "1".to_string())] step: String,
) -> Element {
    let mut value = value;
    rsx! {
        div { class: "field",
            label { "{label}" }
            input { r#type: "number", value: "{value}", min: "{min}", max: "{max}", step: "{step}", oninput: move |e| value.set(e.value()) }
        }
    }
}

#[component]
fn Area(label: String, value: Signal<String>, #[props(default = false)] autofocus: bool) -> Element {
    let mut value = value;
    rsx! {
        div { class: "field",
            label { "{label}" }
            textarea { value: "{value}", autofocus: autofocus, oninput: move |e| value.set(e.value()) }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Current,
    Selected,
    All,
    Range,
}

/// Page scope selector shared by page dialogs.
#[component]
fn PageScope(mode: Signal<Scope>, range: Signal<String>) -> Element {
    let mut mode = mode;
    let mut range = range;
    let st = use_context::<AppState>();
    let lang = st.lang();
    let selected = st.read_active(|t| t.selected_pages.clone()).unwrap_or_default();
    let has_sel = !selected.is_empty();
    let count = st.page_count();
    rsx! {
        div { class: "field",
            label { "{t(lang, \"dialog.pages\")}" }
            div { class: "radio-row",
                label { input { r#type: "radio", checked: mode() == Scope::Current, onchange: move |_| mode.set(Scope::Current) } "{t(lang, \"dialog.pages.current\")}" }
                if has_sel {
                    label { input { r#type: "radio", checked: mode() == Scope::Selected, onchange: move |_| mode.set(Scope::Selected) } "{t(lang, \"dialog.pages.selected\")} ({format_pages(&selected)})" }
                }
                label { input { r#type: "radio", checked: mode() == Scope::All, onchange: move |_| mode.set(Scope::All) } "{t(lang, \"dialog.pages.all\")} ({count})" }
                label { input { r#type: "radio", checked: mode() == Scope::Range, onchange: move |_| mode.set(Scope::Range) } "1-3, 7…" }
            }
            if mode() == Scope::Range {
                input { value: "{range}", placeholder: "1-3, 7", oninput: move |e| range.set(e.value()) }
                div { class: "hint", "{t(lang, \"dialog.pages.hint\")}" }
            }
        }
    }
}

fn scope_pages(st: AppState, mode: Scope, range: &str) -> Result<Vec<usize>, String> {
    let count = st.page_count();
    match mode {
        Scope::Current => Ok(vec![st.current_page()]),
        Scope::Selected => Ok(st.read_active(|t| t.target_pages()).unwrap_or_default()),
        Scope::All => Ok((0..count).collect()),
        Scope::Range => parse_page_range(range, count),
    }
}

fn default_scope(st: AppState) -> Scope {
    if st.read_active(|t| !t.selected_pages.is_empty()).unwrap_or(false) {
        Scope::Selected
    } else {
        Scope::Current
    }
}

/// Apply a document operation on the active tab and close the dialog.
fn apply(st: AppState, f: impl FnOnce(&mut crate::state::DocTab) -> folitecta_core::Result<()>) {
    match st.with_active(f) {
        Some(Ok(())) => {
            files::set_window_title(st);
            st.set_dialog(Dialog::None);
        }
        Some(Err(e)) => st.error(e),
        None => st.set_dialog(Dialog::None),
    }
}

#[component]
pub fn Dialogs() -> Element {
    let st = use_context::<AppState>();
    let dialog = (st.dialog)();
    match dialog {
        Dialog::None => rsx! {},
        Dialog::About => rsx! { About {} },
        Dialog::Shortcuts => rsx! { Shortcuts {} },
        Dialog::Matrix => rsx! { Matrix {} },
        Dialog::Prefs => rsx! { Prefs {} },
        Dialog::CommandLine => rsx! { CommandLine {} },
        Dialog::DocProps => rsx! { DocProps {} },
        Dialog::GoToPage => rsx! { GoToPage {} },
        Dialog::InsertBlank => rsx! { InsertBlank {} },
        Dialog::RotatePages => rsx! { RotatePages {} },
        Dialog::DeletePages => rsx! { DeletePages {} },
        Dialog::ExtractPages => rsx! { ExtractPages {} },
        Dialog::CropPages => rsx! { CropPages {} },
        Dialog::ResizePages => rsx! { ResizePages {} },
        Dialog::AddMargins => rsx! { AddMargins {} },
        Dialog::MovePages => rsx! { MovePages {} },
        Dialog::SwapPages => rsx! { SwapPages {} },
        Dialog::Watermark => rsx! { Watermark {} },
        Dialog::HeaderFooter => rsx! { HeaderFooter {} },
        Dialog::PageNumbers => rsx! { PageNumbers { bates: false } },
        Dialog::Bates => rsx! { PageNumbers { bates: true } },
        Dialog::RemoveMarks => rsx! { RemoveMarks {} },
        Dialog::ExportImages => rsx! { ExportImages {} },
        Dialog::Split => rsx! { Split {} },
        Dialog::MergeFile(path) => rsx! { MergeFile { path: path.display().to_string() } },
        Dialog::NotImplemented(id) => rsx! { NotImplemented { id } },
        Dialog::Confirm { title, body, action } => rsx! { Confirm { title, body, action } },
        Dialog::Unsaved { tab, then } => rsx! { Unsaved { tab, then } },
        Dialog::Password(path) => rsx! { Password { path: path.display().to_string() } },
        Dialog::Note { page, x, y } => rsx! { NoteDialog { page, x, y } },
        Dialog::TextBox { page, rect } => rsx! { TextBoxDialog { page, rect: rect.to_vec() } },
        Dialog::Link { page, rect } => rsx! { LinkDialog { page, rect: rect.to_vec() } },
        Dialog::FieldValue { id, name, value } => {
            rsx! { FieldValue { id_num: id.0, id_gen: id.1, name, value } }
        }
        Dialog::BookmarkNew => rsx! { BookmarkNew {} },
        Dialog::BookmarkRename(path) => rsx! { BookmarkRename { path } },
        Dialog::AnnotationText {
            page,
            index,
            contents,
        } => rsx! { AnnotationText { page, index, contents } },
        Dialog::WordCount { words, chars } => rsx! { WordCount { words, chars } },
        Dialog::UrlReport(urls) => rsx! { UrlReport { urls } },
        Dialog::Summary(text) => rsx! { Summary { text } },
    }
}

#[component]
fn About() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let icon = icon_data_url();
    rsx! {
        Frame { title: t(lang, "dialog.about.title"),
            div { class: "about",
                img { src: "{icon}", alt: "" }
                div {
                    h2 { "Folitecta" }
                    div { class: "ver", "Version {folitecta_core::VERSION}" }
                    p { "{t(lang, \"dialog.about.body\")}" }
                    p { "{t(lang, \"dialog.about.license\")}" }
                    ul {
                        li { "Rendering: hayro (Apache-2.0 OR MIT)" }
                        li { "Document model: lopdf (MIT)" }
                        li { "User interface: Dioxus (MIT OR Apache-2.0)" }
                        li { a { href: "https://github.com/KvasExpert/Folitecta", onclick: move |e| { e.prevent_default(); let _ = crate::util::open_external("https://github.com/KvasExpert/Folitecta"); }, "github.com/KvasExpert/Folitecta" } }
                    }
                    div { class: "row", style: "gap:8px; margin-top: 8px",
                        button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::Shortcuts), "{t(lang, \"dialog.shortcuts.title\")}" }
                        button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::Matrix), "{t(lang, \"dialog.matrix.title\")}" }
                    }
                }
            }
        }
    }
}

#[component]
fn Shortcuts() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let mut rows: Vec<(String, String)> = reg
        .all()
        .iter()
        .filter_map(|c| c.shortcut.clone().map(|s| (s, cmd_label(lang, c))))
        .collect();
    rows.push(("Ctrl+Shift+P".into(), t(lang, "palette.placeholder")));
    rows.push(("Ctrl+Wheel".into(), t(lang, "status.zoom")));
    rows.push(("Esc".into(), t(lang, "tool.hand")));
    rows.sort();
    rsx! {
        Frame { title: t(lang, "dialog.shortcuts.title"),
            div { class: "matrix", table { thead { tr { th { "Shortcut" } th { "Command" } } } tbody { for (k, v) in rows { tr { key: "{k}", td { class: "mono", "{k}" } td { "{v}" } } } } } }
        }
    }
}

#[component]
fn Matrix() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let (done, partial, planned) = reg.status_summary(&implemented_ids(), &partial_ids());
    rsx! {
        Frame { title: t(lang, "dialog.matrix.title"), wide: true,
            div { class: "row", style: "gap: 10px",
                span { class: "badge ok", "{done} {t(lang, \"label.implemented\").to_lowercase()}" }
                span { class: "badge partial", "{partial} {t(lang, \"label.partial\").to_lowercase()}" }
                span { class: "badge planned", "{planned} {t(lang, \"label.planned\").to_lowercase()}" }
                span { class: "muted", "docs/FEATURES.md" }
            }
            div { class: "matrix",
                table {
                    thead { tr { th { "Tab / Group" } th { "Command" } th { "Status" } } }
                    tbody {
                        for (tab, tab_label) in RIBBON_TABS.iter() {
                            for (_, g, g_label) in RIBBON_GROUPS.iter().filter(|(t, _, _)| t == tab) {
                                for c in reg.ribbon_group(tab, g) {
                                    tr { key: "{c.id}",
                                        td { "{tab_label} › {g_label}" }
                                        td { "{c.label}" }
                                        td { if is_partial(&c.id) { span { class: "badge partial", "partial" } } else if is_implemented(&c.id) { span { class: "badge ok", "available" } } else { span { class: "badge planned", "planned" } } }
                                    }
                                }
                            }
                        }
                        for (menu, menu_label) in MENUS.iter() {
                            for c in reg.menu_entries(menu).into_iter().flatten() {
                                tr { key: "m-{c.id}",
                                    td { "{menu_label}" }
                                    td { "{c.label}" }
                                    td { if is_partial(&c.id) { span { class: "badge partial", "partial" } } else if is_implemented(&c.id) { span { class: "badge ok", "available" } } else { span { class: "badge planned", "planned" } } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Prefs() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let prefs = st.ui.read().clone();
    let author = use_signal(|| prefs.author.clone());
    rsx! {
        Frame { title: t(lang, "dialog.prefs.title"), on_ok: move |_| { st.ui_mut().author = author(); st.save_prefs(); st.set_dialog(Dialog::None); },
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.prefs.theme\")}" }
                    select { onchange: move |e| { st.ui_mut().theme = if e.value() == "dark" { Theme::Dark } else { Theme::Light }; st.save_prefs(); },
                        option { value: "light", selected: prefs.theme == Theme::Light, "{t(lang, \"dialog.prefs.theme.light\")}" }
                        option { value: "dark", selected: prefs.theme == Theme::Dark, "{t(lang, \"dialog.prefs.theme.dark\")}" }
                    }
                }
                div { class: "field", label { "{t(lang, \"dialog.prefs.language\")}" }
                    select { onchange: move |e| { st.ui_mut().lang = Lang::from_code(&e.value()); st.save_prefs(); },
                        for l in Lang::ALL { option { value: "{l.code()}", selected: l == prefs.lang, "{l.name()}" } }
                    }
                }
                div { class: "field", label { "{t(lang, \"dialog.prefs.ui\")}" }
                    select { onchange: move |e| { st.ui_mut().ribbon = e.value() == "ribbon"; st.save_prefs(); },
                        option { value: "ribbon", selected: prefs.ribbon, "{t(lang, \"dialog.prefs.ui.ribbon\")}" }
                        option { value: "classic", selected: !prefs.ribbon, "{t(lang, \"dialog.prefs.ui.classic\")}" }
                    }
                }
                Text { label: t(lang, "dialog.prefs.author"), value: author }
            }
        }
    }
}

#[component]
fn CommandLine() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    rsx! {
        Frame { title: t(lang, "cmd.help.cmdline"),
            pre { class: "mono", style: "user-select:text; white-space: pre-wrap",
                "folitecta [FILE.pdf ...]        open files\nfolitecta --features-md OUT.md  write the feature matrix\nfolitecta --version\nfolitecta --help"
            }
        }
    }
}

#[component]
fn DocProps() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let info = st.read_active(|t| t.doc.info()).unwrap_or_default();
    let title = use_signal(|| info.title.clone());
    let author = use_signal(|| info.author.clone());
    let subject = use_signal(|| info.subject.clone());
    let keywords = use_signal(|| info.keywords.clone());
    let path = info
        .path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| t(lang, "label.untitled"));
    let yes_no = |b: bool| {
        if b {
            t(lang, "label.yes")
        } else {
            t(lang, "label.no")
        }
    };
    rsx! {
        Frame { title: t(lang, "dialog.props.title"), wide: true,
            on_ok: move |_| apply(st, |tab| tab.doc.set_info(&title(), &author(), &subject(), &keywords())),
            div { class: "field-grid",
                Text { label: t(lang, "dialog.props.title_field"), value: title, autofocus: true }
                Text { label: t(lang, "dialog.props.author"), value: author }
                Text { label: t(lang, "dialog.props.subject"), value: subject }
                Text { label: t(lang, "dialog.props.keywords"), value: keywords }
            }
            div { class: "kv",
                span { class: "k", "{t(lang, \"dialog.props.path\")}" } span { class: "v", "{path}" }
                span { class: "k", "{t(lang, \"dialog.props.pages\")}" } span { class: "v", "{info.page_count}" }
                span { class: "k", "{t(lang, \"dialog.props.size\")}" } span { class: "v", "{crate::util::human_size(info.file_size)}" }
                span { class: "k", "{t(lang, \"dialog.props.version\")}" } span { class: "v", "{info.version}" }
                span { class: "k", "{t(lang, \"dialog.props.created\")}" } span { class: "v", "{info.created}" }
                span { class: "k", "{t(lang, \"dialog.props.modified\")}" } span { class: "v", "{info.modified}" }
                span { class: "k", "{t(lang, \"dialog.props.creator\")}" } span { class: "v", "{info.creator}" }
                span { class: "k", "{t(lang, \"dialog.props.producer\")}" } span { class: "v", "{info.producer}" }
                span { class: "k", "{t(lang, \"dialog.props.encrypted\")}" } span { class: "v", "{yes_no(info.encrypted)}" }
                span { class: "k", "{t(lang, \"dialog.props.form\")}" } span { class: "v", "{yes_no(info.has_form)}" }
                span { class: "k", "{t(lang, \"dialog.props.tagged\")}" } span { class: "v", "{yes_no(info.tagged)}" }
            }
        }
    }
}

#[component]
fn GoToPage() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let page = use_signal(|| (st.current_page() + 1).to_string());
    let zoom = use_signal(|| ((crate::commands::current_zoom(st) * 100.0).round() as i32).to_string());
    let count = st.page_count();
    rsx! {
        Frame { title: t(lang, "dialog.goto.title"),
            on_ok: move |_| {
                if let Ok(p) = page().trim().parse::<usize>() { if p >= 1 { st.go_to_page(p - 1); } }
                if let Ok(z) = zoom().trim().parse::<f32>() { if z >= 10.0 { st.with_active(|t| t.zoom = Zoom::Fixed(z / 100.0)); } }
                st.set_dialog(Dialog::None);
            },
            div { class: "field-grid",
                Num { label: format!("{} (1-{count})", t(lang, "status.page")), value: page, min: "1", max: count.to_string() }
                Num { label: format!("{} (%)", t(lang, "status.zoom")), value: zoom, min: "10", max: "800" }
            }
        }
    }
}

#[component]
fn InsertBlank() -> Element {
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
fn RotatePages() -> Element {
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
fn DeletePages() -> Element {
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
fn ExtractPages() -> Element {
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
fn CropPages() -> Element {
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
                Num { label: "Top".to_string(), value: top, min: "0", step: "1" }
                Num { label: t(lang, "dialog.hf.right"), value: right, min: "0", step: "1" }
                Num { label: "Bottom".to_string(), value: bottom, min: "0", step: "1" }
            }
            label { class: "check", input { r#type: "checkbox", checked: reset(), onchange: move |_| reset.set(!reset()) } span { "{t(lang, \"cmd.cropPage.hideAllBoxes\").replace(\"Hide All Boxes\", \"Remove existing crop\")}" } }
            PageScope { mode, range }
        }
    }
}

#[component]
fn ResizePages() -> Element {
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
                Num { label: "Width (pt)".to_string(), value: width, min: "10", step: "1" }
                Num { label: "Height (pt)".to_string(), value: height, min: "10", step: "1" }
            }
            div { class: "hint", "A4 = 595 × 842 pt · Letter = 612 × 792 pt · A3 = 842 × 1191 pt" }
            label { class: "check", input { r#type: "checkbox", checked: scale(), onchange: move |_| scale.set(!scale()) } span { "{t(lang, \"dialog.resize.scale\")}" } }
            PageScope { mode, range }
        }
    }
}

#[component]
fn AddMargins() -> Element {
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
                Num { label: "Top".to_string(), value: top, step: "1" }
                Num { label: t(lang, "dialog.hf.right"), value: right, step: "1" }
                Num { label: "Bottom".to_string(), value: bottom, step: "1" }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
fn MovePages() -> Element {
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
            Num { label: format!("Move before page (1-{})", count + 1), value: to, min: "1", max: (count + 1).to_string() }
        }
    }
}

#[component]
fn SwapPages() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let a = use_signal(|| (st.current_page() + 1).to_string());
    let b = use_signal(|| "1".to_string());
    rsx! {
        Frame { title: cmd_label(lang, registry().get("document.swapPages").unwrap()),
            on_ok: move |_| { let pa: usize = a().trim().parse().unwrap_or(1); let pb: usize = b().trim().parse().unwrap_or(1); apply(st, |tab| tab.doc.swap_pages(pa.saturating_sub(1), pb.saturating_sub(1))); },
            div { class: "field-grid", Num { label: "Page A".to_string(), value: a, min: "1" } Num { label: "Page B".to_string(), value: b, min: "1" } }
        }
    }
}

#[component]
fn Watermark() -> Element {
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
            div { class: "field-grid three",
                Num { label: t(lang, "dialog.watermark.size"), value: size, min: "6", max: "400" }
                Num { label: t(lang, "dialog.watermark.opacity"), value: opacity, min: "5", max: "100" }
                Num { label: t(lang, "dialog.watermark.rotation"), value: rotation, min: "-180", max: "180" }
            }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.watermark.position\")}" }
                    select { onchange: move |e| position.set(e.value()),
                        for p in MarkPosition::ALL { option { value: "{p.label()}", selected: p == MarkPosition::Center, "{p.label()}" } }
                    }
                }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
fn HeaderFooter() -> Element {
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
            div { class: "field-grid", Num { label: t(lang, "dialog.watermark.size"), value: size, min: "5", max: "72" } Num { label: "Margin (pt)".to_string(), value: margin, min: "0", max: "200" } }
            PageScope { mode, range }
        }
    }
}

#[component]
fn PageNumbers(bates: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let format = use_signal(|| {
        if bates {
            "DOC-{bates:6}".to_string()
        } else {
            "{page}".to_string()
        }
    });
    let start = use_signal(|| "1".to_string());
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
                    apply(st, |tab| tab.doc.add_page_numbers(&pages, &fmt, s, pos, fs, 28.0, [0.0, 0.0, 0.0]));
                }
                Err(e) => st.error(e),
            },
            Text { label: t(lang, "dialog.numbers.format"), value: format, autofocus: true }
            div { class: "hint", "{t(lang, \"dialog.hf.hint\")}" }
            div { class: "field-grid three",
                Num { label: t(lang, "dialog.numbers.start"), value: start, min: "0" }
                Num { label: t(lang, "dialog.watermark.size"), value: size, min: "5", max: "72" }
                div { class: "field", label { "{t(lang, \"dialog.watermark.position\")}" }
                    select { onchange: move |e| position.set(e.value()),
                        for p in MarkPosition::ALL.iter().filter(|p| **p != MarkPosition::Center) { option { value: "{p.label()}", selected: p.label() == position(), "{p.label()}" } }
                    }
                }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
fn RemoveMarks() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let mut kind = use_signal(|| "all".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.removemarks.title"), danger: true,
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => { let k = kind(); let tag = if k == "all" { None } else { Some(k) }; apply(st, |tab| tab.doc.remove_marks(&pages, tag.as_deref()).map(|_| ())); }
                Err(e) => st.error(e),
            },
            div { class: "field", label { "Kind" }
                select { onchange: move |e| kind.set(e.value()),
                    option { value: "all", "All marks" } option { value: "Watermark", "Watermarks" } option { value: "HeaderFooter", "Headers and footers" } option { value: "PageNumber", "Page / Bates numbers" }
                }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
fn ExportImages() -> Element {
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
                    select { onchange: move |e| format.set(e.value()), option { value: "png", "PNG" } option { value: "jpg", "JPEG" } option { value: "txt", "Text (.txt)" } }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: images_only(), onchange: move |_| images_only.set(!images_only()) } span { "Extract embedded images instead of rendering pages" } }
            PageScope { mode, range }
        }
    }
}

#[component]
fn Split() -> Element {
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
fn MergeFile(path: String) -> Element {
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
            div { class: "kv", span { class: "k", "File" } span { class: "v", "{path}" } }
            div { class: "field", label { "{t(lang, \"dialog.insert.position\")}" }
                select { onchange: move |e| position.set(e.value()),
                    option { value: "start", "At the beginning" }
                    option { value: "before", "{t(lang, \"dialog.insert.before\")}" }
                    option { value: "after", selected: true, "{t(lang, \"dialog.insert.after\")}" }
                    option { value: "end", "{t(lang, \"dialog.insert.end\")}" }
                }
            }
        }
    }
}

#[component]
fn NotImplemented(id: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let (label, path) = reg
        .get(&id)
        .map(|c| (cmd_label(lang, c), reg.path_label(&c.id)))
        .unwrap_or((id.clone(), String::new()));
    let body = tf(
        lang,
        "dialog.notimpl.body",
        &[("label", &label), ("version", folitecta_core::VERSION)],
    );
    rsx! {
        Frame { title: t(lang, "dialog.notimpl.title"),
            div { class: "callout warn", "{body}" }
            div { class: "kv", span { class: "k", "Command" } span { class: "v mono", "{id}" } span { class: "k", "Location" } span { class: "v", "{path}" } }
            button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::Matrix), "{t(lang, \"dialog.matrix.title\")}" }
        }
    }
}

#[component]
fn Confirm(title: String, body: String, action: ConfirmAction) -> Element {
    let st = use_context::<AppState>();
    let act = action.clone();
    rsx! {
        Frame { title, danger: true,
            on_ok: move |_| {
                match act.clone() {
                    ConfirmAction::DeleteAllComments => apply(st, |tab| tab.doc.delete_all_comments(None).map(|_| ())),
                    ConfirmAction::RemoveLinks => apply(st, |tab| tab.doc.remove_links(None).map(|_| ())),
                    ConfirmAction::ResetForm => apply(st, |tab| tab.doc.reset_form().map(|_| ())),
                    ConfirmAction::Revert => { st.set_dialog(Dialog::None); files::revert(st); }
                }
            },
            p { "{body}" }
        }
    }
}

#[component]
fn Unsaved(tab: u64, then: crate::state::AfterUnsaved) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let name = st
        .docs
        .read()
        .iter()
        .find(|t| t.id == tab)
        .map(|t| t.title())
        .unwrap_or_default();
    let then_save = then.clone();
    let then_discard = then.clone();
    rsx! {
        div { class: "backdrop",
            div { class: "dialog", onclick: move |e| e.stop_propagation(),
                div { class: "dialog-head", "{t(lang, \"dialog.unsaved.title\")}" }
                div { class: "dialog-body", p { "{tf(lang, \"dialog.unsaved.body\", &[(\"name\", &name)])}" } }
                div { class: "dialog-foot",
                    button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::None), "{t(lang, \"dialog.cancel\")}" }
                    button { class: "tbtn danger", onclick: move |_| { st.set_dialog(Dialog::None); files::resolve_unsaved(st, tab, then_discard.clone(), false); }, "{t(lang, \"dialog.unsaved.discard\")}" }
                    button { class: "tbtn primary", onclick: move |_| { st.set_dialog(Dialog::None); files::resolve_unsaved(st, tab, then_save.clone(), true); }, "{t(lang, \"dialog.unsaved.save\")}" }
                }
            }
        }
    }
}

#[component]
fn Password(path: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let pw = use_signal(String::new);
    let p = std::path::PathBuf::from(path.clone());
    rsx! {
        Frame { title: t(lang, "dialog.password.title"),
            on_ok: move |_| { let password = pw(); st.set_dialog(Dialog::None); files::open_path(st, p.clone(), Some(&password)); },
            p { "{t(lang, \"dialog.password.body\")}" }
            div { class: "kv", span { class: "k", "File" } span { class: "v", "{path}" } }
            div { class: "field", label { "Password" } input { r#type: "password", autofocus: true, value: "{pw}", oninput: move |e| { let mut s = pw; s.set(e.value()); } } }
        }
    }
}

#[component]
fn NoteDialog(page: usize, x: f32, y: f32) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let author = use_signal(|| st.ui.read().author.clone());
    rsx! {
        Frame { title: t(lang, "dialog.note.title"),
            on_ok: move |_| { let (c, a) = (text(), author()); apply(st, |tab| tab.doc.add_note(page, x, y, &c, &a, [1.0, 0.85, 0.2]).map(|_| ())); st.set_tool(crate::state::Tool::Hand); },
            Area { label: t(lang, "dialog.note.title"), value: text, autofocus: true }
            Text { label: t(lang, "dialog.note.author"), value: author }
        }
    }
}

#[component]
fn TextBoxDialog(page: usize, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let size = use_signal(|| "12".to_string());
    let mut color = use_signal(|| "#1d4ed8".to_string());
    let mut border = use_signal(|| true);
    let r = [rect[0], rect[1], rect[2], rect[3]];
    rsx! {
        Frame { title: t(lang, "tool.text"),
            on_ok: move |_| { let (c, fs, col, b) = (text(), size().trim().parse().unwrap_or(12.0), parse_color(&color()), border()); let author = st.ui.read().author.clone(); apply(st, |tab| tab.doc.add_free_text(page, r, &c, &author, fs, col, b).map(|_| ())); st.set_tool(crate::state::Tool::Hand); },
            Area { label: t(lang, "dialog.note.title"), value: text, autofocus: true }
            div { class: "field-grid",
                Num { label: t(lang, "dialog.watermark.size"), value: size, min: "4", max: "120" }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
            }
            label { class: "check", input { r#type: "checkbox", checked: border(), onchange: move |_| border.set(!border()) } span { "Border" } }
        }
    }
}

#[component]
fn LinkDialog(page: usize, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let url = use_signal(|| "https://".to_string());
    let r = [rect[0], rect[1], rect[2], rect[3]];
    rsx! {
        Frame { title: t(lang, "dialog.link.title"),
            on_ok: move |_| { let u = url(); apply(st, |tab| tab.doc.add_uri_link(page, r, &u).map(|_| ())); st.set_tool(crate::state::Tool::Hand); },
            Text { label: t(lang, "dialog.link.url"), value: url, autofocus: true }
        }
    }
}

#[component]
fn FieldValue(id_num: u32, id_gen: u16, name: String, value: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let v = use_signal(|| value.clone());
    rsx! {
        Frame { title: format!("{} — {}", t(lang, "dialog.field.title"), name),
            on_ok: move |_| { let val = v(); apply(st, |tab| tab.doc.set_field_value((id_num, id_gen), &val)); },
            Text { label: name.clone(), value: v, autofocus: true }
        }
    }
}

#[component]
fn BookmarkNew() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let page = st.current_page();
    let title = use_signal(|| format!("Page {}", page + 1));
    let page_s = use_signal(|| (page + 1).to_string());
    rsx! {
        Frame { title: t(lang, "dialog.bookmark.title"),
            on_ok: move |_| { let ti = title(); let p: usize = page_s().trim().parse::<usize>().unwrap_or(page + 1).saturating_sub(1); apply(st, |tab| tab.doc.add_bookmark(&ti, p, &[])); },
            Text { label: t(lang, "dialog.bookmark.name"), value: title, autofocus: true }
            Num { label: t(lang, "status.page"), value: page_s, min: "1" }
        }
    }
}

#[component]
fn BookmarkRename(path: Vec<usize>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let current = st
        .read_active(|t| {
            let flat = t.doc.outline();
            fn find(items: &[folitecta_core::OutlineItem], p: &[usize]) -> Option<String> {
                let (f, r) = p.split_first()?;
                let it = items.get(*f)?;
                if r.is_empty() {
                    Some(it.title.clone())
                } else {
                    find(&it.children, r)
                }
            }
            find(&flat, &path)
        })
        .flatten()
        .unwrap_or_default();
    let title = use_signal(|| current);
    let p = path.clone();
    rsx! {
        Frame { title: t(lang, "cmd.rename"),
            on_ok: move |_| { let ti = title(); let pp = p.clone(); apply(st, |tab| tab.doc.rename_bookmark(&pp, &ti)); },
            Text { label: t(lang, "dialog.bookmark.name"), value: title, autofocus: true }
        }
    }
}

#[component]
fn AnnotationText(page: usize, index: usize, contents: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(|| contents.clone());
    rsx! {
        Frame { title: t(lang, "dialog.note.title"),
            on_ok: move |_| { let c = text(); apply(st, |tab| tab.doc.set_annotation_contents(page, index, &c)); },
            Area { label: t(lang, "dialog.note.title"), value: text, autofocus: true }
        }
    }
}

#[component]
fn WordCount(words: usize, chars: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    rsx! { Frame { title: t(lang, "cmd.document.wordCount"), div { class: "kv", span { class: "k", "Words" } span { class: "v", "{words}" } span { class: "k", "Characters" } span { class: "v", "{chars}" } } } }
}

#[component]
fn UrlReport(urls: Vec<(usize, String)>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    rsx! {
        Frame { title: t(lang, "cmd.createWebLinksFromText"),
            div { class: "callout", "Addresses found in the page text. Creating clickable link rectangles automatically needs glyph positions, which is planned; use the Web link tool to draw a link over the text meanwhile." }
            if urls.is_empty() { p { "{t(lang, \"toast.not_found\")}" } }
            div { class: "matrix", table { tbody { for (p, u) in urls { tr { key: "{p}-{u}", td { "{p + 1}" } td { class: "mono", style: "user-select:text", "{u}" } } } } } }
        }
    }
}

#[component]
fn Summary(text: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let t2 = text.clone();
    rsx! {
        Frame { title: t(lang, "cmd.comments.summarize"), ok_label: Some(t(lang, "dialog.unsaved.save")),
            on_ok: move |_| { st.set_dialog(Dialog::None); files::save_text_file(st, "comments.txt", t2.clone()); },
            pre { class: "mono", style: "white-space: pre-wrap; user-select: text; max-height: 50vh; overflow: auto", "{text}" }
            if false { span { "{color_hex([0.0,0.0,0.0])}" } }
            if false { span { "{ToastKind::Info == ToastKind::Info}" } }
        }
    }
}
