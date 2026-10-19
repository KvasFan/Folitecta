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
    /// At the side of the window over a lighter backdrop, so the page under
    /// it stays in view (the dialog works on what the page shows).
    #[props(default = false)]
    side: bool,
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
    // A press inside the dialog, so the click that ends it (a drag released
    // over the backdrop) does not close the dialog.
    let mut pressed_inside = use_signal(|| false);
    let mut pressed_outside = use_signal(|| false);
    let dialog_class = match (wide, side) {
        (true, true) => "dialog wide side",
        (true, false) => "dialog wide",
        (false, true) => "dialog side",
        (false, false) => "dialog",
    };
    rsx! {
        div { class: if side { "backdrop side" } else { "backdrop" },
            onmousedown: move |_| {
                pressed_outside.set(!*pressed_inside.peek());
                pressed_inside.set(false);
            },
            onclick: move |_| {
                if *pressed_outside.peek() {
                    st.set_dialog(Dialog::None);
                }
            },
            div { class: "{dialog_class}", onclick: move |e| e.stop_propagation(),
                onmousedown: move |_| pressed_inside.set(true),
                onkeydown: move |e| {
                    e.stop_propagation();
                    if e.key() == Key::Enter {
                        // After the field being typed in takes its value. Enter on a
                        // button is that button's click (the browser makes it one):
                        // the dialog's OK must not run as well.
                        if let Some(h) = ok2 {
                            spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                                let on_button = document::eval("return document.activeElement?.tagName === 'BUTTON';")
                                    .await
                                    .ok()
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                if !on_button {
                                    h.call(());
                                }
                            });
                        }
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
        Dialog::SplitPages => rsx! { crate::ui::features::split::SplitPagesDialog {} },
        Dialog::Background => rsx! { Background {} },
        Dialog::PageLabels => rsx! { PageLabels {} },
        Dialog::Transitions => rsx! { Transitions {} },
        Dialog::ReplacePages(path) => rsx! { ReplacePages { path: path.display().to_string() } },
        Dialog::OverlayFile(path) => rsx! { OverlayFile { path: path.display().to_string() } },
        Dialog::Print(all) => rsx! { PrintDialog { all_docs: all } },
        Dialog::Callout { page, tip, rect } => {
            rsx! { CalloutDialog { page, tip_x: tip.0, tip_y: tip.1, rect: rect.to_vec() } }
        }
        // Keyed: another place or mode starts a new dialog, not the old one's state.
        Dialog::InsertText {
            page,
            index,
            after,
            note,
        } => rsx! { InsertTextDialog { key: "{page}-{index}-{after}-{note}", page, index, after, note } },
        Dialog::ReplaceText {
            page,
            start,
            end,
            note,
        } => rsx! { ReplaceTextDialog { key: "{page}-{start}-{end}-{note}", page, start, end, note } },
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
        } => rsx! { crate::ui::features::signatures::SignDialog { certify, invisible, field } },
        Dialog::SignatureDetails(field) => {
            rsx! { crate::ui::features::signatures::SignatureDetailsDialog { field } }
        }
        Dialog::TimeStamp => rsx! { crate::ui::features::signatures::TimeStampDialog {} },
        Dialog::DigitalIds => rsx! { crate::ui::features::signatures::DigitalIdsDialog {} },
        Dialog::TrustedCerts => rsx! { crate::ui::features::signatures::TrustedCertsDialog {} },
        Dialog::SignatureLibrary => rsx! { crate::ui::features::signs::SignatureLibraryDialog {} },
        Dialog::MacroGuide => rsx! { crate::ui::features::macros::MacroGuideDialog {} },
        Dialog::LaunchApp => rsx! { crate::ui::features::macros::LaunchAppDialog {} },
        Dialog::LayerName { group, rename } => {
            rsx! { crate::ui::panes::layers::LayerNameDialog { group, rename } }
        }
        Dialog::LayerOverrides => rsx! { crate::ui::panes::layers::LayerOverridesDialog {} },
        Dialog::LayerMerge => rsx! { crate::ui::panes::layers::LayerMergeDialog {} },
        Dialog::ImportLayer(path) => {
            rsx! { crate::ui::panes::layers::ImportLayerDialog { path: path.display().to_string() } }
        }
        Dialog::LayerOptions => rsx! { crate::ui::panes::layers::LayerOptionsDialog {} },
        Dialog::AttachmentEdit(index) => {
            rsx! { crate::ui::panes::attachments::AttachmentEditDialog { index } }
        }
        Dialog::AttachmentOptions => rsx! { crate::ui::panes::attachments::AttachmentOptionsDialog {} },
        Dialog::FieldsOptions => rsx! { crate::ui::panes::fields::FieldsOptionsDialog {} },
        Dialog::PortfolioFolder { id } => {
            rsx! { crate::ui::features::portfolio::PortfolioFolderDialog { id } }
        }
        Dialog::PortfolioItem(file) => rsx! { crate::ui::features::portfolio::PortfolioItemDialog { file } },
        Dialog::PortfolioProps => rsx! { crate::ui::features::portfolio::PortfolioPropsDialog {} },
        Dialog::SessionName(index) => rsx! { crate::ui::panes::history::SessionNameDialog { index } },
        Dialog::StampValues(id) => rsx! { crate::ui::features::stamps::StampValuesDialog { id } },
        Dialog::StampText(id) => rsx! { crate::ui::features::stamps::StampTextDialog { id } },
        Dialog::StampGroup(index) => rsx! { crate::ui::features::stamps::StampGroupDialog { index } },
        Dialog::StampOptions => rsx! { crate::ui::features::stamps::StampOptionsDialog {} },
        Dialog::StampRecolor(id) => rsx! { crate::ui::features::stamps::StampRecolorDialog { id } },
        Dialog::StampDesigner => rsx! { crate::ui::features::stamps::StampDesignerDialog {} },
        Dialog::ManageMarks(kind) => rsx! { crate::ui::features::doctools::ManageMarksDialog { kind } },
        Dialog::SpaceAudit => rsx! { crate::ui::features::doctools::SpaceAuditDialog {} },
        Dialog::Recompress(current) => rsx! { crate::ui::features::doctools::RecompressDialog { current } },
        Dialog::InsertTextPages => rsx! { crate::ui::features::doctools::InsertTextPagesDialog {} },
        Dialog::MultiPlace => rsx! { crate::ui::features::doctools::MultiPlaceDialog {} },
        Dialog::DocumentActions => rsx! { crate::ui::features::doctools::DocumentActionsDialog {} },
        Dialog::Compare => rsx! { crate::ui::features::compare::CompareDialog {} },
        Dialog::TableNew { page, rect } => {
            rsx! { crate::ui::features::tables::TableNewDialog { page, rect: rect.to_vec() } }
        }
        Dialog::TableEditor => rsx! { crate::ui::features::tables::TableEditorDialog {} },
        Dialog::FieldCopies(field) => rsx! { crate::ui::features::tables::FieldCopiesDialog { field } },
        Dialog::TagNew { parent, index } => {
            rsx! { crate::ui::features::acc_dialogs::TagNewDialog { parent, index } }
        }
        Dialog::TagProps(id) => rsx! { crate::ui::features::acc_dialogs::TagPropsDialog { id } },
        Dialog::TagFromArea { page, rect } => {
            rsx! { crate::ui::features::acc_dialogs::TagFromAreaDialog { page, rect: rect.to_vec() } }
        }
        Dialog::RoleMap => rsx! { crate::ui::features::acc_dialogs::RoleMapDialog {} },
        Dialog::ClassMap => rsx! { crate::ui::features::acc_dialogs::ClassMapDialog {} },
        Dialog::AltText => rsx! { crate::ui::features::acc_dialogs::AltTextDialog {} },
        Dialog::AccOptions(pane) => rsx! { crate::ui::features::acc_dialogs::AccOptionsDialog { pane } },
        Dialog::RuleExplain(id) => rsx! { crate::ui::features::acc_dialogs::RuleExplainDialog { id } },
        Dialog::DocLanguage => rsx! { crate::ui::features::acc_dialogs::DocLanguageDialog {} },
        Dialog::CalcSettings => rsx! { crate::ui::features::calculator::CalcSettingsDialog {} },
        Dialog::RenameDoc(name) => rsx! { crate::ui::features::fileops::RenameDocDialog { name } },
        Dialog::MetaTemplates => rsx! { crate::ui::features::fileops::MetaTemplatesDialog {} },
        Dialog::MetaTemplateSave => rsx! { crate::ui::features::fileops::MetaTemplateSaveDialog {} },
        Dialog::JsConsole => rsx! { crate::ui::features::javascript::JsConsoleDialog {} },
        Dialog::JsPrefs => rsx! { crate::ui::features::javascript::JsPrefsDialog {} },
        Dialog::DocScripts(add) => rsx! { crate::ui::features::javascript::DocScriptsDialog { add } },
        Dialog::MediaList => rsx! { crate::ui::features::media::MediaListDialog {} },
        Dialog::MediaProps { page, index } => {
            rsx! { crate::ui::features::media::MediaPropsDialog { page, index } }
        }
        Dialog::NewFolder => rsx! { crate::ui::panes::explorer::NewFolderDialog {} },
        Dialog::FolderFilter(index) => rsx! { crate::ui::panes::explorer::FolderFilterDialog { index } },
        Dialog::Combine => rsx! { crate::ui::features::sources::CombineDialog {} },
        Dialog::ImageEditor => rsx! { crate::ui::features::pictures::ImageEditorDialog {} },
        Dialog::ExternalPicture => rsx! { crate::ui::features::pictures::ExternalPictureDialog {} },
        Dialog::PictureSettings => rsx! { crate::ui::features::pictures::PictureSettingsDialog {} },
        Dialog::StyleName(index) => rsx! { crate::ui::panes::comment_styles::StyleNameDialog { index } },
        Dialog::RedactionLook => rsx! { crate::ui::features::redaction::RedactionLookDialog {} },
        Dialog::RedactionCode { set } => rsx! { crate::ui::features::redaction::RedactionCodeDialog { set } },
        Dialog::ApplyRedactions { page } => {
            rsx! { crate::ui::features::redaction::ApplyRedactionsDialog { page } }
        }
        Dialog::FindReplace { find } => {
            rsx! { crate::ui::features::find_replace::FindReplaceDialog { find } }
        }
        Dialog::Recovery => rsx! { crate::ui::features::recovery::RecoveryDialog {} },
        Dialog::Barcode { page, rect } => {
            rsx! { crate::ui::features::barcodes::BarcodeDialog { page, rect: rect.to_vec() } }
        }
        Dialog::Barcodes(found) => rsx! { crate::ui::features::barcodes::BarcodesDialog { found } },
        Dialog::CalcOrder => rsx! { crate::ui::features::form_calc::CalcOrderDialog {} },
        Dialog::NoteIcon(target) => rsx! { NoteIconDialog { target } },
        Dialog::SpellCheck => rsx! { crate::ui::features::spelling::SpellCheckDialog {} },
        Dialog::Ocr { pictures_only } => rsx! { crate::ui::features::ocr::OcrDialog { pictures_only } },
        Dialog::OcrText { text } => rsx! { crate::ui::features::ocr::OcrTextDialog { text } },
        Dialog::FieldItems => rsx! { crate::ui::features::field_editor::FieldItemsDialog {} },
        Dialog::SecurityPolicies => rsx! { crate::ui::features::policies::SecurityPoliciesDialog {} },
        Dialog::CustomizeQat => rsx! { crate::ui::chrome::qat::CustomizeQatDialog {} },
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
