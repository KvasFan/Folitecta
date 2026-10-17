//! Application dialogs: toolbars, zoom, about, shortcuts, feature matrix, preferences, command line, confirmations, unsaved changes, password and printing.

use super::*;

#[component]
pub(super) fn ToolbarsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let visible = crate::ui::toolbars::visible_toolbars(st);
    let menu_on = crate::ui::toolbars::menubar_visible(st);
    let ribbon = st.ui.read().ribbon;
    rsx! {
        Frame { title: t(lang, "cmd.view.toolbars"),
            if ribbon {
                div { class: "hint", "{t(lang, \"dialog.toolbars.classic_only\")}" }
            }
            div { class: "check-list",
                label { class: "check",
                    input { r#type: "checkbox", checked: menu_on, onchange: move |_| crate::commands::dispatch(st, "view.toolbar.menu") }
                    span { "{reg.get(\"view.toolbar.menu\").map(|c| cmd_label(lang, c)).unwrap_or_default()}" }
                }
                for (id, cmds) in crate::ui::toolbars::TOOLBARS.iter().copied() {
                    {
                        let on = visible.iter().any(|v| v == id);
                        let cmd_id = format!("view.toolbar.{id}");
                        let label = reg.get(&cmd_id).map(|c| cmd_label(lang, c)).unwrap_or_else(|| id.to_string());
                        let n = cmds.len();
                        rsx! {
                            label { class: "check", key: "{id}",
                                input { r#type: "checkbox", checked: on, onchange: move |_| crate::commands::dispatch(st, &cmd_id) }
                                span { "{label}" }
                                span { class: "muted small", "{n}" }
                            }
                        }
                    }
                }
            }
            div { class: "row",
                button { class: "tbtn", onclick: move |_| crate::commands::dispatch(st, "view.restoreCmdBarsVisibility"), "{t(lang, \"cmd.view.restoreCmdBarsVisibility\")}" }
                if ribbon {
                    button { class: "tbtn primary", onclick: move |_| { st.ui_mut().ribbon = false; st.save_prefs(); }, "{t(lang, \"dialog.toolbars.switch\")}" }
                }
            }
        }
    }
}

#[component]
pub(super) fn ZoomLevelDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let current = (crate::commands::current_zoom(st) * 100.0).round();
    let value = use_signal(|| current.to_string());
    let set = move |z: Zoom| {
        st.with_active(|t| t.zoom = z);
        st.set_dialog(Dialog::None);
    };
    rsx! {
        Frame { title: t(lang, "cmd.zoom.combo"),
            on_ok: move |_| {
                let v: f32 = value().replace(',', ".").trim_end_matches('%').trim().parse().unwrap_or(current);
                set(Zoom::Fixed((v / 100.0).clamp(0.1, 8.0)));
            },
            Num { label: "%".to_string(), value, min: "10", max: "800", step: "10" }
            div { class: "zoom-presets",
                button { class: "tbtn", onclick: move |_| set(Zoom::FitWidth), "{t(lang, \"cmd.zoom.level.fitWidth\")}" }
                button { class: "tbtn", onclick: move |_| set(Zoom::FitPage), "{t(lang, \"cmd.zoom.level.fitPage\")}" }
                for z in [50u32, 75, 100, 125, 150, 200, 300, 400] {
                    button { class: "tbtn", key: "{z}", onclick: move |_| set(Zoom::Fixed(z as f32 / 100.0)), "{z}%" }
                }
            }
        }
    }
}

#[component]
pub(super) fn PrintDialog(all_docs: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| default_scope(st));
    let range = use_signal(String::new);
    let mut dpi = use_signal(|| "150".to_string());
    let mut annots = use_signal(|| true);
    let mut fit = use_signal(|| true);
    let docs = st.docs.read().len();
    let total_pages: usize = st.docs.read().iter().map(|t| t.doc.page_count()).sum();
    rsx! {
        Frame { title: if all_docs { t(lang, "dialog.print.title_all") } else { t(lang, "dialog.print.title") }, ok_label: Some(t(lang, "dialog.print.go")),
            on_ok: move |_| {
                let pages: Vec<crate::state::PrintPage> = if all_docs {
                    st.docs.read().iter().flat_map(|t| (0..t.doc.page_count()).map(move |p| (t.id, p, t.revision))).collect()
                } else {
                    match scope_pages(st, mode(), &range()) {
                        Ok(pages) => st.read_active(|t| pages.iter().map(|p| (t.id, *p, t.revision)).collect()).unwrap_or_default(),
                        Err(e) => { st.error(e); return; }
                    }
                };
                let d: f32 = dpi().parse().unwrap_or(150.0);
                st.set_dialog(Dialog::None);
                files::start_print(st, pages, d, annots(), fit());
            },
            if all_docs {
                div { class: "hint", "{tf(lang, \"dialog.print.all_hint\", &[(\"docs\", &docs.to_string()), (\"pages\", &total_pages.to_string())])}" }
            } else {
                PageScope { mode, range }
            }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.export.dpi\")}" }
                    select { onchange: move |e| dpi.set(e.value()),
                        option { value: "100", "100 dpi" }
                        option { value: "150", selected: true, "150 dpi" }
                        option { value: "300", "300 dpi" }
                    }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: annots(), onchange: move |_| annots.set(!annots()) } span { "{t(lang, \"dialog.rasterize.annots\")}" } }
            label { class: "check", input { r#type: "checkbox", checked: fit(), onchange: move |_| fit.set(!fit()) } span { "{t(lang, \"dialog.print.fit\")}" } }
            div { class: "hint", "{t(lang, \"dialog.print.hint\")}" }
        }
    }
}

/// Hidden page images laid out for the system print dialog.
#[component]
pub fn PrintView() -> Element {
    let st = use_context::<AppState>();
    let Some(job) = (st.print_job)() else {
        return rsx! {};
    };
    let q = if job.annotations { "" } else { "?annots=0" };
    let class = if job.fit {
        "print-view fit"
    } else {
        "print-view actual"
    };
    rsx! {
        div { class: "{class}", key: "{job.id}",
            for (i, (tab, page, rev)) in job.pages.iter().enumerate() {
                div { class: "print-page", key: "{i}",
                    img { src: "/page/{tab}/{page}/{rev}/{job.scale_milli}.png{q}", alt: "" }
                }
            }
        }
    }
}

#[component]
pub(super) fn About() -> Element {
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
pub(super) fn Shortcuts() -> Element {
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
            div { class: "matrix", table { thead { tr { th { "{t(lang, \"dialog.shortcuts.key\")}" } th { "{t(lang, \"dialog.shortcuts.command\")}" } } } tbody { for (k, v) in rows { tr { key: "{k}", td { class: "mono", "{k}" } td { "{v}" } } } } } }
        }
    }
}

/// Localized status badge for the feature matrix.
pub(super) fn matrix_badge(lang: Lang, id: &str) -> Element {
    if is_partial(id) {
        rsx! { span { class: "badge partial", "{t(lang, \"label.partial\").to_lowercase()}" } }
    } else if is_implemented(id) {
        rsx! { span { class: "badge ok", "{t(lang, \"label.available\")}" } }
    } else {
        rsx! { span { class: "badge planned", "{t(lang, \"label.planned\").to_lowercase()}" } }
    }
}

#[component]
pub(super) fn Matrix() -> Element {
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
                    thead { tr { th { "{t(lang, \"dialog.matrix.location\")}" } th { "{t(lang, \"dialog.shortcuts.command\")}" } th { "{t(lang, \"dialog.matrix.status\")}" } } }
                    tbody {
                        for (tab, tab_label) in RIBBON_TABS.iter() {
                            for (_, g, g_label) in RIBBON_GROUPS.iter().filter(|(t, _, _)| t == tab) {
                                for c in reg.ribbon_group(tab, g) {
                                    tr { key: "{c.id}",
                                        td { "{tab_label} › {g_label}" }
                                        td { "{c.label}" }
                                        td { {matrix_badge(lang, &c.id)} }
                                    }
                                }
                            }
                        }
                        for (menu, menu_label) in MENUS.iter() {
                            for c in reg.menu_entries(menu).into_iter().flatten() {
                                tr { key: "m-{c.id}",
                                    td { "{menu_label}" }
                                    td { "{c.label}" }
                                    td { {matrix_badge(lang, &c.id)} }
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
pub(super) fn Prefs() -> Element {
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
pub(super) fn CommandLine() -> Element {
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
pub(super) fn NotImplemented(id: String) -> Element {
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
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.notimpl.command\")}" } span { class: "v mono", "{id}" } span { class: "k", "{t(lang, \"dialog.notimpl.location\")}" } span { class: "v", "{path}" } }
            button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::Matrix), "{t(lang, \"dialog.matrix.title\")}" }
        }
    }
}

#[component]
pub(super) fn Confirm(title: String, body: String, action: ConfirmAction) -> Element {
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
                    ConfirmAction::ApplyRedactions(page) => {
                        let pages = page.map(|p| vec![p]);
                        apply_count(st, "toast.redactions_applied", |tab| tab.doc.apply_redactions(pages.as_deref()));
                    }
                    ConfirmAction::RemoveScripts => apply_count(st, "toast.scripts_removed", |tab| tab.doc.remove_scripts()),
                    ConfirmAction::ClearSignatures => {
                        apply_count(st, "toast.signatures_cleared", |tab| tab.doc.clear_signatures());
                    }
                    ConfirmAction::AutoTag => {
                        st.set_dialog(Dialog::None);
                        crate::commands::accessibility::run_auto_tag(st);
                    }
                    ConfirmAction::TrashFiles => {
                        st.set_dialog(Dialog::None);
                        crate::commands::explorer::trash_selected(st);
                    }
                    ConfirmAction::TrashDocument => {
                        st.set_dialog(Dialog::None);
                        crate::commands::fileops::trash_now(st);
                    }
                    ConfirmAction::ClearStructure => {
                        st.with_active(|t| { t.acc.tag = None; t.flash = None; });
                        apply(st, |tab| tab.doc.clear_structure());
                    }
                    ConfirmAction::DeletePages(pages) => {
                        st.with_active(|t| t.selected_pages.clear());
                        apply(st, move |tab| tab.doc.delete_pages(&pages));
                    }
                }
            },
            p { "{body}" }
        }
    }
}

#[component]
pub(super) fn Unsaved(tab: u64, then: crate::state::AfterUnsaved) -> Element {
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
pub(super) fn Password(path: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let pw = use_signal(String::new);
    let p = std::path::PathBuf::from(path.clone());
    rsx! {
        Frame { title: t(lang, "dialog.password.title"),
            on_ok: move |_| { let password = pw(); st.set_dialog(Dialog::None); files::open_path(st, p.clone(), Some(&password)); },
            p { "{t(lang, \"dialog.password.body\")}" }
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.file\")}" } span { class: "v", "{path}" } }
            div { class: "field", label { "{t(lang, \"dialog.password.field\")}" } input { r#type: "password", autofocus: true, value: "{pw}", oninput: move |e| { let mut s = pw; s.set(e.value()); } } }
        }
    }
}

#[component]
pub(super) fn WordCount(words: usize, chars: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    rsx! { Frame { title: t(lang, "cmd.document.wordCount"), div { class: "kv", span { class: "k", "{t(lang, \"dialog.wordcount.words\")}" } span { class: "v", "{words}" } span { class: "k", "{t(lang, \"dialog.wordcount.chars\")}" } span { class: "v", "{chars}" } } } }
}
