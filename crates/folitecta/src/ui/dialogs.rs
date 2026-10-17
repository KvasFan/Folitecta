//! Modal dialogs.

use std::rc::Rc;

use dioxus::prelude::*;
use folitecta_core::commands::{MENUS, RIBBON_GROUPS, RIBBON_TABS};
use folitecta_core::marks::{MarkPosition, TextMarkOptions};
use folitecta_core::{
    Document, FlattenScope, ImageFormat, STAMP_PRESETS, SecurityPermissions, TRANSITIONS, registry,
};

use crate::app::icon_data_url;
use crate::commands::{implemented_ids, is_implemented, is_partial, option_on, partial_ids};
use crate::files;
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::icons::Icon;
use crate::state::{AppState, ConfirmAction, Dialog, Guide, Theme, ToastKind, Tool, Zoom};
use crate::util::{format_pages, parse_color, parse_page_range};

#[component]
pub fn Frame(
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
        Dialog::TextBox { page, rect, border } => {
            rsx! { TextBoxDialog { page, rect: rect.to_vec(), border } }
        }
        Dialog::Link { page, rect } => rsx! { LinkDialog { page, rect: rect.to_vec() } },
        Dialog::FieldValue { id, name, value } => {
            rsx! { FieldValue { id_num: id.0, id_gen: id.1, name, value } }
        }
        Dialog::BookmarkRename(path) => rsx! { BookmarkRename { path } },
        Dialog::AnnotationText {
            page,
            index,
            contents,
        } => rsx! { AnnotationText { page, index, contents } },
        Dialog::WordCount { words, chars } => rsx! { WordCount { words, chars } },
        Dialog::NewBlank => rsx! { NewBlank {} },
        Dialog::PasteSpecial => rsx! { PasteSpecial {} },
        Dialog::Rasterize => rsx! { Rasterize {} },
        Dialog::StampPicker => rsx! { StampPicker {} },
        Dialog::Security => rsx! { Security {} },
        Dialog::Flatten => rsx! { Flatten {} },
        Dialog::AddGuides => rsx! { AddGuides {} },
        Dialog::SplitTiles => rsx! { SplitTiles {} },
        Dialog::Background => rsx! { Background {} },
        Dialog::PageLabels => rsx! { PageLabels {} },
        Dialog::Transitions => rsx! { Transitions {} },
        Dialog::ReplacePages(path) => rsx! { ReplacePages { path: path.display().to_string() } },
        Dialog::OverlayFile(path) => rsx! { OverlayFile { path: path.display().to_string() } },
        Dialog::Print(all) => rsx! { PrintDialog { all_docs: all } },
        Dialog::Callout { page, tip, rect } => {
            rsx! { CalloutDialog { page, tip_x: tip.0, tip_y: tip.1, rect: rect.to_vec() } }
        }
        Dialog::InsertText { page, index, after } => rsx! { InsertTextDialog { page, index, after } },
        Dialog::ReplaceText { page, start, end } => rsx! { ReplaceTextDialog { page, start, end } },
        Dialog::Calibrate => rsx! { CalibrateDialog {} },
        Dialog::SummarizeComments => rsx! { SummarizeCommentsDialog {} },
        Dialog::Reply { page, index } => rsx! { ReplyDialog { page, index } },
        Dialog::ReviewStatus { page, index } => rsx! { ReviewStatusDialog { page, index } },
        Dialog::ReviewHistory { page, index } => rsx! { ReviewHistoryDialog { page, index } },
        Dialog::CommentFilter => rsx! { CommentFilterDialog {} },
        Dialog::CommentsViewOptions => rsx! { CommentsViewOptionsDialog {} },
        Dialog::NamedDestEdit(name) => rsx! { NamedDestEditDialog { name } },
        Dialog::BookmarkOptions => rsx! { BookmarkOptionsDialog {} },
        Dialog::BookmarkImportExport => rsx! { BookmarkImportExportDialog {} },
        Dialog::BookmarkProps(path) => rsx! { BookmarkPropsDialog { path } },
        Dialog::Toolbars => rsx! { ToolbarsDialog {} },
        Dialog::ZoomLevel => rsx! { ZoomLevelDialog {} },
        Dialog::Sign {
            certify,
            invisible,
            field,
        } => rsx! { crate::ui::signatures::SignDialog { certify, invisible, field } },
        Dialog::SignatureDetails(field) => rsx! { crate::ui::signatures::SignatureDetailsDialog { field } },
        Dialog::DigitalIds => rsx! { crate::ui::signatures::DigitalIdsDialog {} },
        Dialog::TrustedCerts => rsx! { crate::ui::signatures::TrustedCertsDialog {} },
        Dialog::SignatureLibrary => rsx! { crate::ui::signs::SignatureLibraryDialog {} },
        Dialog::MacroGuide => rsx! { crate::ui::macros::MacroGuideDialog {} },
        Dialog::LaunchApp => rsx! { crate::ui::macros::LaunchAppDialog {} },
        Dialog::LayerName { group, rename } => {
            rsx! { crate::ui::layers_pane::LayerNameDialog { group, rename } }
        }
        Dialog::LayerOverrides => rsx! { crate::ui::layers_pane::LayerOverridesDialog {} },
        Dialog::LayerMerge => rsx! { crate::ui::layers_pane::LayerMergeDialog {} },
        Dialog::ImportLayer(path) => {
            rsx! { crate::ui::layers_pane::ImportLayerDialog { path: path.display().to_string() } }
        }
        Dialog::LayerOptions => rsx! { crate::ui::layers_pane::LayerOptionsDialog {} },
        Dialog::AttachmentEdit(index) => rsx! { crate::ui::attachments_pane::AttachmentEditDialog { index } },
        Dialog::AttachmentOptions => rsx! { crate::ui::attachments_pane::AttachmentOptionsDialog {} },
        Dialog::FieldsOptions => rsx! { crate::ui::fields_pane::FieldsOptionsDialog {} },
        Dialog::PortfolioFolder { id } => rsx! { crate::ui::portfolio::PortfolioFolderDialog { id } },
        Dialog::PortfolioItem(file) => rsx! { crate::ui::portfolio::PortfolioItemDialog { file } },
        Dialog::PortfolioProps => rsx! { crate::ui::portfolio::PortfolioPropsDialog {} },
        Dialog::SessionName(index) => rsx! { crate::ui::history_pane::SessionNameDialog { index } },
        Dialog::StampValues(id) => rsx! { crate::ui::stamps::StampValuesDialog { id } },
        Dialog::StampText(id) => rsx! { crate::ui::stamps::StampTextDialog { id } },
        Dialog::StampGroup(index) => rsx! { crate::ui::stamps::StampGroupDialog { index } },
        Dialog::StampOptions => rsx! { crate::ui::stamps::StampOptionsDialog {} },
        Dialog::StampRecolor(id) => rsx! { crate::ui::stamps::StampRecolorDialog { id } },
        Dialog::StampDesigner => rsx! { crate::ui::stamps::StampDesignerDialog {} },
        Dialog::ManageMarks(kind) => rsx! { crate::ui::doctools::ManageMarksDialog { kind } },
        Dialog::SpaceAudit => rsx! { crate::ui::doctools::SpaceAuditDialog {} },
        Dialog::Recompress(current) => rsx! { crate::ui::doctools::RecompressDialog { current } },
        Dialog::InsertTextPages => rsx! { crate::ui::doctools::InsertTextPagesDialog {} },
        Dialog::MultiPlace => rsx! { crate::ui::doctools::MultiPlaceDialog {} },
        Dialog::DocumentActions => rsx! { crate::ui::doctools::DocumentActionsDialog {} },
        Dialog::Compare => rsx! { crate::ui::compare::CompareDialog {} },
        Dialog::TableNew { page, rect } => {
            rsx! { crate::ui::tables::TableNewDialog { page, rect: rect.to_vec() } }
        }
        Dialog::TableEditor => rsx! { crate::ui::tables::TableEditorDialog {} },
        Dialog::FieldCopies(field) => rsx! { crate::ui::tables::FieldCopiesDialog { field } },
        Dialog::TagNew { parent, index } => rsx! { crate::ui::acc_dialogs::TagNewDialog { parent, index } },
        Dialog::TagProps(id) => rsx! { crate::ui::acc_dialogs::TagPropsDialog { id } },
        Dialog::TagFromArea { page, rect } => {
            rsx! { crate::ui::acc_dialogs::TagFromAreaDialog { page, rect: rect.to_vec() } }
        }
        Dialog::RoleMap => rsx! { crate::ui::acc_dialogs::RoleMapDialog {} },
        Dialog::ClassMap => rsx! { crate::ui::acc_dialogs::ClassMapDialog {} },
        Dialog::AltText => rsx! { crate::ui::acc_dialogs::AltTextDialog {} },
        Dialog::AccOptions(pane) => rsx! { crate::ui::acc_dialogs::AccOptionsDialog { pane } },
        Dialog::RuleExplain(id) => rsx! { crate::ui::acc_dialogs::RuleExplainDialog { id } },
        Dialog::DocLanguage => rsx! { crate::ui::acc_dialogs::DocLanguageDialog {} },
        Dialog::CalcSettings => rsx! { crate::ui::calculator::CalcSettingsDialog {} },
        Dialog::RenameDoc(name) => rsx! { crate::ui::fileops::RenameDocDialog { name } },
        Dialog::MetaTemplates => rsx! { crate::ui::fileops::MetaTemplatesDialog {} },
        Dialog::MetaTemplateSave => rsx! { crate::ui::fileops::MetaTemplateSaveDialog {} },
        Dialog::JsConsole => rsx! { crate::ui::javascript::JsConsoleDialog {} },
        Dialog::JsPrefs => rsx! { crate::ui::javascript::JsPrefsDialog {} },
        Dialog::DocScripts(add) => rsx! { crate::ui::javascript::DocScriptsDialog { add } },
        Dialog::MediaList => rsx! { crate::ui::media::MediaListDialog {} },
        Dialog::MediaProps { page, index } => rsx! { crate::ui::media::MediaPropsDialog { page, index } },
    }
}

#[component]
fn ToolbarsDialog() -> Element {
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
fn ZoomLevelDialog() -> Element {
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
fn NamedDestEditDialog(name: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let dest = st
        .read_active(|t| t.doc.named_destinations().into_iter().find(|d| d.name == name))
        .flatten();
    let Some(dest) = dest else {
        return rsx! {};
    };
    let original_page = dest.page;
    let new_name = use_signal(|| dest.name.clone());
    let page_s = use_signal(|| dest.page.map(|p| (p + 1).to_string()).unwrap_or_default());
    let old = name.clone();
    let count = st.page_count();
    let view_text = dest
        .view
        .as_ref()
        .map(|v| v.describe())
        .unwrap_or_else(|| "—".into());
    rsx! {
        Frame { title: t(lang, "cmd.pdfNamedDestsView.edit"),
            on_ok: move |_| {
                let n = new_name().trim().to_string();
                let page: Option<usize> = page_s().trim().parse::<usize>().ok().filter(|p| *p >= 1 && *p <= count).map(|p| p - 1);
                let old = old.clone();
                let r = st.with_active(|t| -> folitecta_core::Result<()> {
                    if n != old {
                        t.doc.rename_named_destination(&old, &n)?;
                    }
                    if let Some(p) = page.filter(|p| Some(*p) != original_page) {
                        t.doc.set_named_destination(&n, p, None)?;
                    }
                    Ok(())
                });
                st.with_active(|t| t.selected_dest = Some(n.clone()));
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                }
            },
            Text { label: t(lang, "dialog.dest.name"), value: new_name, autofocus: true }
            Num { label: t(lang, "status.page"), value: page_s, min: "1", max: "{count}" }
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.dest.view\")}" } span { class: "v", "{view_text}" } }
        }
    }
}

#[component]
fn BookmarkOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let groups: [(&str, &[&str]); 3] = [
        (
            "dialog.bookmarks.create",
            &[
                "bookmarksView.useTextForTitle",
                "bookmarksView.useCurrentPage",
                "bookmarksView.useInheritedZoom",
                "bookmarksView.useRegionForDest",
            ],
        ),
        (
            "dialog.bookmarks.view",
            &[
                "bookmarksView.showIcons",
                "bookmarksView.wrapTitles",
                "bookmarksView.showContrastText",
                "bookmarksView.markInvalid",
                "bookmarksView.autoExpandBookmarks",
                "bookmarksView.ensureVisibility",
            ],
        ),
        (
            "dialog.bookmarks.dests",
            &[
                "pdfNamedDestsView.useTextForTitle",
                "pdfNamedDestsView.useInheritedZoom",
                "pdfNamedDestsView.useRegionForDest",
                "pdfNamedDestsView.wrapTitles",
                "pdfNamedDestsView.hdr.page",
                "pdfNamedDestsView.hdr.type",
                "pdfNamedDestsView.hdr.details",
            ],
        ),
    ];
    let size = st
        .ui
        .read()
        .value("bookmarks.text_size")
        .unwrap_or("medium")
        .to_string();
    rsx! {
        Frame { title: t(lang, "cmd.bookmarksView.options").trim_end_matches('…').to_string(), wide: true,
            div { class: "field-grid",
                for (title, ids) in groups {
                    div { class: "field", key: "{title}",
                        label { "{t(lang, title)}" }
                        div { class: "check-list",
                            for id in ids.iter().copied() {
                                {
                                    let on = option_on(st, id);
                                    let label = reg.get(id).map(|c| cmd_label(lang, c)).unwrap_or_else(|| id.to_string());
                                    rsx! { label { class: "check", key: "{id}", input { r#type: "checkbox", checked: on, onchange: move |_| st.set_option(id, !on) } span { "{label}" } } }
                                }
                            }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"cmd.bookmarksView.textSize\")}" }
                    select { onchange: move |e| st.set_value("bookmarks.text_size", e.value()),
                        for (v, key) in [("small", "cmd.bookmarksView.textSize.small"), ("medium", "cmd.bookmarksView.textSize.medium"), ("large", "cmd.bookmarksView.textSize.large")] {
                            option { key: "{v}", value: "{v}", selected: size == v, "{t(lang, key)}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BookmarkImportExportDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let has_sel = crate::commands::bookmarks::selected_path(st).is_some();
    let run = move |id: &'static str| {
        move |_| {
            st.set_dialog(Dialog::None);
            crate::commands::dispatch(st, id);
        }
    };
    let copy = move |_| {
        let text = st
            .read_active(|t| t.doc.export_bookmarks_text(None))
            .unwrap_or_default();
        crate::clipboard::set_text(&text);
        st.set_dialog(Dialog::None);
        st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), "");
    };
    rsx! {
        Frame { title: t(lang, "cmd.bookmarksView.importExport"),
            div { class: "hint", "{t(lang, \"dialog.bookmarks.format\")}" }
            div { class: "button-col",
                button { class: "tbtn", onclick: run("document.exportBookmarks"), Icon { name: "export" } "{t(lang, \"cmd.document.exportBookmarks\")}" }
                button { class: "tbtn", disabled: !has_sel, onclick: run("document.exportSelBookmarks"), Icon { name: "export" } "{t(lang, \"cmd.document.exportSelBookmarks\")}" }
                button { class: "tbtn", onclick: run("document.importBookmarks"), Icon { name: "import" } "{t(lang, \"cmd.document.importBookmarks\")}" }
                button { class: "tbtn", onclick: copy, Icon { name: "copy" } "{t(lang, \"dialog.bookmarks.copy\")}" }
                button { class: "tbtn", onclick: run("pasteBookmarks"), Icon { name: "clipboard" } "{t(lang, \"cmd.pasteBookmarks\")}" }
            }
        }
    }
}

#[component]
fn BookmarkPropsDialog(path: Vec<usize>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let item = st
        .read_active(|t| folitecta_core::outline::item_at(&t.doc.outline(), &path).cloned())
        .flatten();
    let Some(item) = item else {
        return rsx! {};
    };
    let count = st.page_count();
    let title = use_signal(|| item.title.clone());
    let mut kind = use_signal(|| {
        if item.page.is_none() && !item.action.is_empty() {
            "uri".to_string()
        } else {
            "page".to_string()
        }
    });
    let page_s = use_signal(|| {
        item.page
            .map(|p| (p + 1).to_string())
            .unwrap_or_else(|| "1".into())
    });
    let uri = use_signal(|| item.action.clone());
    let mut view = use_signal(|| "keep".to_string());
    let mut color = use_signal(|| {
        item.color
            .map(crate::util::color_hex)
            .unwrap_or_else(|| "#000000".into())
    });
    let mut has_color = use_signal(|| item.color.is_some());
    let mut bold = use_signal(|| item.bold);
    let mut italic = use_signal(|| item.italic);
    let original_page = item.page;
    let original_uri = item.action.clone();
    let p = path.clone();
    rsx! {
        Frame { title: t(lang, "dialog.bookmark.props"), wide: true,
            on_ok: move |_| {
                use folitecta_core::{BookmarkTarget, DestView};
                let page: Option<usize> = page_s().trim().parse::<usize>().ok().filter(|v| *v >= 1 && *v <= count).map(|v| v - 1);
                let target = if kind() == "uri" {
                    if uri().trim() == original_uri.trim() { BookmarkTarget::Keep } else { BookmarkTarget::Uri(uri()) }
                } else {
                    match (page, view().as_str()) {
                        (Some(pg), "keep") if Some(pg) == original_page => BookmarkTarget::Keep,
                        (Some(pg), "fit") => BookmarkTarget::Page(pg, Some(DestView::Fit)),
                        (Some(pg), "width") => BookmarkTarget::Page(pg, Some(DestView::FitWidth(None))),
                        (Some(pg), _) => BookmarkTarget::Page(pg, None),
                        (None, _) => BookmarkTarget::Keep,
                    }
                };
                let c = has_color().then(|| parse_color(&color()));
                let (ti, b, i, pp) = (title(), bold(), italic(), p.clone());
                apply(st, move |tab| tab.doc.set_bookmark_properties(&pp, &ti, target, c, b, i));
                st.with_active(|t| t.selected_bookmark = Some(p.clone()));
            },
            Text { label: t(lang, "dialog.bookmark.name"), value: title, autofocus: true }
            div { class: "field",
                label { "{t(lang, \"dialog.bookmark.target\")}" }
                div { class: "radio-row",
                    label { input { r#type: "radio", name: "bm-kind", checked: kind() == "page", onchange: move |_| kind.set("page".into()) } "{t(lang, \"status.page\")}" }
                    label { input { r#type: "radio", name: "bm-kind", checked: kind() == "uri", onchange: move |_| kind.set("uri".into()) } "{t(lang, \"dialog.bookmark.web\")}" }
                }
            }
            if kind() == "uri" {
                Text { label: t(lang, "dialog.link.url"), value: uri, placeholder: "https://" }
            } else {
                div { class: "field-grid",
                    Num { label: t(lang, "status.page"), value: page_s, min: "1", max: "{count}" }
                    div { class: "field",
                        label { "{t(lang, \"dialog.dest.view\")}" }
                        select { onchange: move |e| view.set(e.value()),
                            option { value: "keep", selected: view() == "keep", "{t(lang, \"dialog.bookmark.view_keep\")}" }
                            option { value: "top", selected: view() == "top", "{t(lang, \"dialog.bookmark.view_top\")}" }
                            option { value: "fit", selected: view() == "fit", "{t(lang, \"cmd.zoom.fitPage\")}" }
                            option { value: "width", selected: view() == "width", "{t(lang, \"cmd.zoom.fitWidth\")}" }
                        }
                    }
                }
            }
            div { class: "style-row",
                label { "{t(lang, \"style.text_color\")}" }
                input { r#type: "color", value: "{color}", disabled: !has_color(), onchange: move |e| color.set(e.value()) }
                label { class: "check mini", input { r#type: "checkbox", checked: !has_color(), onchange: move |_| has_color.set(!has_color()) } span { "{t(lang, \"style.none\")}" } }
            }
            div { class: "check-row",
                label { class: "check mini", input { r#type: "checkbox", checked: bold(), onchange: move |_| bold.set(!bold()) } span { "{t(lang, \"cmd.bookmarks.bold\")}" } }
                label { class: "check mini", input { r#type: "checkbox", checked: italic(), onchange: move |_| italic.set(!italic()) } span { "{t(lang, \"cmd.bookmarks.italic\")}" } }
            }
        }
    }
}

fn review_name(lang: Lang, state: &str) -> String {
    t(lang, &format!("review.{}", state.to_lowercase()))
}

fn group_name(lang: Lang, group: &str) -> String {
    t(lang, &format!("group.{group}"))
}

#[component]
fn ReplyDialog(page: usize, index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let comment = st
        .read_active(|t| {
            t.doc
                .page_comment_infos(page)
                .into_iter()
                .find(|c| c.index == index)
        })
        .flatten();
    let Some(c) = comment else {
        return rsx! {};
    };
    let replies: Vec<(String, String, String)> = st
        .read_active(|t| {
            t.doc
                .page_comment_infos(page)
                .into_iter()
                .filter(|x| x.reply_to == Some(c.id) && !x.is_state)
                .map(|x| (x.author, x.modified, x.contents))
                .collect()
        })
        .unwrap_or_default();
    rsx! {
        Frame { title: t(lang, "cmd.annot.addReply"), ok_label: Some(t(lang, "popup.reply")),
            on_ok: move |_| {
                let author = st.ui.peek().author.clone();
                let r = st.with_active(|t| t.doc.add_reply(page, index, &text(), &author));
                st.with_active(|t| t.selected_annot = Some((page, index)));
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                }
            },
            div { class: "thread",
                div { class: "thread-item",
                    div { class: "meta", "{crate::i18n::annot_name(lang, &c.subtype)} · {c.author} · {c.modified}" }
                    div { class: "text", "{c.contents}" }
                }
                for (i, (author, date, body)) in replies.into_iter().enumerate() {
                    div { class: "thread-item reply", key: "{i}",
                        div { class: "meta", "{author} · {date}" }
                        div { class: "text", "{body}" }
                    }
                }
            }
            Area { label: t(lang, "popup.reply_placeholder"), value: text, autofocus: true }
        }
    }
}

#[component]
fn ReviewStatusDialog(page: usize, index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let comment = st
        .read_active(|t| {
            t.doc
                .page_comment_infos(page)
                .into_iter()
                .find(|c| c.index == index)
        })
        .flatten();
    let Some(c) = comment else {
        return rsx! {};
    };
    let initial = if c.review.is_empty() {
        "None".to_string()
    } else {
        c.review.clone()
    };
    let initial_marked = c.marked;
    let mut state = use_signal(|| initial.clone());
    let mut marked = use_signal(|| initial_marked);
    rsx! {
        Frame { title: t(lang, "cmd.annot.status"),
            on_ok: move |_| {
                st.set_dialog(Dialog::None);
                if state() != initial {
                    crate::commands::review::set_state(st, page, index, "Review", &state());
                }
                if marked() != initial_marked {
                    crate::commands::review::set_state(st, page, index, "Marked", if marked() { "Marked" } else { "Unmarked" });
                }
            },
            div { class: "field",
                label { "{t(lang, \"review.state\")}" }
                div { class: "radio-col",
                    for s in folitecta_core::REVIEW_STATES.iter().copied() {
                        label { key: "{s}", input { r#type: "radio", name: "review-state", checked: state() == s, onchange: move |_| state.set(s.to_string()) } "{review_name(lang, s)}" }
                    }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: marked(), onchange: move |_| marked.set(!marked()) } span { "{t(lang, \"review.marked\")}" } }
        }
    }
}

#[component]
fn ReviewHistoryDialog(page: usize, index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let history = st
        .read_active(|t| t.doc.review_history(page, index))
        .and_then(|r| r.ok())
        .unwrap_or_default();
    rsx! {
        Frame { title: t(lang, "cmd.annot.reviewHistory"),
            if history.is_empty() {
                div { class: "hint", "{t(lang, \"review.no_history\")}" }
            } else {
                table { class: "grid-table",
                    thead { tr { th { "{t(lang, \"dialog.summary.by_date\")}" } th { "{t(lang, \"dialog.summary.by_author\")}" } th { "{t(lang, \"review.state\")}" } } }
                    tbody {
                        for (i, h) in history.into_iter().enumerate() {
                            tr { key: "{i}",
                                td { "{h.date}" }
                                td { "{h.author}" }
                                td { "{review_name(lang, &h.state)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CommentFilterDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((infos, current)) = st.read_active(|t| (t.doc.comment_infos(), t.comment_filter.clone())) else {
        return rsx! {};
    };
    let top: Vec<_> = infos
        .into_iter()
        .filter(|c| c.reply_to.is_none() && !c.is_state)
        .collect();
    let mut authors: Vec<String> = top.iter().map(|c| c.author.clone()).collect();
    authors.sort();
    authors.dedup();
    let mut types: Vec<String> = top.iter().map(|c| c.subtype.clone()).collect();
    types.sort();
    types.dedup();
    let mut filter = use_signal(|| current.clone());
    let status_value = filter()
        .status
        .clone()
        .map(|s| if s.is_empty() { "-".to_string() } else { s })
        .unwrap_or_default();
    let checked_value = match filter().checked {
        Some(true) => "yes",
        Some(false) => "no",
        None => "",
    };
    rsx! {
        Frame { title: t(lang, "filter.title"), wide: true,
            on_ok: move |_| {
                let f = filter();
                st.with_active(|t| t.set_comment_filter(f));
                st.set_dialog(Dialog::None);
            },
            div { class: "field-grid",
                div { class: "field",
                    label { "{t(lang, \"cmd.comments.show.byAuthor\")}" }
                    div { class: "check-list",
                        for a in authors {
                            {
                                let shown = !filter().hidden_authors.contains(&a);
                                let a2 = a.clone();
                                let label = if a.is_empty() { "—".to_string() } else { a.clone() };
                                rsx! { label { class: "check", key: "{a}", input { r#type: "checkbox", checked: shown, onchange: move |_| filter.with_mut(|f| { if !f.hidden_authors.remove(&a2) { f.hidden_authors.insert(a2.clone()); } }) } span { "{label}" } } }
                            }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"cmd.comments.show.byType\")}" }
                    div { class: "check-list",
                        for ty in types {
                            {
                                let shown = !filter().hidden_types.contains(&ty);
                                let ty2 = ty.clone();
                                rsx! { label { class: "check", key: "{ty}", input { r#type: "checkbox", checked: shown, onchange: move |_| filter.with_mut(|f| { if !f.hidden_types.remove(&ty2) { f.hidden_types.insert(ty2.clone()); } }) } span { "{crate::i18n::annot_name(lang, &ty)}" } } }
                            }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"review.state\")}" }
                    select { onchange: move |e| { let v = e.value(); filter.with_mut(|f| f.status = match v.as_str() { "" => None, "-" => Some(String::new()), s => Some(s.to_string()) }); },
                        option { value: "", selected: status_value.is_empty(), "{t(lang, \"filter.all\")}" }
                        option { value: "-", selected: status_value == "-", "{t(lang, \"review.none\")}" }
                        for s in folitecta_core::REVIEW_STATES.iter().copied().filter(|s| *s != "None") {
                            option { key: "{s}", value: "{s}", selected: status_value == s, "{review_name(lang, s)}" }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"cmd.comments.show.checked\")}" }
                    select { onchange: move |e| { let v = e.value(); filter.with_mut(|f| f.checked = match v.as_str() { "yes" => Some(true), "no" => Some(false), _ => None }); },
                        option { value: "", selected: checked_value.is_empty(), "{t(lang, \"filter.all\")}" }
                        option { value: "yes", selected: checked_value == "yes", "{t(lang, \"review.marked\")}" }
                        option { value: "no", selected: checked_value == "no", "{t(lang, \"review.unmarked\")}" }
                    }
                }
            }
            div { class: "row",
                button { class: "tbtn", onclick: move |_| filter.set(crate::state::CommentFilter::default()), "{t(lang, \"filter.show_all\")}" }
            }
        }
    }
}

#[component]
fn CommentsViewOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let group = crate::commands::review::current_group(st);
    let toggles = [
        "commentsView.show.authors",
        "commentsView.show.dates",
        "commentsView.show.colors",
        "commentsView.show.subjects",
        "commentsView.show.states",
        "commentsView.show.text",
        "commentsView.show.noTextPlaceholder",
        "commentsView.show.compactItems",
        "commentsView.show.uniformText",
        "commentsView.show.dynamicIcons",
        "commentsView.show.wrapLongTitles",
        "commentsView.showHiddenComments",
        "commentsView.showCommentsFromHiddenLayers",
    ];
    let reg = registry();
    rsx! {
        Frame { title: t(lang, "cmd.commentsView.options"),
            div { class: "field",
                label { "{t(lang, \"cmd.commentsView.groupBy\")}" }
                select { onchange: move |e| crate::commands::review::set_group(st, &e.value()),
                    for g in crate::commands::review::GROUPS.iter().copied().filter(|g| *g != "stateAndAuthor") {
                        option { key: "{g}", value: "{g}", selected: group == g, "{group_name(lang, g)}" }
                    }
                }
            }
            div { class: "check-list",
                for id in toggles {
                    {
                        let on = option_on(st, id);
                        let label = reg.get(id).map(|c| cmd_label(lang, c)).unwrap_or_else(|| id.to_string());
                        rsx! { label { class: "check", key: "{id}", input { r#type: "checkbox", checked: on, onchange: move |_| st.set_option(id, !on) } span { "{label}" } } }
                    }
                }
            }
        }
    }
}

#[component]
fn PrintDialog(all_docs: bool) -> Element {
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
            div { class: "matrix", table { thead { tr { th { "{t(lang, \"dialog.shortcuts.key\")}" } th { "{t(lang, \"dialog.shortcuts.command\")}" } } } tbody { for (k, v) in rows { tr { key: "{k}", td { class: "mono", "{k}" } td { "{v}" } } } } } }
        }
    }
}

/// Localized name of a mark position.
fn pos_label(lang: Lang, p: MarkPosition) -> String {
    t(
        lang,
        &format!("pos.{}", p.label().to_lowercase().replace(' ', "_")),
    )
}

/// Localized status badge for the feature matrix.
fn matrix_badge(lang: Lang, id: &str) -> Element {
    if is_partial(id) {
        rsx! { span { class: "badge partial", "{t(lang, \"label.partial\").to_lowercase()}" } }
    } else if is_implemented(id) {
        rsx! { span { class: "badge ok", "{t(lang, \"label.available\")}" } }
    } else {
        rsx! { span { class: "badge planned", "{t(lang, \"label.planned\").to_lowercase()}" } }
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
                Num { label: t(lang, "dialog.margin.top"), value: top, step: "1" }
                Num { label: t(lang, "dialog.hf.right"), value: right, step: "1" }
                Num { label: t(lang, "dialog.margin.bottom"), value: bottom, step: "1" }
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
            Num { label: tf(lang, "dialog.move.before", &[("count", &(count + 1).to_string())]), value: to, min: "1", max: (count + 1).to_string() }
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
            div { class: "field-grid", Num { label: t(lang, "dialog.swap.a"), value: a, min: "1" } Num { label: t(lang, "dialog.swap.b"), value: b, min: "1" } }
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
            div { class: "field-grid", Num { label: t(lang, "dialog.watermark.size"), value: size, min: "5", max: "72" } Num { label: t(lang, "dialog.margin.pt"), value: margin, min: "0", max: "200" } }
            crate::ui::macros::MacroPicker {}
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
fn RemoveMarks() -> Element {
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
                    select { onchange: move |e| format.set(e.value()), option { value: "png", "PNG" } option { value: "jpg", "JPEG" } option { value: "txt", "{t(lang, \"dialog.export.text\")}" } }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: images_only(), onchange: move |_| images_only.set(!images_only()) } span { "{t(lang, \"dialog.export.embedded\")}" } }
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
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.notimpl.command\")}" } span { class: "v mono", "{id}" } span { class: "k", "{t(lang, \"dialog.notimpl.location\")}" } span { class: "v", "{path}" } }
            button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::Matrix), "{t(lang, \"dialog.matrix.title\")}" }
        }
    }
}

/// Run an operation that reports a count, close the dialog and show `toast_key`.
fn apply_count(
    st: AppState,
    toast_key: &str,
    f: impl FnOnce(&mut crate::state::DocTab) -> folitecta_core::Result<usize>,
) {
    match st.with_active(f) {
        Some(Ok(n)) => {
            files::set_window_title(st);
            st.set_dialog(Dialog::None);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), toast_key, &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => st.set_dialog(Dialog::None),
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
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.file\")}" } span { class: "v", "{path}" } }
            div { class: "field", label { "{t(lang, \"dialog.password.field\")}" } input { r#type: "password", autofocus: true, value: "{pw}", oninput: move |e| { let mut s = pw; s.set(e.value()); } } }
        }
    }
}

/// Return to the hand tool after creating something, unless the user keeps tools selected.
fn tool_done(st: AppState) {
    if !option_on(st, "tool.keepSelected") {
        st.set_tool(Tool::Hand);
    }
}

#[component]
fn NoteDialog(page: usize, x: f32, y: f32) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let author = use_signal(|| st.ui.read().author.clone());
    let mut color = use_signal(|| st.ui.read().value("note.color").unwrap_or("#ffd833").to_string());
    rsx! {
        Frame { title: t(lang, "dialog.note.title"),
            on_ok: move |_| {
                let (c, a, col) = (text(), author(), color());
                st.set_value("note.color", col.clone());
                apply(st, |tab| tab.doc.add_note(page, x, y, &c, &a, parse_color(&col)).map(|_| ()));
                tool_done(st);
            },
            Area { label: t(lang, "dialog.note.text"), value: text, autofocus: true }
            div { class: "field-grid",
                Text { label: t(lang, "dialog.note.author"), value: author }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
            }
        }
    }
}

#[component]
fn TextBoxDialog(page: usize, rect: Vec<f32>, border: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let size = use_signal(|| st.ui.read().value("text.size").unwrap_or("12").to_string());
    let mut color = use_signal(|| {
        st.ui
            .read()
            .value("text.color")
            .unwrap_or(if border { "#1d4ed8" } else { "#000000" })
            .to_string()
    });
    let mut with_border = use_signal(|| border);
    let r = [rect[0], rect[1], rect[2], rect[3]];
    let title = if border {
        t(lang, "tool.text")
    } else {
        t(lang, "tool.typewriter")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let (c, fs, col, b) = (text(), num_or(&size, 12.0).clamp(4.0, 200.0), color(), with_border());
                st.set_value("text.size", fs.to_string());
                st.set_value("text.color", col.clone());
                let author = st.ui.read().author.clone();
                apply(st, |tab| tab.doc.add_free_text(page, r, &c, &author, fs, parse_color(&col), b).map(|_| ()));
                tool_done(st);
            },
            Area { label: t(lang, "dialog.note.text"), value: text, autofocus: true }
            div { class: "field-grid",
                Num { label: t(lang, "dialog.watermark.size"), value: size, min: "4", max: "200" }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
            }
            label { class: "check", input { r#type: "checkbox", checked: with_border(), onchange: move |_| with_border.set(!with_border()) } span { "{t(lang, \"dialog.textbox.border\")}" } }
        }
    }
}

#[component]
fn LinkDialog(page: usize, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let url = use_signal(|| "https://".to_string());
    let target_page = use_signal(|| "1".to_string());
    let mut kind = use_signal(|| "web".to_string());
    let count = st.page_count();
    let r = [rect[0], rect[1], rect[2], rect[3]];
    rsx! {
        Frame { title: t(lang, "dialog.link.title"),
            on_ok: move |_| {
                if kind() == "page" {
                    let p = num_or(&target_page, 1.0) as usize;
                    if p < 1 || p > count {
                        st.error(tf(st.lang(), "dialog.replace.bad_range", &[("count", &count.to_string())]));
                        return;
                    }
                    apply(st, |tab| tab.doc.add_page_link(page, r, p - 1).map(|_| ()));
                } else {
                    let u = url();
                    apply(st, |tab| tab.doc.add_uri_link(page, r, &u).map(|_| ()));
                }
                tool_done(st);
            },
            div { class: "field",
                div { class: "radio-row",
                    label { input { r#type: "radio", checked: kind() == "web", onchange: move |_| kind.set("web".into()) } "{t(lang, \"dialog.link.web\")}" }
                    label { input { r#type: "radio", checked: kind() == "page", onchange: move |_| kind.set("page".into()) } "{t(lang, \"dialog.link.page\")}" }
                }
            }
            if kind() == "page" {
                Num { label: format!("{} (1-{count})", t(lang, "status.page")), value: target_page, min: "1", max: count.to_string() }
            } else {
                Text { label: t(lang, "dialog.link.url"), value: url, autofocus: true }
            }
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
    rsx! { Frame { title: t(lang, "cmd.document.wordCount"), div { class: "kv", span { class: "k", "{t(lang, \"dialog.wordcount.words\")}" } span { class: "v", "{words}" } span { class: "k", "{t(lang, \"dialog.wordcount.chars\")}" } span { class: "v", "{chars}" } } } }
}

/// Standard paper sizes in points (portrait).
const PAPER: &[(&str, &str, f32, f32)] = &[
    ("a4", "A4 (210 × 297 mm)", 595.28, 841.89),
    ("a3", "A3 (297 × 420 mm)", 841.89, 1190.55),
    ("a5", "A5 (148 × 210 mm)", 419.53, 595.28),
    ("b5", "B5 (176 × 250 mm)", 498.9, 708.66),
    ("letter", "Letter (8.5 × 11 in)", 612.0, 792.0),
    ("legal", "Legal (8.5 × 14 in)", 612.0, 1008.0),
    ("tabloid", "Tabloid (11 × 17 in)", 792.0, 1224.0),
];

fn paper_size(id: &str) -> Option<(f32, f32)> {
    PAPER.iter().find(|p| p.0 == id).map(|p| (p.2, p.3))
}

fn num_or(s: &Signal<String>, default: f32) -> f32 {
    s.read().trim().replace(',', ".").parse().unwrap_or(default)
}

#[component]
fn NewBlank() -> Element {
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
fn PasteSpecial() -> Element {
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
fn Rasterize() -> Element {
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

#[component]
fn StampPicker() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let custom = use_signal(|| st.ui.read().value("stamp.custom").unwrap_or("").to_string());
    let mut color = use_signal(|| st.ui.read().value("stamp.color").unwrap_or("#c81e1e").to_string());
    let choose = move |text: String| {
        crate::commands::stamps::clear_pending();
        st.set_value("stamp.text", text);
        st.set_value("stamp.color", color());
        st.set_value("stamp.kind", "text");
        st.set_dialog(Dialog::None);
        st.set_tool(Tool::Stamp);
        st.toast(ToastKind::Info, t(st.lang(), "toast.place_stamp"), "");
    };
    rsx! {
        Frame { title: t(lang, "dialog.stamp.title"), wide: true, ok_label: Some(t(lang, "dialog.stamp.use_custom")),
            on_ok: move |_| {
                let c = custom().trim().to_string();
                if c.is_empty() {
                    st.toast(ToastKind::Info, t(st.lang(), "dialog.stamp.custom_empty"), "");
                } else {
                    st.set_value("stamp.custom", c.clone());
                    choose(c);
                }
            },
            div { class: "stamp-grid",
                for p in STAMP_PRESETS.iter() {
                    button { class: "stamp-preview", key: "{p}", style: "color:{color};border-color:{color}", onclick: move |_| choose(p.to_string()), "{p}" }
                }
            }
            div { class: "field-grid",
                Text { label: t(lang, "dialog.stamp.custom"), value: custom, placeholder: "PAID" }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
            }
            div { class: "row", style: "gap:8px",
                button { class: "tbtn", onclick: move |_| { st.set_dialog(Dialog::None); files::pick_image_stamp(st); }, Icon { name: "image" } "{t(lang, \"dialog.stamp.image\")}" }
            }
            div { class: "hint", "{t(lang, \"dialog.stamp.hint\")}" }
        }
    }
}

/// A permission checkbox: label key, current state and the toggle.
type PermRow = (&'static str, bool, fn(&mut SecurityPermissions));

#[component]
fn Security() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut user = use_signal(String::new);
    let mut owner = use_signal(String::new);
    let mut perms = use_signal(SecurityPermissions::default);
    let p = perms();
    let rows: [PermRow; 6] = [
        ("dialog.security.print", p.print, |p| p.print = !p.print),
        ("dialog.security.modify", p.modify, |p| p.modify = !p.modify),
        ("dialog.security.copy", p.copy, |p| p.copy = !p.copy),
        ("dialog.security.annotate", p.annotate, |p| {
            p.annotate = !p.annotate
        }),
        ("dialog.security.fill", p.fill_forms, |p| {
            p.fill_forms = !p.fill_forms
        }),
        ("dialog.security.assemble", p.assemble, |p| {
            p.assemble = !p.assemble
        }),
    ];
    rsx! {
        Frame { title: t(lang, "dialog.security.title"), wide: true, ok_label: Some(t(lang, "dialog.security.save")),
            on_ok: move |_| {
                let (u, o) = (user(), owner());
                if u.is_empty() && o.is_empty() {
                    st.toast(ToastKind::Warn, t(st.lang(), "dialog.security.need_password"), "");
                    return;
                }
                st.set_dialog(Dialog::None);
                files::save_encrypted(st, u, o, perms());
            },
            div { class: "callout", "{t(lang, \"dialog.security.hint\")}" }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.security.user\")}" } input { r#type: "password", autofocus: true, value: "{user}", oninput: move |e| user.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"dialog.security.owner\")}" } input { r#type: "password", value: "{owner}", oninput: move |e| owner.set(e.value()) } }
            }
            b { "{t(lang, \"dialog.security.permissions\")}" }
            div { class: "check-grid",
                for (key, on, toggle) in rows {
                    label { class: "check", key: "{key}",
                        input { r#type: "checkbox", checked: on, onchange: move |_| toggle(&mut perms.write()) }
                        span { "{t(lang, key)}" }
                    }
                }
            }
        }
    }
}

#[component]
fn Flatten() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let mut what = use_signal(|| "comments".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.flatten.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => {
                    let scope = match what().as_str() { "fields" => FlattenScope::FormFields, "all" => FlattenScope::All, _ => FlattenScope::Comments };
                    apply_count(st, "toast.flattened", |tab| tab.doc.flatten(&pages, scope));
                }
                Err(e) => st.error(e),
            },
            div { class: "hint", "{t(lang, \"dialog.flatten.hint\")}" }
            div { class: "field",
                div { class: "radio-row",
                    label { input { r#type: "radio", checked: what() == "comments", onchange: move |_| what.set("comments".into()) } "{t(lang, \"dialog.flatten.comments\")}" }
                    label { input { r#type: "radio", checked: what() == "fields", onchange: move |_| what.set("fields".into()) } "{t(lang, \"dialog.flatten.fields\")}" }
                    label { input { r#type: "radio", checked: what() == "all", onchange: move |_| what.set("all".into()) } "{t(lang, \"dialog.flatten.all\")}" }
                }
            }
            PageScope { mode, range }
        }
    }
}

/// Parse a list of numbers separated by spaces or semicolons (decimal comma allowed).
fn parse_numbers(s: &str) -> Result<Vec<f32>, String> {
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
fn AddGuides() -> Element {
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
fn SplitTiles() -> Element {
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

#[component]
fn Background() -> Element {
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

/// One page-label range being edited: start page, style, prefix, first number.
type LabelRow = (String, String, String, String);

const LABEL_STYLES: &[(&str, &str)] = &[
    ("D", "1, 2, 3"),
    ("r", "i, ii, iii"),
    ("R", "I, II, III"),
    ("a", "a, b, c"),
    ("A", "A, B, C"),
    ("", "—"),
];

#[component]
fn PageLabels() -> Element {
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
fn Transitions() -> Element {
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
fn use_other_document(path: &str) -> Rc<Result<Document, String>> {
    let p = path.to_string();
    use_hook(move || Rc::new(Document::open(&p).map_err(|e| e.to_string())))
}

#[component]
fn ReplacePages(path: String) -> Element {
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
fn OverlayFile(path: String) -> Element {
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

#[component]
fn CalloutDialog(page: usize, tip_x: f32, tip_y: f32, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let r = [rect[0], rect[1], rect[2], rect[3]];
    rsx! {
        Frame { title: t(lang, "tool.callout"),
            on_ok: move |_| {
                let c = text();
                let author = st.ui.read().author.clone();
                let style = crate::styles::default_style(st, "Callout");
                apply(st, |tab| tab.doc.add_callout(page, (tip_x, tip_y), r, &c, &style, &author).map(|_| ()));
                tool_done(st);
            },
            Area { label: t(lang, "dialog.note.text"), value: text, autofocus: true }
        }
    }
}

#[component]
fn InsertTextDialog(page: usize, index: usize, after: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    rsx! {
        Frame { title: t(lang, "tool.insert_text"),
            on_ok: move |_| {
                let c = text();
                let author = st.ui.read().author.clone();
                let color = crate::styles::default_style(st, "Caret").color.unwrap_or([0.1, 0.4, 0.95]);
                apply(st, |tab| tab.doc.insert_text_mark(page, index, after, &c, color, &author).map(|_| ()));
                tool_done(st);
            },
            Area { label: t(lang, "dialog.insert_text.text"), value: text, autofocus: true }
        }
    }
}

#[component]
fn ReplaceTextDialog(page: usize, start: usize, end: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let original = st
        .read_active(|t| t.doc.text_range(page, start, end))
        .unwrap_or_default();
    let text = use_signal(String::new);
    rsx! {
        Frame { title: t(lang, "tool.replace_text"),
            on_ok: move |_| {
                let c = text();
                let author = st.ui.read().author.clone();
                let color = crate::styles::default_style(st, "Caret").color.unwrap_or([0.1, 0.4, 0.95]);
                apply(st, |tab| {
                    let r = tab.doc.replace_text_range(page, start, end, &c, color, &author);
                    tab.text_sel = None;
                    r
                });
                tool_done(st);
            },
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.replace_text.original\")}" } span { class: "v", "{original}" } }
            Area { label: t(lang, "dialog.replace_text.with"), value: text, autofocus: true }
        }
    }
}

/// Units offered for measurement scales.
const SCALE_UNITS: &[&str] = &["mm", "cm", "m", "km", "in", "ft", "pt"];

/// Current measurement scale from the preferences.
pub fn measure_scale(st: AppState) -> folitecta_core::MeasureScale {
    let ui = st.ui.read();
    let get = |k: &str, d: &str| ui.value(k).unwrap_or(d).to_string();
    let paper: f32 = get("measure.paper", "1").replace(',', ".").parse().unwrap_or(1.0);
    let real: f32 = get("measure.real", "1").replace(',', ".").parse().unwrap_or(1.0);
    let decimals: u8 = get("measure.decimals", "1").parse().unwrap_or(1);
    folitecta_core::MeasureScale::from_ratio(
        paper,
        &get("measure.paper_unit", "mm"),
        real,
        &get("measure.real_unit", "mm"),
        decimals,
    )
}

#[component]
fn SummarizeCommentsDialog() -> Element {
    use folitecta_core::{SummaryLabels, SummaryLayout, SummaryOptions, SummarySort};
    let st = use_context::<AppState>();
    let lang = st.lang();
    let get = |k: &str, d: &str| st.ui.read().value(k).unwrap_or(d).to_string();
    let mut layout = use_signal(|| get("summary.layout", "only"));
    let mut sort = use_signal(|| get("summary.sort", "page"));
    let mut size = use_signal(|| get("summary.size", "10"));
    let mut paper = use_signal(|| get("summary.paper", "a4"));
    let mut hidden = use_signal(|| get("summary.hidden", "0") == "1");
    let count = st
        .read_active(|t| t.doc.summary_entries(true, SummarySort::Page).len())
        .unwrap_or(0);
    let layouts = [
        ("only", "dialog.summary.only"),
        ("separate", "dialog.summary.separate"),
        ("connector", "dialog.summary.connector"),
    ];
    let sorts = [
        ("page", "dialog.summary.by_page"),
        ("author", "dialog.summary.by_author"),
        ("date", "dialog.summary.by_date"),
        ("type", "dialog.summary.by_type"),
    ];
    let sizes = [
        ("8", "dialog.summary.small"),
        ("10", "dialog.summary.medium"),
        ("12", "dialog.summary.large"),
    ];
    rsx! {
        Frame { title: t(lang, "cmd.comments.summarize"), ok_label: Some(t(lang, "dialog.summary.create")),
            on_ok: move |_| {
                st.set_value("summary.layout", layout());
                st.set_value("summary.sort", sort());
                st.set_value("summary.size", size());
                st.set_value("summary.paper", paper());
                st.set_value("summary.hidden", if hidden() { "1" } else { "0" });
                let (_, _, pw, ph) = PAPER.iter().copied().find(|p| p.0 == paper()).unwrap_or(PAPER[0]);
                let types = ["Text", "FreeText", "Line", "Square", "Circle", "Polygon", "PolyLine", "Highlight", "Underline", "Squiggly", "StrikeOut", "Stamp", "Caret", "Ink", "FileAttachment", "Sound", "Redact"]
                    .iter()
                    .map(|s| (s.to_string(), crate::i18n::annot_name(lang, s)))
                    .collect();
                let opts = SummaryOptions {
                    layout: match layout().as_str() { "separate" => SummaryLayout::SeparatePages, "connector" => SummaryLayout::ConnectorLines, _ => SummaryLayout::CommentsOnly },
                    sort: match sort().as_str() { "author" => SummarySort::Author, "date" => SummarySort::Date, "type" => SummarySort::Type, _ => SummarySort::Page },
                    font_size: size().parse().unwrap_or(10.0),
                    include_hidden: hidden(),
                    paper: (pw, ph),
                    labels: SummaryLabels {
                        title: t(lang, "dialog.summary.doc_title"),
                        page: t(lang, "status.page"),
                        no_comments: t(lang, "toast.no_comments"),
                        types,
                    },
                };
                match st.read_active(|t| t.doc.summarize_comments(&opts)) {
                    Some(Ok(doc)) => {
                        st.set_dialog(Dialog::None);
                        files::open_document_tab(st, doc);
                    }
                    Some(Err(e)) => st.error(e),
                    None => st.set_dialog(Dialog::None),
                }
            },
            div { class: "hint", "{tf(lang, \"dialog.summary.count\", &[(\"count\", &count.to_string())])}" }
            div { class: "field",
                label { "{t(lang, \"dialog.summary.layout\")}" }
                div { class: "radio-col",
                    for (value, key) in layouts {
                        label { key: "{value}", input { r#type: "radio", name: "summary-layout", checked: layout() == value, onchange: move |_| layout.set(value.to_string()) } "{t(lang, key)}" }
                    }
                }
            }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.summary.sort\")}" }
                    select { onchange: move |e| sort.set(e.value()),
                        for (value, key) in sorts { option { key: "{value}", value: "{value}", selected: sort() == value, "{t(lang, key)}" } }
                    }
                }
                div { class: "field", label { "{t(lang, \"dialog.summary.size\")}" }
                    select { onchange: move |e| size.set(e.value()),
                        for (value, key) in sizes { option { key: "{value}", value: "{value}", selected: size() == value, "{t(lang, key)}" } }
                    }
                }
                if layout() == "only" {
                    div { class: "field", label { "{t(lang, \"dialog.summary.paper\")}" }
                        select { onchange: move |e| paper.set(e.value()),
                            for (id, name, _, _) in PAPER.iter().copied() { option { key: "{id}", value: "{id}", selected: paper() == id, "{name}" } }
                        }
                    }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: hidden(), onchange: move |_| hidden.set(!hidden()) } span { "{t(lang, \"dialog.summary.hidden\")}" } }
        }
    }
}

#[component]
fn CalibrateDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let get = |k: &str, d: &str| st.ui.read().value(k).unwrap_or(d).to_string();
    let paper = use_signal(|| get("measure.paper", "1"));
    let real = use_signal(|| get("measure.real", "1"));
    let decimals = use_signal(|| get("measure.decimals", "1"));
    let mut paper_unit = use_signal(|| get("measure.paper_unit", "mm"));
    let mut real_unit = use_signal(|| get("measure.real_unit", "mm"));
    let preview = {
        let p: f32 = paper().replace(',', ".").parse().unwrap_or(1.0);
        let r: f32 = real().replace(',', ".").parse().unwrap_or(1.0);
        folitecta_core::MeasureScale::from_ratio(p, &paper_unit(), r, &real_unit(), 1).ratio
    };
    rsx! {
        Frame { title: t(lang, "dialog.calibrate.title"),
            on_ok: move |_| {
                st.set_value("measure.paper", paper());
                st.set_value("measure.paper_unit", paper_unit());
                st.set_value("measure.real", real());
                st.set_value("measure.real_unit", real_unit());
                st.set_value("measure.decimals", decimals());
                st.set_dialog(Dialog::None);
            },
            div { class: "hint", "{t(lang, \"dialog.calibrate.hint\")}" }
            div { class: "field-grid",
                Num { label: t(lang, "dialog.calibrate.paper"), value: paper, min: "0", step: "0.1" }
                div { class: "field", label { "{t(lang, \"dialog.calibrate.unit\")}" }
                    select { onchange: move |e| paper_unit.set(e.value()),
                        for u in SCALE_UNITS.iter().filter(|u| !["m", "km", "ft"].contains(u)) { option { key: "{u}", value: "{u}", selected: *u == paper_unit(), "{u}" } }
                    }
                }
                Num { label: t(lang, "dialog.calibrate.real"), value: real, min: "0", step: "0.1" }
                div { class: "field", label { "{t(lang, \"dialog.calibrate.unit\")}" }
                    select { onchange: move |e| real_unit.set(e.value()),
                        for u in SCALE_UNITS.iter() { option { key: "{u}", value: "{u}", selected: *u == real_unit(), "{u}" } }
                    }
                }
                Num { label: t(lang, "dialog.calibrate.decimals"), value: decimals, min: "0", max: "6" }
            }
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.calibrate.scale\")}" } span { class: "v", "{preview}" } }
        }
    }
}
