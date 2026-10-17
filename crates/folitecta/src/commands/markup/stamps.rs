//! Stamp library (collections, favorites, recents), placing stamps, and the
//! stamp designer.

use std::sync::Mutex;

use base64::Engine;
use dioxus::prelude::*;
use folitecta_core::STAMP_PRESETS;
use folitecta_core::stamp_design::{StampDesign, StampElement, StampValues};
use serde::{Deserialize, Serialize};

use crate::commands::{handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, Pane, ToastKind, Tool, app_data_dir};

/// Ids of the standard stamps (not stored in the library).
const STANDARD_BASE: u64 = 1_000_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StampItem {
    pub id: u64,
    pub name: String,
    /// `text`, `image` or `design`.
    pub kind: String,
    pub text: String,
    pub color: [f32; 3],
    /// PNG or JPEG, base64.
    pub image: String,
    pub design: Option<StampDesign>,
}

impl StampItem {
    pub fn is_standard(&self) -> bool {
        self.id >= STANDARD_BASE
    }

    /// An SVG preview of the stamp.
    pub fn preview_svg(&self) -> Option<String> {
        match self.kind.as_str() {
            "design" => self.design.as_ref().map(|d| d.svg(&preview_values())),
            "text" => Some(text_design(&self.text, self.color).svg(&preview_values())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StampCollection {
    pub name: String,
    pub stamps: Vec<StampItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StampLibrary {
    pub collections: Vec<StampCollection>,
    pub favorites: Vec<u64>,
    pub recents: Vec<u64>,
    pub next_id: u64,
    /// Last number given by counter fields.
    pub counter: i64,
}

fn library_path() -> Option<std::path::PathBuf> {
    app_data_dir().map(|d| d.join("stamps.json"))
}

pub fn library() -> StampLibrary {
    library_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_library(lib: &StampLibrary) {
    if let Some(p) = library_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(lib) {
            let _ = std::fs::write(p, json);
        }
    }
}

/// Bumped on every change so the panes redraw.
pub static LIBRARY_REV: GlobalSignal<u64> = Signal::global(|| 0);

fn changed(lib: &StampLibrary) {
    save_library(lib);
    *LIBRARY_REV.write() += 1;
}

/// The standard stamps.
pub fn standard() -> Vec<StampItem> {
    STAMP_PRESETS
        .iter()
        .enumerate()
        .map(|(i, p)| StampItem {
            id: STANDARD_BASE + i as u64,
            name: p.to_string(),
            kind: "text".into(),
            text: p.to_string(),
            color: if matches!(*p, "APPROVED" | "FINAL" | "REVIEWED" | "RECEIVED") {
                [0.08, 0.5, 0.2]
            } else {
                [0.78, 0.12, 0.12]
            },
            ..Default::default()
        })
        .collect()
}

/// Every stamp: standard ones and the library's.
pub fn all_stamps(lib: &StampLibrary) -> Vec<StampItem> {
    let mut v = standard();
    for c in &lib.collections {
        v.extend(c.stamps.iter().cloned());
    }
    v
}

pub fn find(lib: &StampLibrary, id: u64) -> Option<StampItem> {
    all_stamps(lib).into_iter().find(|s| s.id == id)
}

/// A text stamp as a design (rounded frame and the text).
pub fn text_design(text: &str, color: [f32; 3]) -> StampDesign {
    let size = 22.0;
    let w = (folitecta_core::util::helvetica_width(text, size) * 1.06 + 30.0).max(90.0);
    let h = 44.0;
    StampDesign {
        width: w,
        height: h,
        background: None,
        elements: vec![
            StampElement {
                kind: "rect".into(),
                rect: [2.0, 2.0, w - 4.0, h - 4.0],
                color,
                line_width: 2.5,
                radius: 6.0,
                ..Default::default()
            },
            StampElement {
                kind: "text".into(),
                text: text.into(),
                rect: [6.0, 9.0, w - 12.0, size],
                color,
                font_size: size,
                ..Default::default()
            },
        ],
    }
}

/// Field values shown in previews.
pub fn preview_values() -> StampValues {
    StampValues {
        name: "Name".into(),
        email: "name@example.org".into(),
        corporation: "Company".into(),
        login: "user".into(),
        counter: 1,
        custom: Default::default(),
        now: (2026, 1, 1, 9, 30, 0),
    }
}

/// Field values when a stamp is placed now.
pub fn current_values(
    st: AppState,
    counter: i64,
    custom: std::collections::HashMap<String, String>,
) -> StampValues {
    let p = |k: &str| crate::commands::signs::profile_value(st, k);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = folitecta_core::util::unix_display(now);
    // "YYYY-MM-DD HH:MM"
    let num = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    StampValues {
        name: p("name"),
        email: p("email"),
        corporation: p("company"),
        login: std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_default(),
        counter,
        custom,
        now: (
            num(0, 4) as i32,
            num(5, 7) as u8,
            num(8, 10) as u8,
            num(11, 13) as u8,
            num(14, 16) as u8,
            0,
        ),
    }
}

/// The stamp waiting to be placed, with the values of its custom fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PendingStamp {
    pub item: StampItem,
    pub custom: std::collections::HashMap<String, String>,
}

static PENDING: Mutex<Option<PendingStamp>> = Mutex::new(None);

pub fn pending() -> Option<PendingStamp> {
    PENDING.lock().expect("pending stamp").clone()
}

/// Forget the armed library stamp (another stamp was chosen).
pub fn clear_pending() {
    PENDING.lock().expect("pending stamp").take();
}

/// Arm the stamp tool with a stamp (asking the custom fields first).
pub fn use_stamp(st: AppState, item: StampItem) {
    if no_doc(st) {
        return;
    }
    let needs = item
        .design
        .as_ref()
        .map(|d| !d.custom_fields().is_empty())
        .unwrap_or(false);
    if needs && item.kind == "design" {
        st.set_dialog(Dialog::StampValues(item.id));
        return;
    }
    arm(
        st,
        PendingStamp {
            item,
            custom: Default::default(),
        },
    );
}

pub fn arm(st: AppState, p: PendingStamp) {
    match p.item.kind.as_str() {
        "image" => {
            let data = base64::engine::general_purpose::STANDARD
                .decode(&p.item.image)
                .unwrap_or_default();
            st.set_value("stamp.kind", "image");
            st.set_pending_image(Some(data));
        }
        "text" => {
            st.set_value("stamp.kind", "text");
            st.set_value("stamp.text", p.item.text.clone());
            st.set_value("stamp.color", crate::util::color_hex(p.item.color));
        }
        _ => st.set_value("stamp.kind", "design"),
    }
    *PENDING.lock().expect("pending stamp") = Some(p);
    st.set_tool(Tool::Stamp);
    st.toast(ToastKind::Info, t(st.lang(), "toast.place_stamp"), "");
}

/// Note a placed stamp: recents and counters.
pub fn placed(item_id: u64) {
    let mut lib = library();
    lib.recents.retain(|r| *r != item_id);
    lib.recents.insert(0, item_id);
    lib.recents.truncate(12);
    changed(&lib);
}

/// Next value of counter fields (kept in the library).
pub fn next_counter() -> i64 {
    let mut lib = library();
    lib.counter += 1;
    let v = lib.counter;
    changed(&lib);
    v
}

/// Place the armed designed stamp at `rect` (PDF space) or, for a click,
/// at its natural size centred on `(x, y)`.
pub fn place_design(
    st: AppState,
    page: usize,
    rect: [f32; 4],
    tiny: bool,
    at: (f32, f32),
) -> Option<folitecta_core::Result<()>> {
    let p = pending()?;
    let design = match p.item.kind.as_str() {
        "design" => p.item.design.clone()?,
        _ => return None,
    };
    let r = if tiny {
        let (w, h) = (design.width, design.height);
        [at.0 - w / 2.0, at.1 - h / 2.0, at.0 + w / 2.0, at.1 + h / 2.0]
    } else {
        rect
    };
    let counter = if design.has_counter() {
        next_counter()
    } else {
        library().counter
    };
    let values = current_values(st, counter, p.custom.clone());
    let author = st.ui.peek().author.clone();
    let r = st.with_active(|t| {
        t.doc
            .add_designed_stamp(page, r, &design, &values, &author, &p.item.name)
            .map(|_| ())
    });
    if matches!(r, Some(Ok(()))) {
        placed(p.item.id);
    }
    r
}

/// Selected stamp of the pane: (collection index or None for virtual groups, id).
pub static SELECTED: GlobalSignal<Option<u64>> = Signal::global(|| None);
/// Selected collection (index in the library).
pub static SELECTED_COLLECTION: GlobalSignal<Option<usize>> = Signal::global(|| None);

fn selected_item() -> Option<StampItem> {
    let id = (*SELECTED.peek())?;
    find(&library(), id)
}

fn need_selected(st: AppState) -> Option<StampItem> {
    let r = selected_item();
    if r.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.stamp_select"), "");
    }
    r
}

/// Collection where new stamps go: the selected one, or "Custom".
fn target_collection(st: AppState, lib: &mut StampLibrary) -> usize {
    if let Some(i) = (*SELECTED_COLLECTION.peek()).filter(|i| *i < lib.collections.len()) {
        return i;
    }
    let name = t(st.lang(), "stamps.custom");
    match lib.collections.iter().position(|c| c.name == name) {
        Some(i) => i,
        None => {
            lib.collections.push(StampCollection {
                name,
                stamps: Vec::new(),
            });
            lib.collections.len() - 1
        }
    }
}

/// Add a stamp to the library and return its id.
pub fn add_stamp(st: AppState, mut item: StampItem) -> u64 {
    let mut lib = library();
    lib.next_id += 1;
    item.id = lib.next_id;
    let c = target_collection(st, &mut lib);
    lib.collections[c].stamps.push(item.clone());
    changed(&lib);
    *SELECTED.write() = Some(item.id);
    item.id
}

/// Replace a stamp of the library (same id).
pub fn update_stamp(item: StampItem) {
    let mut lib = library();
    for c in &mut lib.collections {
        for s in &mut c.stamps {
            if s.id == item.id {
                *s = item.clone();
            }
        }
    }
    changed(&lib);
}

fn add_from_clipboard(st: AppState) {
    match crate::clipboard::get_png() {
        Some(png) => {
            let n = library()
                .collections
                .iter()
                .map(|c| c.stamps.len())
                .sum::<usize>()
                + 1;
            add_stamp(
                st,
                StampItem {
                    name: tf(st.lang(), "stamps.picture_name", &[("n", &n.to_string())]),
                    kind: "image".into(),
                    image: base64::engine::general_purpose::STANDARD.encode(png),
                    ..Default::default()
                },
            );
            crate::commands::show_pane(st, Pane::Stamps);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_no_image"), ""),
    }
}

fn duplicate(st: AppState) {
    let Some(mut item) = need_selected(st) else { return };
    item.name = tf(st.lang(), "stamps.copy_of", &[("name", &item.name)]);
    add_stamp(st, item);
}

pub fn remove_selected(st: AppState) {
    let Some(item) = need_selected(st) else { return };
    if item.is_standard() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.stamp_standard"), "");
        return;
    }
    let mut lib = library();
    for c in &mut lib.collections {
        c.stamps.retain(|s| s.id != item.id);
    }
    lib.favorites.retain(|f| *f != item.id);
    lib.recents.retain(|f| *f != item.id);
    changed(&lib);
    *SELECTED.write() = None;
}

fn edit_selected(st: AppState) {
    let Some(item) = need_selected(st) else { return };
    if item.is_standard() {
        // Standard stamps are edited as a copy.
        let mut copy = item.clone();
        copy.kind = "design".into();
        copy.design = Some(text_design(&item.text, item.color));
        open_designer(st, Some(copy), true);
        return;
    }
    match item.kind.as_str() {
        "design" => open_designer(st, Some(item), false),
        "text" => st.set_dialog(Dialog::StampText(Some(item.id))),
        _ => st.set_dialog(Dialog::StampText(Some(item.id))),
    }
}

fn sort_collection(st: AppState) {
    let mut lib = library();
    let Some(i) = (*SELECTED_COLLECTION.peek()).filter(|i| *i < lib.collections.len()) else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.stamp_collection"), "");
        return;
    };
    lib.collections[i].stamps.sort_by_key(|s| s.name.to_lowercase());
    changed(&lib);
}

fn favorites(st: AppState, op: &str) {
    let mut lib = library();
    match op {
        "clear" => lib.favorites.clear(),
        _ => {
            let Some(item) = need_selected(st) else { return };
            lib.favorites.retain(|f| *f != item.id);
            if op == "add" {
                lib.favorites.push(item.id);
            }
        }
    }
    changed(&lib);
}

fn recents(st: AppState, op: &str) {
    let mut lib = library();
    match op {
        "clear" => lib.recents.clear(),
        _ => {
            let Some(item) = need_selected(st) else { return };
            lib.recents.retain(|f| *f != item.id);
        }
    }
    changed(&lib);
}

fn zoom(st: AppState, factor: f32) {
    let cur: f32 = st
        .ui
        .peek()
        .value("stamps.size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(120.0);
    st.set_value(
        "stamps.size",
        format!("{}", (cur * factor).clamp(70.0, 260.0).round()),
    );
}

// ---- Designer ------------------------------------------------------------------

/// What the stamp designer is editing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DesignerState {
    pub design: StampDesign,
    pub name: String,
    /// Stamp of the library being edited (`None`: a new stamp).
    pub item: Option<u64>,
    pub selected: Option<usize>,
    /// Preview scale.
    pub zoom: f32,
    pub templates: bool,
    pub formats: bool,
}

pub static DESIGNER: GlobalSignal<DesignerState> = Signal::global(DesignerState::default);

/// Open the designer on a stamp (`copy`: save it as a new stamp).
pub fn open_designer(st: AppState, item: Option<StampItem>, copy: bool) {
    let (design, name, id) = match item {
        Some(i) => (
            i.design.clone().unwrap_or_else(|| text_design(&i.text, i.color)),
            i.name.clone(),
            (!copy).then_some(i.id),
        ),
        None => (StampDesign::default(), t(st.lang(), "stamps.new_name"), None),
    };
    *DESIGNER.write() = DesignerState {
        design,
        name,
        item: id,
        selected: None,
        zoom: 0.0,
        templates: false,
        formats: false,
    };
    st.set_dialog(Dialog::StampDesigner);
}

fn designer_open(st: AppState) -> bool {
    if (st.dialog)() != Dialog::StampDesigner {
        open_designer(st, None, false);
    }
    true
}

/// Add an element to the design and select it.
pub fn add_element(st: AppState, mut el: StampElement) {
    designer_open(st);
    let mut d = DESIGNER.write();
    if el.rect == StampElement::default().rect {
        let n = StampElement::new(&el.kind, d.design.width, d.design.height);
        el.rect = n.rect;
        if el.points.is_empty() {
            el.points = n.points;
        }
    }
    d.design.elements.push(el);
    d.selected = Some(d.design.elements.len() - 1);
}

fn add_kind(st: AppState, kind: &str) {
    let (w, h) = {
        let d = DESIGNER.peek();
        (d.design.width, d.design.height)
    };
    add_element(st, StampElement::new(kind, w, h));
}

fn add_field(st: AppState, field: &str) {
    let (w, h) = {
        let d = DESIGNER.peek();
        (d.design.width, d.design.height)
    };
    let mut el = StampElement::new("field", w, h);
    el.field = field.into();
    el.format = match field {
        "date" => "YYYY-MM-DD".into(),
        "counter" => "4".into(),
        _ => String::new(),
    };
    el.text = match field {
        "custom" => t(st.lang(), "stamps.custom_label"),
        "macros" => "{page}/{pages}".into(),
        _ => String::new(),
    };
    add_element(st, el);
}

fn add_file(st: AppState) {
    designer_open(st);
    spawn(async move {
        let Some(f) = rfd::AsyncFileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file()
            .await
        else {
            return;
        };
        let data = f.read().await;
        let (w, h) = {
            let d = DESIGNER.peek();
            (d.design.width, d.design.height)
        };
        let (iw, ih) = folitecta_core::Document::image_size(&data)
            .map(|(a, b)| (a as f32, b as f32))
            .unwrap_or((100.0, 50.0));
        let s = ((w * 0.8) / iw).min((h * 0.8) / ih);
        let el = StampElement {
            kind: "image".into(),
            rect: [(w - iw * s) / 2.0, (h - ih * s) / 2.0, iw * s, ih * s],
            image: base64::engine::general_purpose::STANDARD.encode(data),
            ..Default::default()
        };
        add_element(st, el);
    });
}

fn designer_op(st: AppState, op: &str) {
    designer_open(st);
    let mut d = DESIGNER.write();
    match op {
        "clear" => {
            d.design.elements.clear();
            d.selected = None;
        }
        "crop" => d.design.crop_to_content(),
        "remove" => {
            if let Some(i) = d.selected.filter(|i| *i < d.design.elements.len()) {
                d.design.elements.remove(i);
                d.selected = None;
            }
        }
        "bg" => {
            d.design.background = match d.design.background {
                Some(_) => None,
                None => Some([1.0, 1.0, 1.0]),
            }
        }
        "templates" => d.templates = !d.templates,
        "formats" => d.formats = !d.formats,
        "fit" => d.zoom = 0.0,
        "new" => {
            d.design = StampDesign::default();
            d.item = None;
            d.selected = None;
        }
        _ => {}
    }
}

/// Save the design to the library (`as_new`: as another stamp). Returns its id.
pub fn save_design(st: AppState, as_new: bool) -> u64 {
    let d = DESIGNER.peek().clone();
    let item = StampItem {
        id: d.item.unwrap_or(0),
        name: if d.name.trim().is_empty() {
            t(st.lang(), "stamps.new_name")
        } else {
            d.name.trim().to_string()
        },
        kind: "design".into(),
        design: Some(d.design.clone()),
        color: d
            .design
            .elements
            .first()
            .map(|e| e.color)
            .unwrap_or([0.0, 0.0, 0.0]),
        ..Default::default()
    };
    let id = match d.item {
        Some(id) if !as_new => {
            update_stamp(item);
            id
        }
        _ => add_stamp(st, item),
    };
    DESIGNER.write().item = Some(id);
    st.toast(ToastKind::Ok, t(st.lang(), "toast.stamp_saved"), "");
    id
}

/// Select the stamps on the page made with the designer's stamp.
fn select_annots(st: AppState) {
    if no_doc(st) {
        return;
    }
    let name = DESIGNER.peek().name.clone();
    let clean: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let target = format!("FolitectaDesign{clean}");
    let found = st
        .read_active(|t| {
            let doc = t.doc.inner();
            let mut out = Vec::new();
            for p in 0..t.doc.page_count() {
                for a in t.doc.annotations(p).unwrap_or_default() {
                    let is =
                        a.id.and_then(|id| doc.get_dictionary(id).ok())
                            .and_then(|d| d.get(b"Name").ok())
                            .and_then(|n| n.as_name().ok())
                            == Some(target.as_bytes());
                    if is {
                        out.push((p, a.index));
                    }
                }
            }
            out
        })
        .unwrap_or_default();
    match found.first() {
        Some(&(p, i)) => {
            st.set_dialog(Dialog::None);
            st.with_active(|t| t.selected_annot = Some((p, i)));
            st.navigate(p, true);
            st.toast(
                ToastKind::Info,
                tf(
                    st.lang(),
                    "toast.layer_annots",
                    &[("count", &found.len().to_string())],
                ),
                "",
            );
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.layer_annots_none"), ""),
    }
}

fn focus_field(st: AppState, id: &'static str) {
    designer_open(st);
    spawn(async move {
        let _ = document::eval(&format!(
            "for (let i = 0; i < 20 && !document.getElementById('{id}'); i++) await new Promise(r => setTimeout(r, 30)); const e = document.getElementById('{id}'); if (e) {{ e.focus(); e.select && e.select(); }}"
        ))
        .await;
    });
}

/// Check states of the stamp commands.
pub fn stamps_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "view.stamps" => Some(st.ui.peek().show_left_pane && st.ui.peek().left_pane == "stamps"),
        _ => None,
    }
}

handlers! {
    "view.stamps" => |st| crate::commands::toggle_pane(st, Pane::Stamps),
    "stampsView.addCustom" => |st| st.set_dialog(Dialog::StampText(None)),
    "stampsView.addGroup" => |st| st.set_dialog(Dialog::StampGroup(None)),
    "stampsView.addNew" => |st| open_designer(st, None, false),
    "stampsView.addNewFromCb" => add_from_clipboard,
    "stampsView.options" => |st| st.set_dialog(Dialog::StampOptions),
    "stampsView.sel.collection.sort" => sort_collection,
    "stampsView.sel.stamp.duplicate" => duplicate,
    "stampsView.selection.edit" => edit_selected,
    "stampsView.selection.recolor" => |st| { if let Some(i) = need_selected(st) { st.set_dialog(Dialog::StampRecolor(i.id)) } },
    "stampsView.virtual.favorites.add" => |st| favorites(st, "add"),
    "stampsView.virtual.favorites.remove" => |st| favorites(st, "remove"),
    "stampsView.virtual.favorites.clear" => |st| favorites(st, "clear"),
    "stampsView.virtual.recents.remove" => |st| recents(st, "remove"),
    "stampsView.virtual.recents.clear" => |st| recents(st, "clear"),
    "stampsView.zoomIn" => |st| zoom(st, 1.2),
    "stampsView.zoomOut" => |st| zoom(st, 1.0 / 1.2),
    "stampCustom.new" => |st| open_designer(st, None, false),
    "stampCustom.edit" => edit_selected,
    "stampCustom.remove" => |st| designer_op(st, "remove"),
    "stampCustom.clear" => |st| designer_op(st, "clear"),
    "stampCustom.cropWhiteMargins" => |st| designer_op(st, "crop"),
    "stampCustom.pageBgColor" => |st| designer_op(st, "bg"),
    "stampCustom.pageWidth" => |st| focus_field(st, "stamp-width"),
    "stampCustom.pageHeight" => |st| focus_field(st, "stamp-height"),
    "stampCustom.openFromTemplate" => |st| designer_op(st, "templates"),
    "stampCustom.add.template" => |st| designer_op(st, "templates"),
    "stampCustom.manage.dateFormats" => |st| designer_op(st, "formats"),
    "stampCustom.zoom.level.fit" => |st| designer_op(st, "fit"),
    "stampCustom.save" => |st| { designer_open(st); save_design(st, false); },
    "stampCustom.saveAs" => |st| { designer_open(st); save_design(st, true); },
    "stampCustom.addStamp" => |st| {
        designer_open(st);
        let id = save_design(st, false);
        if let Some(item) = find(&library(), id) {
            st.set_dialog(Dialog::None);
            use_stamp(st, item);
        }
    },
    "stampCustom.selectAnnots" => select_annots,
    "stampCustom.menu" => |st| crate::commands::show_pane(st, Pane::Stamps),
    "stampCustom.recentsMenu" => |st| crate::commands::show_pane(st, Pane::Stamps),
    "stampCustom.add.text" => |st| add_kind(st, "text"),
    "stampCustom.add.rect" => |st| add_kind(st, "rect"),
    "stampCustom.add.oval" => |st| add_kind(st, "oval"),
    "stampCustom.add.line" => |st| add_kind(st, "line"),
    "stampCustom.add.polyline" => |st| add_kind(st, "polyline"),
    "stampCustom.add.polygon" => |st| add_kind(st, "polygon"),
    "stampCustom.add.curve" => |st| add_kind(st, "curve"),
    "stampCustom.add.file" => add_file,
    "stampCustom.add.field.date" => |st| add_field(st, "date"),
    "stampCustom.add.field.counter" => |st| add_field(st, "counter"),
    "stampCustom.add.field.custom" => |st| add_field(st, "custom"),
    "stampCustom.add.field.macros" => |st| add_field(st, "macros"),
    "stampCustom.add.field.identity.name" => |st| add_field(st, "name"),
    "stampCustom.add.field.identity.email" => |st| add_field(st, "email"),
    "stampCustom.add.field.identity.corporation" => |st| add_field(st, "corporation"),
    "stampCustom.add.field.identity.loginName" => |st| add_field(st, "login"),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_stamps_become_designs() {
        let d = text_design("PAID", [0.0, 0.5, 0.0]);
        assert_eq!(d.elements.len(), 2);
        assert!(d.width >= 90.0);
        assert!(d.svg(&preview_values()).contains(">PAID<"));
        assert_eq!(standard().len(), STAMP_PRESETS.len());
        assert!(standard().iter().all(|s| s.is_standard()));
    }
}
