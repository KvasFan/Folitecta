//! Document tools: split at cuts, crop removal, space audit, image
//! recompression, PDF/A, document actions.

use folitecta_core::Document;
use lopdf::{Object, Stream, dictionary};

fn noisy_png(w: u32, h: u32) -> Vec<u8> {
    let mut seed = 12345u32;
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let n = (seed >> 16) as u8;
        image::Rgb([(x as u8).wrapping_add(n / 4), (y as u8).wrapping_add(n / 3), n])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn pages_split_at_cut_lines() {
    let mut doc = Document::new_blank(2, 400.0, 600.0).unwrap();
    // One vertical cut and two horizontal cuts: six parts of the first page.
    let made = doc.split_pages_at(&[0], &[200.0], &[200.0, 400.0]).unwrap();
    assert_eq!(made, 5);
    assert_eq!(doc.page_count(), 7);
    let sizes = doc.page_sizes();
    assert!((sizes[0].width - 200.0).abs() < 0.5 && (sizes[0].height - 200.0).abs() < 0.5);
    assert!((sizes[6].width - 400.0).abs() < 0.5);
    // Cuts outside the page do nothing.
    assert!(doc.split_pages_at(&[6], &[900.0], &[]).is_err());
    doc.undo().unwrap();
    assert_eq!(doc.page_count(), 2);
}

#[test]
fn cropped_content_is_removed() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    doc.add_note(0, 380.0, 380.0, "outside", "Ann", [1.0, 1.0, 0.0])
        .unwrap();
    doc.add_note(0, 100.0, 100.0, "inside", "Ann", [1.0, 1.0, 0.0])
        .unwrap();
    doc.crop_pages(&[0], [50.0, 50.0, 300.0, 300.0]).unwrap();
    assert_eq!(doc.remove_cropped_content(&[0]).unwrap(), 1);
    let size = doc.page_sizes()[0];
    assert_eq!(size.media_box, size.crop_box);
    let notes = doc.annotations(0).unwrap();
    assert_eq!(notes.iter().filter(|a| a.subtype == "Text").count(), 1);
    assert_eq!(doc.remove_cropped_content(&[0]).unwrap(), 0);
    doc.render_page_png(0, 1.0).unwrap();
}

#[test]
fn space_is_audited_and_images_recompressed() {
    let mut doc = Document::new_blank(2, 400.0, 400.0).unwrap();
    doc.add_image(0, noisy_png(300, 200), None).unwrap();
    doc.add_note(1, 100.0, 100.0, "note", "Ann", [1.0, 1.0, 0.0])
        .unwrap();
    let audit = doc.space_audit();
    let get = |c: &str| {
        audit
            .iter()
            .find(|i| i.category == c)
            .map(|i| i.bytes)
            .unwrap_or(0)
    };
    assert!(get("images") > 50_000, "{audit:?}");
    assert!(get("content") > 0 && get("annotations") > 0);
    let report = doc.recompress_images(None, 60, Some(150)).unwrap();
    assert_eq!(report.images, 1);
    assert!(report.after < report.before);
    let after = doc.space_audit();
    let images_after = after.iter().find(|i| i.category == "images").unwrap().bytes;
    assert!(images_after < get("images"));
    doc.render_page_png(0, 1.0).unwrap();
    // Already small: nothing more to gain.
    assert_eq!(doc.recompress_images(Some(&[0]), 95, None).unwrap().images, 0);
}

#[test]
fn pdfa_identification_can_be_discarded() {
    let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
    let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" pdfaid:conformance="B"><pdfaid:part>2</pdfaid:part><dc:title>T</dc:title></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let id = doc.inner_mut().add_object(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        xmp.as_bytes().to_vec(),
    ));
    doc.inner_mut()
        .catalog_mut()
        .unwrap()
        .set("Metadata", Object::Reference(id));
    doc.commit().unwrap();
    assert!(doc.is_pdfa());
    assert!(doc.discard_pdfa().unwrap());
    assert!(!doc.is_pdfa());
    let meta = String::from_utf8_lossy(&doc.inner().get_object(id).unwrap().as_stream().unwrap().content)
        .to_string();
    assert!(
        !meta.contains("pdfaid") && meta.contains("<dc:title>T</dc:title>"),
        "{meta}"
    );
    assert!(!doc.discard_pdfa().unwrap());
}

#[test]
fn document_actions_are_edited() {
    let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
    assert!(doc.document_actions().is_empty());
    doc.set_document_action("WP", "app.alert('printing');").unwrap();
    doc.set_document_action("Open", "console.println('hi');").unwrap();
    let reopened = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    let acts = reopened.document_actions();
    assert_eq!(acts.len(), 2);
    assert!(acts.iter().any(|(e, c)| e == "WP" && c.contains("printing")));
    doc.set_document_action("WP", "").unwrap();
    doc.set_document_action("Open", "").unwrap();
    assert!(doc.document_actions().is_empty());
    assert!(doc.set_document_action("XX", "x").is_err());
}

#[test]
fn marking_many_results_is_one_undo_step() {
    let mut doc = Document::from_text("fox one fox two fox three", Default::default()).unwrap();
    let hits = doc.search("fox", Default::default());
    assert_eq!(hits.len(), 3);
    doc.markup_hits(&hits, folitecta_core::Markup::Highlight, [1.0, 1.0, 0.0], "Ann")
        .unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 3);
    doc.undo().unwrap();
    assert!(doc.annotations(0).unwrap().is_empty());
}

#[test]
fn the_objects_of_the_file_are_inspected_from_the_trailer_down() {
    use folitecta_core::InspectPath;
    let doc = folitecta_core::Document::from_text("Hello inspector", Default::default()).unwrap();
    let trailer = doc.inspect(&InspectPath::trailer()).unwrap();
    let root = trailer.entries.iter().find(|e| e.key == "/Root").unwrap();
    assert_eq!(root.kind, "reference");
    assert!(root.summary.contains("/Catalog"), "{root:?}");
    let catalog = doc.inspect(root.open.as_ref().unwrap()).unwrap();
    let pages = catalog.entries.iter().find(|e| e.key == "/Pages").unwrap();
    let tree = doc.inspect(pages.open.as_ref().unwrap()).unwrap();
    let kids = tree.entries.iter().find(|e| e.key == "/Kids").unwrap();
    assert_eq!(kids.kind, "array");
    // Inside an array of the object: a step, then the page it names.
    let list = doc.inspect(kids.open.as_ref().unwrap()).unwrap();
    assert_eq!(list.path.steps, vec!["/Kids".to_string()]);
    assert!(
        list.path.title().ends_with("obj › /Kids"),
        "{}",
        list.path.title()
    );
    let page = doc.inspect(list.entries[0].open.as_ref().unwrap()).unwrap();
    assert_eq!(page.path.object, Some(doc.page_ids()[0]));
    let media = page.entries.iter().find(|e| e.key == "/MediaBox").unwrap();
    assert!(
        media.summary.starts_with('[') && media.open.is_some(),
        "{media:?}"
    );
    // The content stream: its data read as text.
    let contents = page.entries.iter().find(|e| e.key == "/Contents").unwrap();
    let at = doc.inspect(contents.open.as_ref().unwrap()).unwrap();
    // One stream, or an array of them (the text in one of them).
    let streams: Vec<_> = match at.stream {
        Some(s) => vec![s],
        None => at
            .entries
            .iter()
            .filter_map(|e| doc.inspect(e.open.as_ref()?).ok()?.stream)
            .collect(),
    };
    assert!(streams.iter().all(|s| s.is_text), "{streams:?}");
    assert!(streams.iter().any(|s| s.text.contains("Tj")), "{streams:?}");
    // A place that is not there.
    let wrong = InspectPath {
        object: Some(doc.page_ids()[0]),
        steps: vec!["/Nothing".into()],
    };
    assert!(doc.inspect(&wrong).is_err());
    assert!(doc.object_count() > 5);
}

#[test]
fn document_actions_cleared_in_one_step() {
    let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
    assert_eq!(doc.clear_document_actions().unwrap(), 0);
    doc.set_document_action("WP", "app.alert('printing');").unwrap();
    doc.set_document_action("WC", "app.alert('closing');").unwrap();
    doc.set_document_action("Open", "console.println('hi');").unwrap();
    assert_eq!(doc.clear_document_actions().unwrap(), 3);
    assert!(doc.document_actions().is_empty());
    doc.undo().unwrap();
    assert_eq!(
        doc.document_actions().len(),
        3,
        "one undo step brings all three back"
    );
}
