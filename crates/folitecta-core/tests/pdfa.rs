//! PDF/A-2b preflight and conversion: a document whose fonts are embedded,
//! with JavaScript, device colours, a hidden note and an interpolated
//! picture, converted into a copy that declares PDF/A-2b and still passes
//! when opened again; a document with a font that is not embedded never
//! declared conformant; a protected document refused; the original never
//! changed.

use std::io::Cursor;

use folitecta_core::{
    Document, EncryptionProfile, Renderer, SecurityPermissions, TextLayout, security::protected_copy,
};
use lopdf::Object;

fn picture() -> Vec<u8> {
    let img = image::RgbImage::from_fn(60, 40, |i, j| image::Rgb([(i * 4) as u8, (j * 6) as u8, 90]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Text beyond WinAnsi: the whole document is set in the embedded font.
fn embedded_only() -> Document {
    let md = "# Отчёт\n\nТекст документа на нескольких строках, со ссылкой и списком.\n\n- один\n- два\n";
    let mut doc = Document::from_markdown(md, TextLayout::default(), "Отчёт").unwrap();
    doc.add_image(0, picture(), Some([100.0, 100.0, 220.0, 180.0]))
        .unwrap();
    doc.set_document_scripts(&[("start".into(), "app.alert('hello');".into())])
        .unwrap();
    let note = doc
        .add_note(0, 300.0, 300.0, "a hidden note", "Tester", [1.0, 0.8, 0.0])
        .unwrap();
    // The note hidden, the picture interpolated.
    let inner = doc.inner_mut();
    inner.get_dictionary_mut(note).unwrap().set("F", 2);
    for obj in inner.objects.values_mut() {
        if let Object::Stream(s) = obj {
            if s.dict.get(b"Subtype").ok().and_then(|n| n.as_name().ok()) == Some(b"Image".as_slice()) {
                s.dict.set("Interpolate", true);
            }
        }
    }
    doc.commit().unwrap();
    doc.clear_history();
    doc
}

fn rules(p: &folitecta_core::Preflight) -> Vec<&'static str> {
    p.broken().into_iter().map(|(r, _)| r).collect()
}

#[test]
fn a_document_with_embedded_fonts_becomes_pdfa_2b() {
    let doc = embedded_only();
    let original = doc.bytes();
    let before = doc.preflight_pdfa();
    let broken = rules(&before);
    for r in [
        "actions",
        "output-intent",
        "annotation-flags",
        "interpolate",
        "identification",
    ] {
        assert!(broken.contains(&r), "{r} in {broken:?}");
    }
    assert!(!broken.contains(&"fonts"), "{:?}", before.findings);
    let (converted, outcome) = doc.convert_to_pdfa().unwrap();
    assert_eq!(doc.bytes(), original, "the original is not changed");
    assert!(outcome.claimed, "{:?}", outcome.after.findings);
    assert!(outcome.after.passed());
    let xmp = converted.xmp_packet().unwrap();
    assert!(xmp.contains("<pdfaid:part>2</pdfaid:part>") && xmp.contains("<pdfaid:conformance>B"));
    assert!(xmp.contains("Отчёт"), "the title is repeated in XMP");
    // Saved and opened again, it still passes, and it draws.
    let reopened = Document::from_bytes(converted.bytes().to_vec(), None, None).unwrap();
    let again = reopened.preflight_pdfa();
    assert!(again.passed(), "{:?}", again.findings);
    let catalog = reopened.inner().catalog().unwrap();
    assert!(catalog.has(b"OutputIntents"));
    assert!(
        !catalog.has(b"Names") || {
            let names = catalog.get(b"Names").unwrap();
            let names = match names {
                Object::Reference(r) => reopened.inner().get_dictionary(*r).unwrap(),
                other => other.as_dict().unwrap(),
            };
            !names.has(b"JavaScript")
        }
    );
    assert!(
        Renderer::new(reopened.bytes())
            .unwrap()
            .render_page(0, 0.3)
            .is_ok()
    );
    // Saved to a file (a new modification date and producer) and opened: still PDF/A.
    let dir = std::env::temp_dir().join(format!("folitecta-pdfa-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("converted.pdf");
    let mut saved = reopened;
    // A modification date later than the conversion's.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    saved.save_as(&path).unwrap();
    let from_disk = Document::open(&path).unwrap();
    let after_save = from_disk.preflight_pdfa();
    std::fs::remove_dir_all(&dir).ok();
    assert!(after_save.passed(), "{:?}", after_save.findings);
}

#[test]
fn a_font_that_is_not_embedded_keeps_the_document_from_claiming_pdfa() {
    // WinAnsi text: the standard Helvetica, not embedded.
    let doc = Document::from_markdown("# Report\n\nPlain text.", TextLayout::default(), "Report").unwrap();
    let before = doc.preflight_pdfa();
    let fonts: Vec<_> = before.findings.iter().filter(|f| f.rule == "fonts").collect();
    assert!(!fonts.is_empty() && fonts.iter().all(|f| !f.fixable), "{fonts:?}");
    assert!(fonts.iter().any(|f| f.detail.contains("Helvetica")));
    let (converted, outcome) = doc.convert_to_pdfa().unwrap();
    assert!(!outcome.claimed);
    assert!(rules(&outcome.after).contains(&"fonts"));
    let xmp = converted.xmp_packet().unwrap();
    assert!(!xmp.contains("pdfaid"), "never declared without conforming");
    // What could be fixed was: the metadata agree with the information.
    assert!(!rules(&outcome.after).contains(&"metadata-sync"));
    assert!(!rules(&outcome.after).contains(&"metadata"));
}

#[test]
fn a_protected_document_is_refused() {
    let plain = embedded_only();
    let bytes = protected_copy(
        plain.bytes().as_slice(),
        "",
        "owner",
        SecurityPermissions::default(),
        EncryptionProfile::Aes256,
    )
    .unwrap();
    let doc = Document::from_bytes(bytes, None, Some("")).unwrap();
    let found = doc.preflight_pdfa();
    assert!(rules(&found).contains(&"encryption"));
    assert!(doc.convert_to_pdfa().is_err());
}
