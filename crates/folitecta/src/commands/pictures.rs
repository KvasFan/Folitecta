//! Pictures on the pages: the tool that selects them, quick edits, the
//! picture editor and editing in another app.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use dioxus::prelude::*;
use folitecta_core::imaging::{Edit, Filter, Format, Work, skew_angle};
use folitecta_core::pictures::PageImage;

use super::{handlers, no_doc, option_on, select_tool};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, DocTab, ToastKind, Tool};

/// A picture open in the editor.
pub struct Session {
    pub tab: u64,
    pub page: usize,
    pub index: usize,
    pub work: Work,
    /// Earlier states, for undo inside the editor.
    pub undo: Vec<Work>,
    pub generation: u64,
}

/// The picture of the editor, shared with the thread that serves its preview.
pub static SESSION: LazyLock<Mutex<Option<Session>>> = LazyLock::new(|| Mutex::new(None));

/// What the editor shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditorView {
    pub generation: u64,
    pub size: (u32, u32),
    pub format: Option<Format>,
    /// Zoom of the preview (None: the picture fits the visible area).
    pub zoom: Option<f32>,
    /// Size of the visible area (CSS pixels), measured when the editor opens.
    pub stage: (f32, f32),
    pub cropping: bool,
    /// Area to keep, in pixels of the picture: x0, y0, x1, y1.
    pub crop: Option<[f32; 4]>,
    /// Open menu of the toolbar: "filters", "convert", "zoom".
    pub menu: Option<&'static str>,
    pub busy: bool,
    pub can_undo: bool,
}

pub static EDITOR: GlobalSignal<EditorView> = Signal::global(EditorView::default);

static GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::Relaxed)
}

/// Undo states kept by the editor, by memory.
const UNDO_BYTES: usize = 400 << 20;

/// Zoom levels of the editor.
pub const ZOOMS: &[f32] = &[0.1, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0];

fn area(im: &PageImage) -> f32 {
    (im.rect[2] - im.rect[0]) * (im.rect[3] - im.rect[1])
}

/// The picture selected with the pictures tool.
pub fn selected(st: AppState) -> Option<(usize, usize)> {
    st.read_active(|t| t.image_sel).flatten()
}

/// The selected picture, or the largest one of the current page.
fn target(st: AppState) -> Option<(usize, usize)> {
    if no_doc(st) {
        return None;
    }
    if let Some(s) = selected(st) {
        let exists = st
            .read_active(|t| t.doc.page_images(s.0).map(|l| s.1 < l.len()).unwrap_or(false))
            .unwrap_or(false);
        if exists {
            return Some(s);
        }
    }
    let page = st.current_page();
    let biggest = st
        .read_active(|t| {
            t.doc.page_images(page).ok().and_then(|l| {
                l.iter()
                    .max_by(|a, b| area(a).total_cmp(&area(b)))
                    .map(|im| im.index)
            })
        })
        .flatten();
    match biggest {
        Some(i) => {
            st.with_active(|t| t.image_sel = Some((page, i)));
            Some((page, i))
        }
        None => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.no_pictures"), "");
            None
        }
    }
}

fn with_tab<T>(st: AppState, id: u64, f: impl FnOnce(&mut DocTab) -> T) -> Option<T> {
    let mut docs = st.docs_mut();
    let tab = docs.iter_mut().find(|t| t.id == id)?;
    let out = f(tab);
    tab.refresh();
    Some(out)
}

/// The picture as a work to edit.
fn load(st: AppState, page: usize, index: usize) -> Option<(u64, Work)> {
    let loaded = st
        .read_active(|t| {
            let im = t.doc.page_images(page).ok()?.get(index).cloned()?;
            Some((t.id, im, t.doc.image_pixels(page, index)))
        })
        .flatten()?;
    match loaded {
        (tab, im, Ok(px)) => Some((tab, Work::new(px, im.matrix, im.colors))),
        (_, _, Err(e)) => {
            st.error(e);
            None
        }
    }
}

/// Put an edited picture back on its page.
fn store(st: AppState, tab: u64, page: usize, index: usize, work: &Work) {
    let everywhere = option_on(st, "editImage.applyToAll");
    let r = with_tab(st, tab, |t| {
        if everywhere {
            t.doc
                .replace_image_everywhere(page, index, &work.pixels, work.placement, work.colors)
                .map(|_| ())
        } else {
            t.doc
                .replace_page_image(page, index, &work.pixels, work.placement, work.colors)
        }
    });
    match r {
        Some(Err(e)) => st.error(e),
        Some(Ok(())) => {
            with_tab(st, tab, |t| t.image_sel = Some((page, index)));
            crate::files::set_window_title(st);
        }
        None => {}
    }
}

/// What an operation does to a picture.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Edit(Edit),
    /// Straighten by the angle found in the picture.
    Deskew,
}

/// Run an operation on a work; returns the angle straightened by (deskew).
fn run_op(work: &mut Work, op: Op) -> f32 {
    match op {
        Op::Edit(e) => {
            work.apply(e);
            0.0
        }
        Op::Deskew => {
            let a = skew_angle(&work.pixels);
            if a != 0.0 {
                work.apply(Edit::Turn(-a));
            }
            a
        }
    }
}

fn report_deskew(st: AppState, op: Op, angle: f32) {
    if op == Op::Deskew {
        let lang = st.lang();
        if angle == 0.0 {
            st.toast(ToastKind::Info, t(lang, "toast.picture_straight"), "");
        } else {
            st.toast(
                ToastKind::Ok,
                tf(
                    lang,
                    "toast.picture_straightened",
                    &[("angle", &format!("{:.1}", angle.abs()))],
                ),
                "",
            );
        }
    }
}

pub fn editor_open(st: AppState) -> bool {
    *st.dialog.peek() == Dialog::ImageEditor && SESSION.lock().map(|s| s.is_some()).unwrap_or(false)
}

/// Run an operation in the editor, or on the selected picture straight away.
fn operate(st: AppState, op: Op) {
    if editor_open(st) {
        edit_in_editor(st, op);
    } else {
        quick(st, op);
    }
}

fn quick(st: AppState, op: Op) {
    let Some((page, index)) = target(st) else { return };
    let Some((tab, mut work)) = load(st, page, index) else {
        return;
    };
    // Not tied to the toolbar that asked: it goes away with the selection.
    dioxus::core::spawn_forever(async move {
        let done = tokio::task::spawn_blocking(move || {
            let a = run_op(&mut work, op);
            (work, a)
        })
        .await;
        if let Ok((work, angle)) = done {
            if !(op == Op::Deskew && angle == 0.0) {
                store(st, tab, page, index, &work);
            }
            report_deskew(st, op, angle);
        }
    });
}

fn start_session(
    tab: u64,
    page: usize,
    index: usize,
    work: Work,
    cropping: bool,
    menu: Option<&'static str>,
) {
    let generation = next_generation();
    let (size, format) = (work.size(), work.format());
    if let Ok(mut s) = SESSION.lock() {
        *s = Some(Session {
            tab,
            page,
            index,
            work,
            undo: Vec::new(),
            generation,
        });
    }
    *EDITOR.write() = EditorView {
        generation,
        size,
        format: Some(format),
        cropping,
        menu,
        ..Default::default()
    };
}

/// Open the selected picture in the editor.
pub fn open_editor(st: AppState) {
    open_with(st, false, None);
}

fn open_with(st: AppState, cropping: bool, menu: Option<&'static str>) {
    if editor_open(st) {
        let mut v = EDITOR.write();
        v.cropping |= cropping;
        if menu.is_some() {
            v.menu = menu;
        }
        return;
    }
    let Some((page, index)) = target(st) else { return };
    let Some((tab, work)) = load(st, page, index) else {
        return;
    };
    start_session(tab, page, index, work, cropping, menu);
    st.set_dialog(Dialog::ImageEditor);
}

fn edit_in_editor(st: AppState, op: Op) {
    if EDITOR.peek().busy {
        return;
    }
    let taken = SESSION
        .lock()
        .ok()
        .and_then(|s| s.as_ref().map(|s| s.work.clone()));
    let Some(mut work) = taken else { return };
    EDITOR.write().busy = true;
    dioxus::core::spawn_forever(async move {
        let before = work.clone();
        let done = tokio::task::spawn_blocking(move || {
            let a = run_op(&mut work, op);
            (work, a)
        })
        .await;
        let Ok((work, angle)) = done else {
            EDITOR.write().busy = false;
            return;
        };
        report_deskew(st, op, angle);
        if op == Op::Deskew && angle == 0.0 {
            EDITOR.write().busy = false;
            return;
        }
        let generation = next_generation();
        let (size, format) = (work.size(), work.format());
        if let Ok(mut s) = SESSION.lock() {
            if let Some(s) = s.as_mut() {
                s.undo.push(before);
                // Keep the undo states within a memory budget.
                while s.undo.len() > 1 && s.undo.iter().map(|w| w.pixels.len()).sum::<usize>() > UNDO_BYTES {
                    s.undo.remove(0);
                }
                s.work = work;
                s.generation = generation;
            }
        }
        let mut v = EDITOR.write();
        v.busy = false;
        v.generation = generation;
        v.size = size;
        v.format = Some(format);
        v.can_undo = true;
        if matches!(op, Op::Edit(Edit::Crop(_))) {
            v.crop = None;
            v.cropping = false;
        }
    });
}

/// Undo the last edit made in the editor.
pub fn undo_in_editor() {
    let mut state = None;
    if let Ok(mut s) = SESSION.lock() {
        if let Some(s) = s.as_mut() {
            if let Some(prev) = s.undo.pop() {
                s.work = prev;
                s.generation = next_generation();
                state = Some((s.generation, s.work.size(), s.work.format(), !s.undo.is_empty()));
            }
        }
    }
    if let Some((generation, size, format, more)) = state {
        let mut v = EDITOR.write();
        v.generation = generation;
        v.size = size;
        v.format = Some(format);
        v.can_undo = more;
        v.crop = None;
    }
}

/// Put the edited picture on the page and close the editor.
pub fn apply_editor(st: AppState) {
    if EDITOR.peek().busy {
        return;
    }
    let taken = SESSION.lock().ok().and_then(|mut s| s.take());
    st.set_dialog(Dialog::None);
    *EDITOR.write() = EditorView::default();
    if let Some(s) = taken {
        if !s.undo.is_empty() {
            store(st, s.tab, s.page, s.index, &s.work);
        }
    }
}

/// Crop: start drawing the area, or keep the area drawn.
fn crop(st: AppState) {
    if editor_open(st) {
        let (cropping, area) = {
            let v = EDITOR.peek();
            (v.cropping, v.crop)
        };
        match area {
            Some(r) if cropping => {
                let px = [
                    r[0].min(r[2]).max(0.0) as u32,
                    r[1].min(r[3]).max(0.0) as u32,
                    r[0].max(r[2]).ceil() as u32,
                    r[1].max(r[3]).ceil() as u32,
                ];
                if px[2] > px[0] && px[3] > px[1] {
                    edit_in_editor(st, Op::Edit(Edit::Crop(px)));
                }
            }
            _ => {
                let mut v = EDITOR.write();
                v.cropping = !cropping;
                v.crop = None;
            }
        }
    } else {
        open_with(st, true, None);
    }
}

fn zoom_by(st: AppState, up: bool) {
    if !editor_open(st) {
        open_editor(st);
        return;
    }
    let current = fit_zoom_now();
    let next = if up {
        ZOOMS.iter().copied().find(|z| *z > current + 1e-3).unwrap_or(4.0)
    } else {
        ZOOMS
            .iter()
            .rev()
            .copied()
            .find(|z| *z < current - 1e-3)
            .unwrap_or(0.1)
    };
    EDITOR.write().zoom = Some(next);
}

/// The zoom of the preview as shown.
pub fn fit_zoom_now() -> f32 {
    let v = EDITOR.peek();
    v.zoom.unwrap_or_else(|| fit_zoom(v.size, v.stage))
}

/// The zoom that fits a picture of `size` in the visible area (never above 1).
pub fn fit_zoom(size: (u32, u32), stage: (f32, f32)) -> f32 {
    if size.0 == 0 || size.1 == 0 || stage.0 < 10.0 || stage.1 < 10.0 {
        return 1.0;
    }
    ((stage.0 - 8.0) / size.0 as f32)
        .min((stage.1 - 8.0) / size.1 as f32)
        .clamp(0.01, 1.0)
}

fn fit_visible(st: AppState) {
    if editor_open(st) {
        EDITOR.write().zoom = None;
    } else {
        open_editor(st);
    }
}

fn show_menu(st: AppState, menu: &'static str) {
    if editor_open(st) {
        let mut v = EDITOR.write();
        v.menu = if v.menu == Some(menu) { None } else { Some(menu) };
    } else {
        open_with(st, false, Some(menu));
    }
}

fn convert(st: AppState, f: Format) {
    operate(st, Op::Edit(Edit::Convert(f)));
}

fn filter(st: AppState, f: Filter) {
    operate(st, Op::Edit(Edit::Filter(f)));
}

// ---- Editing in another app ---------------------------------------------------------

/// The picture being edited in another app: where it is and where its file is.
#[derive(Debug, Clone, PartialEq)]
pub struct External {
    pub tab: u64,
    pub page: usize,
    pub index: usize,
    pub path: PathBuf,
    pub modified: Option<std::time::SystemTime>,
}

pub static EXTERNAL: GlobalSignal<Option<External>> = Signal::global(|| None);

/// File types for editing in another app.
pub const EDIT_TYPES: &[&str] = &["png", "jpg", "bmp", "tif"];

/// Which editor `editImage` uses: "builtin", "default" (the system's app for
/// pictures) or "app" (the one chosen with Edit with).
pub fn editor_choice(st: AppState) -> String {
    match st.value("image.editor").as_str() {
        "default" => "default".into(),
        "app" if !st.value("image.editor.app").is_empty() => "app".into(),
        _ => "builtin".into(),
    }
}

pub fn edit_type(st: AppState) -> String {
    let v = st.value("image.editType");
    if EDIT_TYPES.contains(&v.as_str()) {
        v
    } else {
        "png".into()
    }
}

/// Edit the selected picture with the editor chosen in the settings.
pub fn edit_selected(st: AppState) {
    match editor_choice(st).as_str() {
        "builtin" => open_editor(st),
        _ => edit_outside(st, true),
    }
}

/// Write the picture to a file for another app; with `launch` the app opens it.
pub fn edit_outside(st: AppState, launch: bool) {
    let Some((page, index)) = target(st) else { return };
    let Some((tab, work)) = load(st, page, index) else {
        return;
    };
    let ext = edit_type(st);
    let dir = std::env::temp_dir().join("folitecta-pictures");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!(
        "picture-{tab}-{}-{}-{}.{ext}",
        page + 1,
        index + 1,
        next_generation()
    ));
    let written = match ext.as_str() {
        // JPEG and BMP have no transparency.
        "jpg" | "bmp" => image::DynamicImage::ImageRgba8(work.pixels).to_rgb8().save(&path),
        _ => work.pixels.save(&path),
    };
    if let Err(e) = written {
        st.error(e.to_string());
        return;
    }
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    *EXTERNAL.write() = Some(External {
        tab,
        page,
        index,
        path: path.clone(),
        modified,
    });
    st.set_dialog(Dialog::ExternalPicture);
    if launch {
        let r = if editor_choice(st) == "app" {
            std::process::Command::new(st.value("image.editor.app"))
                .arg(&path)
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        } else {
            crate::util::open_external(&path.display().to_string()).map_err(|e| e.to_string())
        };
        if let Err(e) = r {
            st.error(e);
        }
    }
}

/// The picture file was saved since it was written.
pub fn external_changed() -> bool {
    EXTERNAL
        .peek()
        .as_ref()
        .is_some_and(|x| std::fs::metadata(&x.path).and_then(|m| m.modified()).ok() != x.modified)
}

/// Put the picture edited in another app on the page.
pub fn update_from_outside(st: AppState) {
    let Some(x) = EXTERNAL.peek().clone() else { return };
    let pixels = match image::open(&x.path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            st.error(e.to_string());
            return;
        }
    };
    let current = st
        .docs
        .read()
        .iter()
        .find(|t| t.id == x.tab)
        .and_then(|t| t.doc.page_images(x.page).ok()?.get(x.index).cloned());
    let Some(im) = current else { return };
    let work = Work::replacing((im.width, im.height), pixels, im.matrix, im.colors);
    store(st, x.tab, x.page, x.index, &work);
    if let Some(e) = EXTERNAL.write().as_mut() {
        e.modified = std::fs::metadata(&e.path).and_then(|m| m.modified()).ok();
    }
}

fn choose_app(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new().pick_file().await else {
            return;
        };
        st.set_value("image.editor.app", file.path().display().to_string());
        st.set_value("image.editor", "app");
        edit_selected(st);
    });
}

fn set_editor(st: AppState, choice: &str) {
    st.set_value("image.editor", choice);
    edit_selected(st);
}

// ---- Preview of the editor ------------------------------------------------------------

/// Largest edge of the preview.
const PREVIEW_EDGE: u32 = 1600;

/// The last preview: its generation and PNG data.
type Preview = Option<(u64, Arc<Vec<u8>>)>;

static PREVIEW: LazyLock<Mutex<Preview>> = LazyLock::new(|| Mutex::new(None));

/// The preview of the editor as PNG (`/imgedit/{generation}.png`).
pub fn preview(_path: &str) -> Option<Arc<Vec<u8>>> {
    let (generation, small) = {
        let s = SESSION.lock().ok()?;
        let s = s.as_ref()?;
        if let Some((g, png)) = PREVIEW.lock().ok()?.as_ref() {
            if *g == s.generation {
                return Some(png.clone());
            }
        }
        let (w, h) = s.work.size();
        let small = if w.max(h) > PREVIEW_EDGE {
            let k = PREVIEW_EDGE as f32 / w.max(h) as f32;
            image::imageops::thumbnail(
                &s.work.pixels,
                ((w as f32 * k) as u32).max(1),
                ((h as f32 * k) as u32).max(1),
            )
        } else {
            s.work.pixels.clone()
        };
        (s.generation, small)
    };
    let mut out = std::io::Cursor::new(Vec::new());
    small.write_to(&mut out, image::ImageFormat::Png).ok()?;
    let png = Arc::new(out.into_inner());
    if let Ok(mut p) = PREVIEW.lock() {
        *p = Some((generation, png.clone()));
    }
    Some(png)
}

/// Check states of the picture commands.
pub fn pictures_state(st: AppState, id: &str) -> Option<bool> {
    let format = || EDITOR.peek().format;
    match id {
        "tool.editContent.images" => Some((st.tool)() == Tool::EditImages),
        "editImage.builtin" => Some(editor_choice(st) == "builtin"),
        "editImage.default" => Some(editor_choice(st) == "default"),
        "editImage.editWith" => Some(editor_choice(st) == "app"),
        "imageEditor.convert.1bpp_indexed" => Some(format() == Some(Format::Bilevel)),
        "imageEditor.convert.8bpp_indexed" => Some(format() == Some(Format::Indexed)),
        "imageEditor.convert.8bpp_grayscale" => Some(format() == Some(Format::Gray)),
        "imageEditor.convert.grayscaleAlpha" => Some(format() == Some(Format::GrayAlpha)),
        "imageEditor.convert.24bpp" => Some(format() == Some(Format::Rgb)),
        "imageEditor.convert.32bpp" => Some(format() == Some(Format::Rgba)),
        _ => None,
    }
}

fn select_pictures_tool(st: AppState) {
    if no_doc(st) {
        return;
    }
    select_tool(st, Tool::EditImages);
}

handlers! {
    "tool.editContent.images" => select_pictures_tool,
    "editImage.builtin" => |st| set_editor(st, "builtin"),
    "editImage.default" => |st| set_editor(st, "default"),
    "editImage.editWith" => choose_app,
    "editImage.editType" => |st| st.set_dialog(Dialog::PictureSettings),
    "imageEditor.rotate90CW" => |st| operate(st, Op::Edit(Edit::RotateRight)),
    "imageEditor.rotate90CCW" => |st| operate(st, Op::Edit(Edit::RotateLeft)),
    "imageEditor.flipHorizontal" => |st| operate(st, Op::Edit(Edit::FlipHorizontal)),
    "imageEditor.flipVertical" => |st| operate(st, Op::Edit(Edit::FlipVertical)),
    "imageEditor.grayscale" => |st| operate(st, Op::Edit(Edit::Grayscale)),
    "imageEditor.deskew" => |st| operate(st, Op::Deskew),
    "imageEditor.crop" => crop,
    "imageEditor.zoomIn" => |st| zoom_by(st, true),
    "imageEditor.zoomOut" => |st| zoom_by(st, false),
    "imageEditor.fitVisible" => fit_visible,
    "imageEditor.zoom.combo" => |st| show_menu(st, "zoom"),
    "imageEditor.filters.rcm" => |st| show_menu(st, "filters"),
    "imageEditor.filters.despeckle" => |st| operate(st, Op::Edit(Edit::Despeckle)),
    "imageEditor.filters.smooth" => |st| filter(st, Filter::Smooth),
    "imageEditor.filters.gaussianBlur" => |st| filter(st, Filter::GaussianBlur),
    "imageEditor.filters.meanRemoval" => |st| filter(st, Filter::MeanRemoval),
    "imageEditor.filters.sharpen" => |st| filter(st, Filter::Sharpen),
    "imageEditor.filters.embossLaplascian" => |st| filter(st, Filter::EmbossLaplacian),
    "imageEditor.filters.embossHorzVert" => |st| filter(st, Filter::EmbossHorzVert),
    "imageEditor.filters.embossAllDirections" => |st| filter(st, Filter::EmbossAllDirections),
    "imageEditor.filters.embossLossy" => |st| filter(st, Filter::EmbossLossy),
    "imageEditor.filters.edgeDetect" => |st| filter(st, Filter::EdgeDetect),
    "imageEditor.convert.rcm" => |st| show_menu(st, "convert"),
    "imageEditor.convert.1bpp_indexed" => |st| convert(st, Format::Bilevel),
    "imageEditor.convert.8bpp_indexed" => |st| convert(st, Format::Indexed),
    "imageEditor.convert.8bpp_grayscale" => |st| convert(st, Format::Gray),
    "imageEditor.convert.grayscaleAlpha" => |st| convert(st, Format::GrayAlpha),
    "imageEditor.convert.24bpp" => |st| convert(st, Format::Rgb),
    "imageEditor.convert.32bpp" => |st| convert(st, Format::Rgba),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_fits_the_visible_area() {
        assert_eq!(fit_zoom((400, 200), (808.0, 808.0)), 1.0);
        assert!((fit_zoom((1600, 800), (808.0, 808.0)) - 0.5).abs() < 1e-6);
        assert!((fit_zoom((800, 1600), (408.0, 808.0)) - 0.5).abs() < 1e-6);
        assert_eq!(fit_zoom((0, 10), (100.0, 100.0)), 1.0);
    }
}
