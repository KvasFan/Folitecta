//! The user's own templates: a document saved as a template says what it is
//! (name, origin, date, version, fonts it does not carry); a document made
//! from it is a new copy, so the template file never changes.

use folitecta_core::security::{EncryptionProfile, SecurityPermissions};
use folitecta_core::testdocs::{Builder, form_doc, standard_fonts};
use folitecta_core::{Document, Error};
use lopdf::dictionary;

#[test]
fn a_form_saved_as_a_template_makes_new_documents() {
    let dir = std::env::temp_dir().join(format!("folitecta-user-templates-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("order.pdf");
    std::fs::write(&source, form_doc(2, 3)).unwrap();
    let doc = Document::open(&source).unwrap();
    let bytes = doc.template_bytes("  Order form ").unwrap();
    let template = Document::from_bytes(bytes.clone(), None, None).unwrap();
    let info = template.template_info().unwrap();
    assert_eq!(info.name, "Order form");
    assert_eq!(info.origin, "order.pdf");
    assert_eq!(info.version, folitecta_core::VERSION);
    assert!(info.made.starts_with("D:20"), "{}", info.made);
    assert!(info.fonts.is_empty(), "{:?}", info.fonts);
    assert_eq!(info.pages, 2);
    // The document itself is unchanged.
    assert!(doc.template_info().is_none());

    let made = Document::from_template_file(bytes.clone()).unwrap();
    assert!(made.path().is_none(), "a new document, with no file yet");
    assert!(!made.can_undo());
    assert!(made.template_info().is_none(), "not a template itself");
    assert_eq!(made.form_fields().len(), doc.form_fields().len());
    assert_eq!(made.page_count(), 2);
    // A file that is no template.
    assert!(Document::from_template_file(form_doc(1, 1)).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn templates_name_the_fonts_they_need() {
    let mut b = Builder::new();
    let mut fonts = standard_fonts();
    fonts.set(
        "F9",
        dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Garamond-Book" },
    );
    b.page(
        (300.0, 300.0),
        b"BT /F9 12 Tf 20 200 Td (Hello) Tj ET".to_vec(),
        dictionary! { "Font" => fonts },
        Vec::new(),
    );
    let doc = Document::from_bytes(b.finish(), None, None).unwrap();
    assert_eq!(doc.fonts_not_carried(), ["Garamond-Book"]);
    let t = Document::from_bytes(doc.template_bytes("Letterhead").unwrap(), None, None).unwrap();
    assert_eq!(t.template_info().unwrap().fonts, ["Garamond-Book"]);
    // Embedded fonts are carried.
    let embedded = Document::from_bytes(folitecta_core::testdocs::font_doc(1), None, None).unwrap();
    assert!(embedded.fonts_not_carried().is_empty());
}

#[test]
fn protected_documents_and_empty_names_are_refused() {
    let dir = std::env::temp_dir().join(format!("folitecta-user-templates-p-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("locked.pdf");
    Document::new_blank(1, 300.0, 300.0)
        .unwrap()
        .save_encrypted(
            &file,
            "u",
            "o",
            SecurityPermissions::default(),
            EncryptionProfile::Aes256,
        )
        .unwrap();
    let locked = Document::open_with_password(&file, "u").unwrap();
    assert!(matches!(locked.template_bytes("Secret"), Err(Error::Invalid(_))));
    let plain = Document::new_blank(1, 300.0, 300.0).unwrap();
    assert!(plain.template_bytes("   ").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
