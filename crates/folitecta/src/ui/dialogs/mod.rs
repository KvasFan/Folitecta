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

mod app;
mod marks;
mod navigation;
mod pages;
mod review;
mod tools;

pub use app::*;
pub use marks::*;
pub use navigation::*;
pub use pages::*;
pub use review::*;
pub use tools::*;

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
