//! Signatures and initials (handwritten, typed or pictures) placed on pages,
//! and the filler texts of the signer's profile (name, date, e-mail...).

use std::sync::Mutex;

use base64::Engine;
use dioxus::prelude::ReadableExt;
use folitecta_core::SignatureMark;
use serde::{Deserialize, Serialize};

use crate::commands::{core_result, handlers, no_doc, select_tool};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind, Tool, app_data_dir};

/// A signature or initials kept in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedMark {
    pub id: u64,
    pub initials: bool,
    /// "strokes", "typed" or "image".
    pub kind: String,
    #[serde(default)]
    pub strokes: Vec<Vec<(f32, f32)>>,
    #[serde(default)]
    pub text: String,
    /// PNG or JPEG, base64.
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub color: [f32; 3],
}

impl SavedMark {
    pub fn to_mark(&self) -> SignatureMark {
        match self.kind.as_str() {
            "typed" => SignatureMark::Typed {
                text: self.text.clone(),
                color: self.color,
            },
            "image" => SignatureMark::Image(
                base64::engine::general_purpose::STANDARD
                    .decode(&self.image)
                    .unwrap_or_default(),
            ),
            _ => SignatureMark::Strokes {
                strokes: self.strokes.clone(),
                color: self.color,
                width: 0.035,
            },
        }
    }
}

fn library_path() -> Option<std::path::PathBuf> {
    app_data_dir().map(|d| d.join("signatures.json"))
}

/// The library of signatures and initials.
pub fn library() -> Vec<SavedMark> {
    library_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_library(marks: &[SavedMark]) -> Result<(), String> {
    let path = library_path().ok_or("no data folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(marks).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Add a mark to the library and return it.
pub fn add_to_library(mut mark: SavedMark) -> Result<SavedMark, String> {
    let mut all = library();
    mark.id = all.iter().map(|m| m.id).max().unwrap_or(0) + 1;
    all.push(mark.clone());
    save_library(&all)?;
    Ok(mark)
}

pub fn remove_from_library(id: u64) {
    let all: Vec<SavedMark> = library().into_iter().filter(|m| m.id != id).collect();
    let _ = save_library(&all);
}

/// What the placing tools put on the page.
#[derive(Debug, Clone, PartialEq)]
enum Pending {
    Mark(SavedMark),
    Text(String),
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

/// Place a mark of the library: click for the natural size, or drag a rectangle.
pub fn start_placing(st: AppState, mark: SavedMark) {
    if no_doc(st) {
        return;
    }
    *PENDING.lock().expect("pending mark") = Some(Pending::Mark(mark));
    st.set_tool(Tool::PlaceSignature);
    st.toast(ToastKind::Info, t(st.lang(), "toast.place_signature"), "");
}

/// Place a text of the profile with a click.
fn start_text(st: AppState, text: String) {
    if no_doc(st) {
        return;
    }
    *PENDING.lock().expect("pending mark") = Some(Pending::Text(text));
    st.set_tool(Tool::PlaceSignature);
    st.toast(ToastKind::Info, t(st.lang(), "toast.place_text"), "");
}

/// The user clicked (`tiny`) or dragged `rect` (PDF space) on `page`.
pub fn place_at(st: AppState, page: usize, rect: [f32; 4], tiny: bool) {
    let pending = PENDING.lock().expect("pending mark").clone();
    let author = st.ui.peek().author.clone();
    let keep = crate::commands::option_on(st, "tool.keepSelected");
    let r = match pending {
        Some(Pending::Mark(m)) => {
            let mark = m.to_mark();
            let rect = if tiny {
                // Natural size: 36 pt high signatures, 24 pt high initials, from the click.
                let h = if m.initials { 24.0 } else { 36.0 };
                let w = h * mark.aspect();
                [rect[0], rect[3] - h, rect[0] + w, rect[3]]
            } else {
                rect
            };
            st.with_active(|t| t.doc.add_signature_mark(page, rect, &mark, &author).map(|_| ()))
        }
        Some(Pending::Text(text)) => {
            let w = folitecta_core::fonts::text_width(&text, 12.0) + 12.0;
            let r = [rect[0], rect[3] - 18.0, rect[0] + w, rect[3]];
            st.with_active(|t| {
                t.doc
                    .add_free_text(page, r, &text, &author, 12.0, [0.0, 0.0, 0.0], false)
                    .map(|_| ())
            })
        }
        None => None,
    };
    core_result(st, r);
    if !keep {
        PENDING.lock().expect("pending mark").take();
        st.set_tool(Tool::Hand);
    }
}

/// Profile value for a filler text (empty when not set).
pub fn profile_value(st: AppState, key: &str) -> String {
    match key {
        "name" => st.ui.peek().author.clone(),
        "date" => folitecta_core::util::today_display(),
        other => st
            .ui
            .peek()
            .value(&format!("profile.{other}"))
            .unwrap_or_default()
            .to_string(),
    }
}

fn place_profile(st: AppState, key: &str) {
    let v = profile_value(st, key);
    if v.trim().is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.profile_missing"), "");
        st.set_dialog(Dialog::SignatureLibrary);
        return;
    }
    start_text(st, v);
}

/// Use the first signature (or initials) of the library.
fn apply_signature(st: AppState, initials: bool) {
    match library().into_iter().find(|m| m.initials == initials) {
        Some(m) => start_placing(st, m),
        None => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.no_saved_signature"), "");
            st.set_dialog(Dialog::SignatureLibrary);
        }
    }
}

fn apply_all(st: AppState) {
    if no_doc(st) {
        return;
    }
    match st.with_active(|t| t.doc.apply_signature_marks()) {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.signatures_applied",
                    &[("count", &n.to_string())],
                ),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn cancel_all(st: AppState) {
    if no_doc(st) {
        return;
    }
    match st.with_active(|t| t.doc.remove_signature_marks()) {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.signatures_removed",
                    &[("count", &n.to_string())],
                ),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn export_library(st: AppState) {
    let json = serde_json::to_string_pretty(&library()).unwrap_or_default();
    crate::files::save_text_file(st, "signatures.json", json);
}

fn import_library(st: AppState) {
    dioxus::prelude::spawn(async move {
        let Some(f) = rfd::AsyncFileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
            .await
        else {
            return;
        };
        let data = f.read().await;
        match serde_json::from_slice::<Vec<SavedMark>>(&data) {
            Ok(marks) => {
                let n = marks.len();
                for m in marks {
                    let _ = add_to_library(m);
                }
                st.toast(
                    ToastKind::Ok,
                    tf(
                        st.lang(),
                        "toast.signatures_imported",
                        &[("count", &n.to_string())],
                    ),
                    "",
                );
            }
            Err(e) => st.error(e),
        }
    });
}

/// Stop placing (when another tool is chosen).
pub fn cancel_pending() {
    PENDING.lock().expect("pending mark").take();
}

handlers! {
    "tool.sign.and.initials" => |st| st.set_dialog(Dialog::SignatureLibrary),
    "document.signatures.placesign" => |st| st.set_dialog(Dialog::SignatureLibrary),
    "signs.manage" => |st| st.set_dialog(Dialog::SignatureLibrary),
    "signs.add" => |st| st.set_dialog(Dialog::SignatureLibrary),
    "signs.edit" => |st| st.set_dialog(Dialog::SignatureLibrary),
    "signs.remove" => |st| st.set_dialog(Dialog::SignatureLibrary),
    "signMarkup.popup.createNew" => |st| st.set_dialog(Dialog::SignatureLibrary),
    "signMarkup.popup.fromList" => |st| st.set_dialog(Dialog::SignatureLibrary),
    "signs.applySignature" => |st| apply_signature(st, false),
    "signs.markup.signature" => |st| apply_signature(st, false),
    "signs.markup.initials" => |st| {
        if library().iter().any(|m| m.initials) { apply_signature(st, true) } else { place_profile(st, "initials") }
    },
    "signs.markup.name" => |st| place_profile(st, "name"),
    "signs.markup.date" => |st| place_profile(st, "date"),
    "signs.markup.email" => |st| place_profile(st, "email"),
    "signs.markup.company" => |st| place_profile(st, "company"),
    "signs.applyAll" => apply_all,
    "signs.cancelAll" => cancel_all,
    "signs.export" => export_library,
    "signs.import" => import_library,
    "tool.freeSign" => |st| select_tool(st, Tool::Ink),
    "tool.signMarkup" => |st| select_tool(st, Tool::FieldSignature),
}
