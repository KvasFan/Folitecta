use std::path::{Path, PathBuf};

use folitecta_core::Document;
use folitecta_core::document::write_pdf_file;
use folitecta_core::util::write_atomic;

/// A new empty folder for one test.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-atomic-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Files of a folder, temporary ones included.
fn entries(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

#[test]
fn files_are_replaced_whole_and_leave_no_temporary_file() {
    let dir = folder("replace");
    let file = dir.join("a.pdf");
    std::fs::write(&file, b"old content").unwrap();
    write_atomic(&file, b"new content").unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), b"new content");
    assert_eq!(entries(&dir), ["a.pdf"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_write_leaves_the_target_and_no_temporary_file() {
    let dir = folder("fail");
    // The target is a folder: the rename fails.
    let target = dir.join("b.pdf");
    std::fs::create_dir_all(&target).unwrap();
    assert!(write_atomic(&target, b"%PDF-1.7\n%%EOF").is_err());
    assert!(target.is_dir());
    assert_eq!(entries(&dir), ["b.pdf"]);
    // A folder that does not exist: nothing is created.
    assert!(write_atomic(dir.join("missing").join("c.pdf"), b"x").is_err());
    assert_eq!(entries(&dir), ["b.pdf"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn incomplete_pdf_bytes_are_never_written() {
    let dir = folder("incomplete");
    let file = dir.join("d.pdf");
    std::fs::write(&file, b"%PDF-1.7 the original\n%%EOF\n").unwrap();
    assert!(write_pdf_file(&file, b"%PDF-1.7 cut in the middle").is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"%PDF-1.7 the original\n%%EOF\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_document_stays_unsaved_when_saving_fails() {
    let dir = folder("document");
    let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
    let bad = dir.join("missing").join("e.pdf");
    assert!(doc.save_as(&bad).is_err());
    assert!(doc.is_dirty());
    assert!(doc.path().is_none());
    let good = dir.join("e.pdf");
    doc.save_as(&good).unwrap();
    assert!(!doc.is_dirty());
    assert_eq!(Document::open(&good).unwrap().page_count(), 1);
    assert_eq!(entries(&dir), ["e.pdf"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(windows)]
#[test]
fn a_file_locked_by_another_program_keeps_its_content() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = folder("locked");
    let file = dir.join("f.pdf");
    let mut doc = Document::new_blank(2, 200.0, 200.0).unwrap();
    doc.save_as(&file).unwrap();
    let before = std::fs::read(&file).unwrap();
    // Open without sharing, as some programs do: the file cannot be replaced.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&file)
        .unwrap();
    doc.delete_pages(&[1]).unwrap();
    assert!(doc.save().is_err());
    drop(lock);
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert!(doc.is_dirty());
    assert_eq!(entries(&dir), ["f.pdf"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exports_replace_their_files_whole() {
    let dir = folder("exports");
    let doc = Document::from_text("Exported words", folitecta_core::textdoc::TextLayout::default()).unwrap();
    let text = dir.join("words.txt");
    let picture = dir.join("page.png");
    std::fs::write(&text, "an older, longer text that must not show through").unwrap();
    std::fs::write(&picture, b"old").unwrap();
    doc.export_text(&text).unwrap();
    doc.export_page_image(0, 36.0, folitecta_core::ImageFormat::Png, &picture)
        .unwrap();
    assert!(std::fs::read_to_string(&text).unwrap().contains("Exported words"));
    assert!(!std::fs::read_to_string(&text).unwrap().contains("older"));
    assert!(std::fs::read(&picture).unwrap().starts_with(b"\x89PNG"));
    assert_eq!(entries(&dir), ["page.png", "words.txt"]);
    // A target that cannot be written: an error, and nothing left behind.
    let blocked = dir.join("blocked.txt");
    std::fs::create_dir_all(&blocked).unwrap();
    assert!(doc.export_text(&blocked).is_err());
    assert_eq!(entries(&dir), ["blocked.txt", "page.png", "words.txt"]);
    let _ = std::fs::remove_dir_all(&dir);
}
