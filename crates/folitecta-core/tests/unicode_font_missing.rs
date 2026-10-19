//! Generated text when the Unicode font is missing or cannot be used. A test
//! program of its own: the font folders are shared by the whole process.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use folitecta_core::fonts::{UNICODE_FONT_FILE, set_font_dirs};
use folitecta_core::{Document, Error, GlyphProblem};

/// The tests change the folders one after the other.
static FOLDERS: Mutex<()> = Mutex::new(());

const BOX: [f32; 4] = [50.0, 600.0, 545.0, 640.0];

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-fonts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bundled() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts")
}

fn freesans() -> Vec<u8> {
    std::fs::read(bundled().join(UNICODE_FONT_FILE)).unwrap()
}

fn add_box(doc: &mut Document, text: &str) -> folitecta_core::Result<lopdf::ObjectId> {
    doc.add_free_text(0, BOX, text, "Reviewer", 12.0, [0.0; 3], true)
}

fn font_subtype(doc: &Document) -> String {
    doc.annotation_fonts(0, 0).unwrap()[0].subtype.clone()
}

fn appearance_of(doc: &Document) -> lopdf::ObjectId {
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    let annot = doc.inner().get_dictionary(id).unwrap();
    let ap = annot.get(b"AP").unwrap().as_dict().unwrap();
    ap.get(b"N").unwrap().as_reference().unwrap()
}

/// A copy of FreeSans whose OS/2 table says `fs_type`.
fn with_fs_type(fs_type: u16) -> Vec<u8> {
    let mut data = freesans();
    let tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..tables {
        let rec = 12 + 16 * i;
        if &data[rec..rec + 4] == b"OS/2" {
            let at =
                u32::from_be_bytes([data[rec + 8], data[rec + 9], data[rec + 10], data[rec + 11]]) as usize;
            data[at + 8..at + 10].copy_from_slice(&fs_type.to_be_bytes());
            return data;
        }
    }
    panic!("no OS/2 table");
}

#[test]
fn without_the_font_file_unicode_text_is_refused_and_winansi_text_still_works() {
    let _guard = FOLDERS.lock().unwrap_or_else(|e| e.into_inner());
    let empty = folder("empty");
    set_font_dirs(vec![empty.clone()]);
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    doc.add_note(0, 20.0, 820.0, "note", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    doc.undo().unwrap();
    let (revision, bytes) = (doc.revision(), doc.bytes());
    match add_box(&mut doc, "Łódź") {
        Err(Error::Glyphs(GlyphProblem::FontMissing { file, searched })) => {
            assert_eq!(file, UNICODE_FONT_FILE);
            assert_eq!(searched, vec![empty.clone()]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(doc.revision(), revision);
    assert_eq!(*doc.bytes(), *bytes);
    assert!(doc.can_redo() && !doc.can_undo());
    // Text in WinAnsi does not need the font.
    add_box(&mut doc, "Lodz").unwrap();
    assert_eq!(font_subtype(&doc), "Type1");
    // Checking text gives the same answer without a document.
    assert!(matches!(
        folitecta_core::fonts::check_text("Ελληνικά"),
        Err(GlyphProblem::FontMissing { .. })
    ));
    assert!(folitecta_core::fonts::check_text("Città").is_ok());
    // Widths fall back to Helvetica.
    assert!(folitecta_core::fonts::text_width("Łódź", 10.0) > 0.0);
}

#[test]
fn a_moved_text_box_keeps_its_drawing_and_a_resize_is_refused() {
    let _guard = FOLDERS.lock().unwrap_or_else(|e| e.into_inner());
    set_font_dirs(vec![bundled()]);
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    add_box(&mut doc, "Łódź").unwrap();
    assert_eq!(font_subtype(&doc), "Type0");
    set_font_dirs(vec![folder("gone")]);
    let ap = appearance_of(&doc);
    doc.move_annotation(0, 0, 20.0, -30.0).unwrap();
    assert_eq!(
        appearance_of(&doc),
        ap,
        "the drawing is kept, never redrawn with '?'"
    );
    assert_eq!(
        doc.annotations(0).unwrap()[0].rect,
        [BOX[0] + 20.0, BOX[1] - 30.0, BOX[2] + 20.0, BOX[3] - 30.0]
    );
    let (revision, bytes, undo) = (doc.revision(), doc.bytes(), doc.can_undo());
    let r = doc.annotations(0).unwrap()[0].rect;
    assert!(matches!(
        doc.set_annotation_rect(0, 0, [r[0], r[1] - 50.0, r[2], r[3]]),
        Err(Error::Glyphs(GlyphProblem::FontMissing { .. }))
    ));
    assert_eq!((doc.revision(), doc.can_undo()), (revision, undo));
    assert_eq!(*doc.bytes(), *bytes);
    // With the font back, the resize redraws the text.
    set_font_dirs(vec![bundled()]);
    doc.set_annotation_rect(0, 0, [r[0], r[1] - 50.0, r[2], r[3]])
        .unwrap();
    assert_ne!(appearance_of(&doc), ap);
    assert_eq!(font_subtype(&doc), "Type0");
}

#[test]
fn unusable_font_files_are_reported_and_the_next_folder_is_used() {
    let _guard = FOLDERS.lock().unwrap_or_else(|e| e.into_inner());
    let unusable = |name: &str, data: &[u8]| -> (PathBuf, GlyphProblem) {
        let dir = folder(name);
        std::fs::write(dir.join(UNICODE_FONT_FILE), data).unwrap();
        set_font_dirs(vec![dir.clone()]);
        let problem = folitecta_core::fonts::check_text("Łódź").unwrap_err();
        (dir, problem)
    };
    let (truncated, problem) = unusable("truncated", &freesans()[..2000]);
    assert!(
        matches!(&problem, GlyphProblem::FontUnreadable { path, .. } if *path == truncated.join(UNICODE_FONT_FILE)),
        "{problem:?}"
    );
    let (_, problem) = unusable("text", b"this is not a font");
    assert!(
        matches!(problem, GlyphProblem::FontUnreadable { .. }),
        "{problem:?}"
    );
    let (_, problem) = unusable("restricted", &with_fs_type(0x0002));
    assert_eq!(
        problem,
        GlyphProblem::NotEmbeddable {
            font: "FreeSans".into()
        }
    );
    let (_, problem) = unusable("no-subsetting", &with_fs_type(0x0100));
    assert!(
        matches!(problem, GlyphProblem::NotEmbeddable { .. }),
        "{problem:?}"
    );
    // Preview & Print allows embedding.
    let print = folder("print");
    std::fs::write(print.join(UNICODE_FONT_FILE), with_fs_type(0x0004)).unwrap();
    set_font_dirs(vec![print]);
    assert!(folitecta_core::fonts::check_text("Łódź").is_ok());
    // A damaged file in the first folder: the next folder's font is used.
    set_font_dirs(vec![truncated, bundled()]);
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    add_box(&mut doc, "Łódź").unwrap();
    assert_eq!(font_subtype(&doc), "Type0");
    set_font_dirs(vec![bundled()]);
}
