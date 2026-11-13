//! Application dialogs: toolbars, zoom, about, shortcuts, feature matrix, preferences, command line, confirmations, unsaved changes, password, and the sheets laid out for the system print dialog.

use super::*;

#[component]
pub(super) fn ToolbarsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let visible = crate::ui::chrome::toolbars::visible_toolbars(st);
    let menu_on = crate::ui::chrome::toolbars::menubar_visible(st);
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
                for (id, cmds) in crate::ui::chrome::toolbars::TOOLBARS.iter().copied() {
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

/// The sheets laid out as pictures for the system print dialog.
#[component]
pub fn PrintView() -> Element {
    let st = use_context::<AppState>();
    let Some(job) = (st.print_job)() else {
        return rsx! {};
    };
    let page_style = format!(
        "@media print {{ @page {{ size: {:.2}pt {:.2}pt; margin: 0; }} }}",
        job.paper.0, job.paper.1
    );
    let sheets: Vec<(usize, usize)> = job
        .sheets
        .iter()
        .enumerate()
        .flat_map(|(d, n)| (0..*n).map(move |i| (d, i)))
        .collect();
    rsx! {
        div { class: "print-view fit", key: "{job.generation}",
            style { "{page_style}" }
            for (d, i) in sheets {
                div { class: "print-page", key: "{d}-{i}",
                    img { src: "/sheet/s/{job.generation}/{d}/{i}/{job.scale_milli}.png", alt: "" }
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
    let settings = crate::state::app_data_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    let portable = crate::portable::active();
    rsx! {
        Frame { title: t(lang, "dialog.about.title"),
            div { class: "about",
                img { src: "{icon}", alt: "" }
                div {
                    h2 { "Folitecta" }
                    div { class: "ver", "Version {folitecta_core::VERSION}" }
                    p { "{t(lang, \"dialog.about.body\")}" }
                    p { "{t(lang, \"dialog.about.license\")}" }
                    p { class: "hint about-settings",
                        {crate::i18n::tf(lang, "dialog.about.settings", &[("path", &settings)])}
                        if portable { " {t(lang, \"dialog.about.portable\")}" }
                    }
                    ul {
                        li { "Rendering: hayro (Apache-2.0 OR MIT)" }
                        li { "Document model: lopdf (MIT)" }
                        li { "User interface: Dioxus (MIT OR Apache-2.0)" }
                        li { "Text beyond Western European characters, and fonts embedded for PDF/A: GNU FreeFont FreeSans, FreeSerif and FreeMono (GPL-3.0-or-later with the font exception)" }
                        li { a { href: "https://github.com/KvasFan/Folitecta", onclick: move |e| { e.prevent_default(); let _ = crate::util::open_external("https://github.com/KvasFan/Folitecta"); }, "github.com/KvasFan/Folitecta" } }
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
    let sites = crate::commands::navigation::follow::allowed_sites(st);
    rsx! {
        Frame { title: t(lang, "dialog.prefs.title"), on_ok: move |_| { st.ui_mut().author = author(); st.save_prefs(); st.set_dialog(Dialog::None); },
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.prefs.theme\")}" }
                    select { onchange: move |e| { st.ui_mut().theme = Theme::from_attr(&e.value()); st.save_prefs(); },
                        for th in Theme::ALL {
                            {
                                let name = t(lang, &format!("dialog.prefs.theme.{}", th.attr()));
                                rsx! { option { key: "{th.attr()}", value: "{th.attr()}", selected: prefs.theme == th, "{name}" } }
                            }
                        }
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
            label { class: "check",
                input { r#type: "checkbox", checked: crate::commands::option_on(st, "view.ribbonLabels"),
                    onchange: move |_| crate::commands::dispatch(st, "view.ribbonLabels") }
                span { "{t(lang, \"dialog.prefs.labels\")}" }
            }
            if !sites.is_empty() {
                div { class: "row prefs-sites", style: "gap:8px;align-items:center;flex-wrap:wrap",
                    span { class: "hint", {tf(lang, "dialog.prefs.sites", &[("list", &sites.join(", "))])} }
                    button { class: "tbtn prefs-forget-sites", onclick: move |_| crate::commands::navigation::follow::forget_sites(st), "{t(lang, \"dialog.prefs.sites_forget\")}" }
                }
            }
            label { class: "check", title: "{t(lang, \"dialog.prefs.page_labels.hint\")}",
                input { r#type: "checkbox", class: "prefs-page-labels", checked: crate::commands::option_on(st, "pages.useLabels"),
                    onchange: move |e| st.set_option("pages.useLabels", e.checked()) }
                span { "{t(lang, \"dialog.prefs.page_labels\")}" }
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
                "folitecta [FILE.pdf ...]           open files\nfolitecta --settings DIR [FILE ...] keep the settings in DIR\nfolitecta --run-actions SEQ.json [--output DIR] [--suffix TEXT] [--overwrite] FILES...\n                                   run an action sequence on files and folders\nfolitecta --features-md OUT.md     write the feature matrix\nfolitecta --version\nfolitecta --help"
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
    // Importing the comments that can be drawn is a choice, not a loss.
    let choice = matches!(action, ConfirmAction::ImportCommentsSkipping(_));
    let ok_label = choice.then(|| t(st.lang(), "dialog.import_undrawable.ok"));
    rsx! {
        Frame { title, danger: !choice, ok_label,
            on_ok: move |_| {
                match act.clone() {
                    ConfirmAction::DeleteAllComments => apply(st, |tab| tab.doc.delete_all_comments(None).map(|_| ())),
                    ConfirmAction::RemoveLinks => apply(st, |tab| tab.doc.remove_links(None).map(|_| ())),
                    ConfirmAction::ResetForm => apply(st, |tab| tab.doc.reset_form().map(|_| ())),
                    ConfirmAction::Revert => { st.set_dialog(Dialog::None); files::revert(st); }
                    ConfirmAction::RemoveScripts => apply_count(st, "toast.scripts_removed", |tab| tab.doc.remove_scripts()),
                    ConfirmAction::ClearDocumentActions => apply_count(st, "toast.document_actions_cleared", |tab| tab.doc.clear_document_actions()),
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
                    ConfirmAction::RemoveCropped(pages) => {
                        st.set_dialog(Dialog::None);
                        crate::commands::doctools::remove_cropped_now(st, &pages);
                    }
                    ConfirmAction::DeletePages(pages) => {
                        st.with_active(|t| t.selected_pages.clear());
                        apply(st, move |tab| tab.doc.delete_pages(&pages));
                    }
                    ConfirmAction::ImportCommentsSkipping(data) => files::import_drawable_comments(st, &data),
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
pub(super) fn Password(target: PasswordTarget, retry: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let pw = use_signal(String::new);
    let (body, label, value) = match &target {
        PasswordTarget::File(path) => ("dialog.password.body", "dialog.file", path.display().to_string()),
        PasswordTarget::Replace { path, .. } => (
            "dialog.password.body_revert",
            "dialog.file",
            path.display().to_string(),
        ),
        PasswordTarget::Revision { field, .. } => {
            ("dialog.password.body_revision", "sig.field", field.clone())
        }
        PasswordTarget::Owner { tab, .. } => (
            "dialog.password.body_owner",
            "dialog.file",
            st.docs
                .peek()
                .iter()
                .find(|d| d.id == *tab)
                .map(|d| d.title())
                .unwrap_or_default(),
        ),
    };
    let text = match &target {
        PasswordTarget::Owner { refused, .. } => tf(
            lang,
            body,
            &[(
                "what",
                &t(lang, &format!("dialog.password.restricted.{}", refused.id())),
            )],
        ),
        _ => t(lang, body),
    };
    let title = if matches!(target, PasswordTarget::Owner { .. }) {
        t(lang, "dialog.password.title_owner")
    } else {
        t(lang, "dialog.password.title")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let password = pw();
                st.set_dialog(Dialog::None);
                match target.clone() {
                    PasswordTarget::File(path) => files::open_path(st, path, Some(&password)),
                    PasswordTarget::Replace { path, tab } => files::open_in_place_of(st, tab, path, Some(&password)),
                    PasswordTarget::Revision { tab, field } => {
                        crate::commands::signatures::open_signed_revision_with(st, tab, &field, Some(&password))
                    }
                    PasswordTarget::Owner { tab, refused } => {
                        let unlocked = st
                            .docs_mut()
                            .iter_mut()
                            .find(|d| d.id == tab)
                            .is_some_and(|d| d.doc.unlock_owner(&password));
                        if unlocked {
                            st.toast(crate::state::ToastKind::Ok, t(lang, "dialog.password.owner_ok"), "");
                        } else {
                            st.set_dialog(Dialog::Password { target: PasswordTarget::Owner { tab, refused }, retry: true });
                        }
                    }
                }
            },
            p { class: "password-body", "{text}" }
            if retry {
                p { class: "hint warn password-wrong", "{t(lang, \"dialog.password.wrong\")}" }
            }
            div { class: "kv", span { class: "k", "{t(lang, label)}" } span { class: "v", "{value}" } }
            div { class: "field", label { "{t(lang, \"dialog.password.field\")}" } input { class: "password-input", r#type: "password", autofocus: true, value: "{pw}", oninput: move |e| { let mut s = pw; s.set(e.value()); } } }
        }
    }
}

/// A link or button that reaches outside the document, named before it opens.
#[component]
pub(super) fn OpenTarget(tab: u64, target: folitecta_core::LinkTarget) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut trust = use_signal(|| false);
    let mut always = use_signal(|| false);
    let site = match &target {
        folitecta_core::LinkTarget::Uri(u) => crate::commands::navigation::follow::web_site(u),
        _ => None,
    };
    let site_ok = site.clone();
    let (body, what) = match &target {
        folitecta_core::LinkTarget::Uri(u) => ("dialog.open_target.web", u.clone()),
        folitecta_core::LinkTarget::Launch(f) => ("dialog.open_target.file", f.clone()),
        folitecta_core::LinkTarget::Remote(f) => ("dialog.open_target.document", f.clone()),
        other => ("dialog.open_target.file", other.describe()),
    };
    rsx! {
        Frame { title: t(lang, "dialog.open_target.title"), ok_label: t(lang, "dialog.open_target.open"),
            on_ok: move |_| {
                st.set_dialog(Dialog::None);
                if trust() {
                    if let Some(d) = st.docs_mut().iter_mut().find(|d| d.id == tab) {
                        d.trust_links = true;
                    }
                }
                if let (true, Some(s)) = (always(), &site_ok) {
                    crate::commands::navigation::follow::allow_site(st, s);
                }
                crate::commands::navigation::follow::open(st, &target);
            },
            p { "{t(lang, body)}" }
            div { class: "kv", span { class: "v open-target", "{what}" } }
            label { class: "check", input { class: "open-target-trust", r#type: "checkbox", checked: trust(), onchange: move |_| trust.set(!trust()) } span { "{t(lang, \"dialog.open_target.trust\")}" } }
            if let Some(s) = site {
                label { class: "check", input { class: "open-target-always", r#type: "checkbox", checked: always(), onchange: move |_| always.set(!always()) }
                    span { {tf(lang, "dialog.open_target.always", &[("site", &s)])} } }
            }
        }
    }
}

#[component]
pub(super) fn WordCount(words: usize, chars: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    rsx! { Frame { title: t(lang, "cmd.document.wordCount"), div { class: "kv", span { class: "k", "{t(lang, \"dialog.wordcount.words\")}" } span { class: "v", "{words}" } span { class: "k", "{t(lang, \"dialog.wordcount.chars\")}" } span { class: "v", "{chars}" } } } }
}

/// The title of the article drawn with the Article tool.
#[component]
pub(super) fn ArticleTitle() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let title = use_signal(String::new);
    let parts = crate::commands::articles::DRAFT.read().len();
    rsx! {
        Frame { title: t(lang, "dialog.article.title"),
            on_ok: move |_| crate::commands::articles::make(st, &title()),
            p { class: "hint", "{tf(lang, \"dialog.article.parts\", &[(\"count\", &parts.to_string())])}" }
            Text { label: t(lang, "dialog.article.name"), value: title, autofocus: true }
        }
    }
}

/// A new title for an article of the Articles pane.
#[component]
pub(super) fn ArticleRename(index: usize, title: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let name = use_signal(|| title.clone());
    rsx! {
        Frame { title: t(lang, "dialog.article.rename"),
            on_ok: move |_| {
                match st.with_active(|t| t.doc.rename_article(index, &name())) {
                    Some(Ok(())) => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                    Some(Err(e)) => st.error(e.to_string()),
                    None => {}
                }
            },
            Text { label: t(lang, "dialog.article.name"), value: name, autofocus: true }
        }
    }
}

/// The commands of a group command (Other Tools, Clipboard, Panes…): a click
/// runs one and closes the window.
#[component]
pub(super) fn CommandGroupDialog(id: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let title = reg.get(&id).map(|c| cmd_label(lang, c)).unwrap_or_default();
    let ids = crate::commands::groups::group_commands(&id);
    rsx! {
        Frame { title,
            div { class: "command-group",
                if ids.is_empty() { p { class: "muted", "{t(lang, \"dialog.group.none\")}" } }
                for cid in ids.into_iter() {
                    {
                        let label = reg.get(cid).map(|c| cmd_label(lang, c)).unwrap_or_default();
                        let path = reg.path_label(cid);
                        let shortcut = reg.get(cid).and_then(|c| c.shortcut.clone()).unwrap_or_default();
                        rsx! {
                            button { class: "command-group-row", key: "{cid}", "data-cmd": "{cid}", title: "{path}",
                                onclick: move |_| { st.set_dialog(Dialog::None); crate::commands::dispatch(st, cid); },
                                Icon { name: crate::icons::for_command(cid) }
                                span { class: "grow", "{label}" }
                                if !shortcut.is_empty() { span { class: "muted", "{shortcut}" } }
                                {crate::ui::status_badge(cid)}
                            }
                        }
                    }
                }
            }
        }
    }
}
