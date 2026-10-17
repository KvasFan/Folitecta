//! Integration tests for the document model: every operation is exercised on a
//! generated three-page document and verified by re-reading the result.

use folitecta_core::marks::{MarkPosition, TextMarkOptions};
use folitecta_core::{Document, ImageFormat, Markup, OutlineItem, SearchOptions, Shape};

fn sample() -> Document {
    let mut doc = Document::new_blank(3, 595.0, 842.0).expect("blank document");
    // Give every page some text so extraction and search have something to find.
    doc.add_header_footer(
        &[0, 1, 2],
        ["Alpha report", "", ""],
        ["", "Page {page} of {pages}", ""],
        11.0,
        30.0,
        [0.0, 0.0, 0.0],
    )
    .expect("header");
    doc.add_text_watermark(
        &[1],
        &TextMarkOptions {
            text: "BRAVO".into(),
            ..TextMarkOptions::default()
        },
    )
    .expect("watermark");
    doc.add_text_watermark(
        &[2],
        &TextMarkOptions {
            text: "CHARLIE".into(),
            rotation: 0.0,
            ..TextMarkOptions::default()
        },
    )
    .expect("watermark");
    doc
}

fn reloaded(doc: &Document) -> Document {
    Document::from_bytes(doc.bytes().as_ref().clone(), None, None).expect("reload")
}

#[test]
fn blank_document_has_pages_and_metadata() {
    let doc = Document::new_blank(2, 612.0, 792.0).unwrap();
    assert_eq!(doc.page_count(), 2);
    let size = doc.page_size(0).unwrap();
    assert_eq!((size.width, size.height, size.rotation), (612.0, 792.0, 0));
    let info = doc.info();
    assert!(info.producer.contains("Folitecta"));
    assert_eq!(info.page_count, 2);
}

#[test]
fn text_extraction_and_search() {
    let doc = sample();
    let text = doc.page_text(0);
    assert!(text.contains("Alpha report"), "text was {text:?}");
    assert!(text.contains("Page 1 of 3"), "text was {text:?}");
    let hits = doc.search("bravo", SearchOptions::default());
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].page, 1);
    assert!(
        doc.search(
            "bravo",
            SearchOptions {
                case_sensitive: true,
                whole_words: false,
                ..Default::default()
            }
        )
        .is_empty()
    );
    assert_eq!(
        doc.search(
            "Alpha",
            SearchOptions {
                case_sensitive: false,
                whole_words: true,
                ..Default::default()
            }
        )
        .len(),
        3
    );
    let (words, _) = doc.word_count();
    assert!(words >= 15, "words = {words}");
}

#[test]
fn rotate_delete_insert_duplicate_reverse() {
    let mut doc = sample();
    doc.rotate_pages(&[0], 90).unwrap();
    assert_eq!(doc.page_size(0).unwrap().rotation, 90);
    assert_eq!(doc.page_size(0).unwrap().width, 842.0);
    doc.rotate_pages(&[0], -90).unwrap();
    assert_eq!(doc.page_size(0).unwrap().rotation, 0);

    doc.insert_blank_pages(1, 2, 300.0, 400.0).unwrap();
    assert_eq!(doc.page_count(), 5);
    assert_eq!(doc.page_size(1).unwrap().width, 300.0);
    assert!(doc.page_text(1).is_empty());
    assert!(doc.page_text(3).contains("BRAVO"));

    doc.delete_pages(&[1, 2]).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert!(doc.page_text(1).contains("BRAVO"));

    doc.duplicate_pages(&[2]).unwrap();
    assert_eq!(doc.page_count(), 4);
    assert!(doc.page_text(3).contains("CHARLIE"));

    doc.reverse_pages().unwrap();
    assert!(doc.page_text(0).contains("CHARLIE"));
    assert!(doc.page_text(3).contains("Page 1 of 3"));

    doc.move_pages(&[3], 0).unwrap();
    assert!(doc.page_text(0).contains("Page 1 of 3"));

    doc.swap_pages(0, 1).unwrap();
    assert!(doc.page_text(1).contains("Page 1 of 3"));

    assert!(
        doc.delete_pages(&[0, 1, 2, 3]).is_err(),
        "cannot delete every page"
    );
    let again = reloaded(&doc);
    assert_eq!(again.page_count(), 4);
}

#[test]
fn extract_split_and_merge() {
    let doc = sample();
    let part = doc.extract_pages(&[2, 0]).unwrap();
    assert_eq!(part.page_count(), 2);
    assert!(part.page_text(0).contains("CHARLIE"));
    assert!(part.page_text(1).contains("Page 1 of 3"));

    let chunks = doc.split_every(2).unwrap();
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[1].page_count(), 1);

    let mut target = Document::new_blank(1, 200.0, 200.0).unwrap();
    target.insert_document(1, &doc, Some(&[1])).unwrap();
    assert_eq!(target.page_count(), 2);
    assert!(target.page_text(1).contains("BRAVO"));
    target.insert_document(0, &doc, None).unwrap();
    assert_eq!(target.page_count(), 5);
    assert!(target.page_text(0).contains("Alpha report"));
    assert_eq!(reloaded(&target).page_count(), 5);
}

#[test]
fn crop_resize_and_margins() {
    let mut doc = sample();
    doc.crop_pages(&[0], [50.0, 50.0, 300.0, 400.0]).unwrap();
    let s = doc.page_size(0).unwrap();
    assert_eq!((s.width, s.height), (250.0, 350.0));
    doc.reset_crop(&[0]).unwrap();
    assert_eq!(doc.page_size(0).unwrap().width, 595.0);

    doc.crop_pages_by_margins(&[1], 10.0, 20.0, 30.0, 40.0).unwrap();
    let s = doc.page_size(1).unwrap();
    assert_eq!((s.width, s.height), (555.0, 782.0));

    doc.resize_pages(&[2], 400.0, 400.0, true).unwrap();
    let s = doc.page_size(2).unwrap();
    assert_eq!((s.width, s.height), (400.0, 400.0));
    assert!(doc.page_text(2).contains("CHARLIE"), "content survives scaling");

    doc.add_margins(&[2], 10.0, 10.0, 10.0, 10.0).unwrap();
    assert_eq!(doc.page_size(2).unwrap().width, 420.0);
}

#[test]
fn annotations_roundtrip() {
    let mut doc = sample();
    doc.add_note(0, 100.0, 700.0, "Please review", "Ada", [1.0, 0.9, 0.2])
        .unwrap();
    doc.add_free_text(
        0,
        [100.0, 500.0, 300.0, 560.0],
        "Typed text\nsecond line",
        "Ada",
        12.0,
        [0.0, 0.0, 1.0],
        true,
    )
    .unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [50.0, 50.0, 150.0, 150.0],
        [1.0, 0.0, 0.0],
        Some([1.0, 1.0, 0.0]),
        2.0,
        "Ada",
        "",
    )
    .unwrap();
    doc.add_shape(
        0,
        Shape::Circle,
        [200.0, 50.0, 300.0, 150.0],
        [0.0, 0.5, 0.0],
        None,
        1.0,
        "Ada",
        "circle",
    )
    .unwrap();
    doc.add_line(
        0,
        (10.0, 10.0),
        (200.0, 300.0),
        [0.0, 0.0, 0.0],
        1.5,
        true,
        "Ada",
        "",
    )
    .unwrap();
    doc.add_ink(
        0,
        &[vec![(10.0, 10.0), (20.0, 30.0), (40.0, 20.0)]],
        [0.2, 0.2, 0.8],
        2.0,
        "Ada",
    )
    .unwrap();
    doc.add_text_markup(
        0,
        Markup::Highlight,
        &[[100.0, 820.0, 300.0, 820.0, 100.0, 800.0, 300.0, 800.0]],
        [1.0, 1.0, 0.0],
        "Ada",
        "hl",
    )
    .unwrap();
    doc.add_text_markup(
        0,
        Markup::StrikeOut,
        &[[100.0, 790.0, 300.0, 790.0, 100.0, 780.0, 300.0, 780.0]],
        [1.0, 0.0, 0.0],
        "Ada",
        "",
    )
    .unwrap();

    let annots = doc.annotations(0).unwrap();
    assert_eq!(annots.len(), 8);
    assert_eq!(annots[0].subtype, "Text");
    assert_eq!(annots[0].contents, "Please review");
    assert_eq!(annots[0].author, "Ada");
    assert_eq!(annots[1].subtype, "FreeText");
    assert_eq!(annots[6].subtype, "Highlight");
    assert_eq!(doc.comments().len(), 8);

    doc.set_annotation_contents(0, 3, "an ellipse").unwrap();
    assert_eq!(doc.annotations(0).unwrap()[3].contents, "an ellipse");
    doc.set_annotation_hidden(0, 3, true).unwrap();
    assert!(doc.annotations(0).unwrap()[3].hidden);

    doc.delete_annotation(0, 1).unwrap();
    let annots = doc.annotations(0).unwrap();
    assert_eq!(annots.len(), 7);
    assert_eq!(annots[1].subtype, "Square");

    let again = reloaded(&doc);
    assert_eq!(again.annotations(0).unwrap().len(), 7);
    // The page still renders with the annotations in place.
    let img = again.render_page_png(0, 0.5).unwrap();
    assert!(img.width > 200 && img.height > 300);

    assert_eq!(doc.delete_all_comments(Some(0)).unwrap(), 7);
    assert!(doc.annotations(0).unwrap().is_empty());
}

#[test]
fn bookmarks_roundtrip() {
    let mut doc = sample();
    assert!(doc.outline().is_empty());
    doc.add_bookmark("Chapter 1", 0, &[]).unwrap();
    doc.add_bookmark("Chapter 2", 1, &[]).unwrap();
    doc.add_bookmark("Section 2.1", 2, &[1]).unwrap();
    let tree = doc.outline();
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[1].children.len(), 1);
    assert_eq!(tree[1].children[0].title, "Section 2.1");
    assert_eq!(tree[1].children[0].page, Some(2));
    assert_eq!(doc.bookmark_count(), 3);

    doc.rename_bookmark(&[0], "Intro").unwrap();
    doc.set_bookmark_page(&[0], 2).unwrap();
    assert_eq!(doc.outline()[0].title, "Intro");
    assert_eq!(doc.outline()[0].page, Some(2));

    doc.delete_bookmark(&[1, 0]).unwrap();
    assert!(doc.outline()[1].children.is_empty());

    let again = reloaded(&doc);
    assert_eq!(again.outline().len(), 2);

    doc.set_outline(&[OutlineItem::new("Only", Some(0))]).unwrap();
    assert_eq!(doc.outline().len(), 1);
    doc.set_outline(&[]).unwrap();
    assert!(doc.outline().is_empty());
    doc.bookmarks_from_pages().unwrap();
    assert_eq!(doc.bookmark_count(), 3);
}

#[test]
fn attachments_roundtrip() {
    let mut doc = sample();
    assert!(doc.attachments().is_empty());
    doc.add_attachment(
        "notes.txt",
        b"hello attachment".to_vec(),
        "Meeting notes",
        Some("text/plain"),
    )
    .unwrap();
    doc.add_attachment("data.bin", vec![1, 2, 3, 4], "", None)
        .unwrap();
    let atts = doc.attachments();
    assert_eq!(atts.len(), 2);
    let notes = atts.iter().find(|a| a.name == "notes.txt").unwrap();
    assert_eq!(notes.description, "Meeting notes");
    assert_eq!(notes.size, Some(16));
    assert_eq!(doc.attachment_data(notes).unwrap(), b"hello attachment");
    assert_eq!(notes.mime, "text/plain");

    let again = reloaded(&doc);
    assert_eq!(again.attachments().len(), 2);

    doc.remove_attachment(notes).unwrap();
    assert_eq!(doc.attachments().len(), 1);
    assert_eq!(doc.attachments()[0].name, "data.bin");
}

#[test]
fn links_and_destinations() {
    let mut doc = sample();
    doc.add_uri_link(0, [10.0, 10.0, 100.0, 30.0], "https://example.org")
        .unwrap();
    doc.add_page_link(0, [10.0, 40.0, 100.0, 60.0], 2).unwrap();
    let links = doc.links(0).unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(
        links[0].target,
        folitecta_core::LinkTarget::Uri("https://example.org".into())
    );
    assert_eq!(links[1].target, folitecta_core::LinkTarget::Page(2));
    assert_eq!(doc.all_links().len(), 2);
    assert_eq!(reloaded(&doc).links(0).unwrap().len(), 2);
    assert_eq!(doc.remove_links(Some(0)).unwrap(), 2);
    assert!(doc.links(0).unwrap().is_empty());
    assert!(doc.named_destinations().is_empty());
}

#[test]
fn page_marks_can_be_removed() {
    let mut doc = sample();
    assert_eq!(doc.marks_on_page(1), vec!["HeaderFooter", "Watermark"]);
    doc.add_page_numbers(
        &[0, 1, 2],
        "ABC-{bates:5}",
        100,
        MarkPosition::BottomRight,
        9.0,
        20.0,
        [0.0, 0.0, 0.0],
    )
    .unwrap();
    assert!(doc.page_text(2).contains("ABC-00102"));
    assert_eq!(doc.remove_marks(&[1], Some("Watermark")).unwrap(), 1);
    assert!(!doc.page_text(1).contains("BRAVO"));
    assert!(doc.page_text(1).contains("Alpha report"));
    assert_eq!(doc.remove_marks(&[0, 1, 2], None).unwrap(), 2 + 2 + 3);
    assert!(doc.page_text(0).is_empty());
    assert_eq!(doc.empty_pages(), vec![0, 1, 2]);
}

#[test]
fn metadata_update() {
    let mut doc = sample();
    doc.set_info("My Title", "Ada Lovelace", "Subject", "a, b")
        .unwrap();
    let info = doc.info();
    assert_eq!(info.title, "My Title");
    assert_eq!(info.author, "Ada Lovelace");
    assert_eq!(reloaded(&doc).info().keywords, "a, b");
    doc.set_info("", "Ada Lovelace", "", "").unwrap();
    assert_eq!(doc.info().title, "");
}

#[test]
fn undo_redo_restores_state() {
    let mut doc = sample();
    assert!(doc.can_undo(), "building the sample recorded history");
    doc.clear_history();
    assert!(!doc.can_undo());
    doc.rotate_pages(&[0], 90).unwrap();
    doc.delete_pages(&[2]).unwrap();
    assert_eq!(doc.page_count(), 2);
    doc.undo().unwrap();
    assert_eq!(doc.page_count(), 3);
    assert_eq!(doc.page_size(0).unwrap().rotation, 90);
    doc.undo().unwrap();
    assert_eq!(doc.page_size(0).unwrap().rotation, 0);
    assert!(doc.undo().is_err());
    doc.redo().unwrap();
    assert_eq!(doc.page_size(0).unwrap().rotation, 90);
    doc.redo().unwrap();
    assert_eq!(doc.page_count(), 2);
    assert!(doc.redo().is_err());
}

#[test]
fn render_and_export() {
    let doc = sample();
    let img = doc.render_page_png(0, 1.0).unwrap();
    assert_eq!((img.width, img.height), (595, 842));
    assert!(img.png.starts_with(&[0x89, b'P', b'N', b'G']));
    let decoded = image::load_from_memory(&img.png).unwrap().to_rgb8();
    let dark = decoded.pixels().filter(|p| p[0] < 128).count();
    assert!(
        dark > 50,
        "rendered page shows the header text (dark pixels: {dark})"
    );

    let dir = std::env::temp_dir().join(format!("folitecta-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let files = doc
        .export_pages_images(&[0, 2], &dir, 36.0, ImageFormat::Jpeg { quality: 80 }, "page")
        .unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0].exists());
    doc.export_text(dir.join("text.txt")).unwrap();
    assert!(
        std::fs::read_to_string(dir.join("text.txt"))
            .unwrap()
            .contains("CHARLIE")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_and_reopen() {
    let mut doc = sample();
    let path = std::env::temp_dir().join(format!("folitecta-save-{}.pdf", std::process::id()));
    doc.save_as(&path).unwrap();
    assert!(!doc.is_dirty());
    assert_eq!(doc.display_name(), path.file_name().unwrap().to_string_lossy());
    let opened = Document::open(&path).unwrap();
    assert_eq!(opened.page_count(), 3);
    assert!(opened.page_text(1).contains("BRAVO"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn render_service_serves_pages() {
    let doc = sample();
    let service = folitecta_core::RenderService::new(doc.bytes());
    let sizes = service.page_sizes_blocking().unwrap();
    assert_eq!(sizes.len(), 3);
    let page = service.render_blocking(1, 0.25).unwrap();
    assert_eq!(page.width, (595.0f32 * 0.25) as u32);
    let cached = service.render_blocking(1, 0.25).unwrap();
    assert_eq!(cached, page);
    let mut edited = sample();
    edited.delete_pages(&[0]).unwrap();
    service.replace(edited.bytes());
    assert_eq!(service.page_sizes_blocking().unwrap().len(), 2);
}
