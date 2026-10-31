//! Names beyond WinAnsi in what Folitecta draws from them (FOL-L018/S05): the
//! visible part of a digital signature, a typed signature and the poster of
//! a media comment use the embedded font; a typed signature that cannot be
//! drawn is refused before any change; a name in a certificate or a file
//! that cannot be drawn never stops the work.

use folitecta_core::digsig::{DigitalId, IdentityInfo, KeyKind, SignOptions, SignPlacement};
use folitecta_core::{Document, Error, SignatureMark};

/// The file has a subset of the embedded font.
fn embeds_font(bytes: &[u8]) -> bool {
    lopdf::Document::load_mem(bytes)
        .unwrap()
        .objects
        .values()
        .any(|o| {
            o.as_dict().is_ok_and(|d| {
                d.get(b"BaseFont")
                    .and_then(|b| b.as_name())
                    .is_ok_and(|n| String::from_utf8_lossy(n).contains("+FreeSans"))
            })
        })
}

fn identity(name: &str) -> IdentityInfo {
    IdentityInfo {
        name: name.into(),
        organization: "Example".into(),
        unit: "Legal".into(),
        email: "signer@example.org".into(),
        country: "IT".into(),
    }
}

fn signed(name: &str) -> Vec<u8> {
    let id = DigitalId::create_self_signed(&identity(name), KeyKind::EcP256, 3).unwrap();
    let doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    let opts = SignOptions {
        reason: "Zgoda".into(),
        placement: SignPlacement::New {
            page: 0,
            rect: [40.0, 40.0, 240.0, 100.0],
        },
        ..SignOptions::default()
    };
    doc.sign(&id, &opts).unwrap()
}

#[test]
fn the_visible_part_of_a_signature_shows_the_name() {
    assert!(!embeds_font(&signed("Ada Rossi")));
    assert!(embeds_font(&signed("Łukasz Żółć")));
    // No letters for it: signed all the same.
    assert!(!signed("漢字").is_empty());
}

#[test]
fn typed_signatures_beyond_winansi() {
    let typed = |text: &str| SignatureMark::Typed {
        text: text.into(),
        color: [0.0, 0.0, 0.5],
    };
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    doc.add_signature_mark(0, [40.0, 40.0, 240.0, 100.0], &typed("Ada"), "Ada")
        .unwrap();
    assert!(!embeds_font(&doc.bytes()));
    doc.add_signature_mark(0, [40.0, 140.0, 240.0, 200.0], &typed("Łukasz"), "Łukasz")
        .unwrap();
    assert!(embeds_font(&doc.bytes()));
    let before = doc.bytes().to_vec();
    let undo = doc.can_undo();
    let refused = doc.add_signature_mark(0, [40.0, 240.0, 240.0, 300.0], &typed("漢字"), "x");
    assert!(matches!(refused, Err(Error::Glyphs(_))));
    assert_eq!(doc.bytes().to_vec(), before);
    assert_eq!(doc.can_undo(), undo);
}

#[test]
fn the_poster_of_a_media_comment_shows_its_file_name() {
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    doc.add_media(0, [40.0, 40.0, 240.0, 200.0], "plain.mp4", vec![0; 16], "Ann")
        .unwrap();
    assert!(!embeds_font(&doc.bytes()));
    doc.add_media(0, [40.0, 240.0, 240.0, 400.0], "Łódź.mp4", vec![0; 16], "Ann")
        .unwrap();
    assert!(embeds_font(&doc.bytes()));
    doc.add_media(0, [240.0, 240.0, 390.0, 400.0], "漢字.mp4", vec![0; 16], "Ann")
        .unwrap();
}

#[test]
fn redaction_codes_beyond_winansi() {
    use folitecta_core::redaction::{RedactionLook, RedactionMethod};
    let look = |text: &str| RedactionLook {
        text: text.into(),
        ..RedactionLook::default()
    };
    for method in [RedactionMethod::Content, RedactionMethod::Picture] {
        let mut doc = Document::from_text("Visible words on the page", Default::default()).unwrap();
        doc.add_redaction_with(0, [40.0, 40.0, 240.0, 100.0], "Ann", &look("Łódź"))
            .unwrap();
        doc.apply_redactions_with(None, method).unwrap();
        assert!(embeds_font(&doc.bytes()), "{method:?}");
        // Again on the same page: the first one's font stays.
        doc.add_redaction_with(0, [40.0, 140.0, 240.0, 200.0], "Ann", &look("Ελληνικά"))
            .unwrap();
        doc.apply_redactions_with(None, method).unwrap();
        assert!(embeds_font(&doc.bytes()), "{method:?}");
    }
    let mut doc = Document::from_text("Visible", Default::default()).unwrap();
    let before = doc.bytes().to_vec();
    let refused = doc.add_redaction_with(0, [40.0, 40.0, 240.0, 100.0], "Ann", &look("漢字"));
    assert!(matches!(refused, Err(Error::Glyphs(_))));
    assert_eq!(doc.bytes().to_vec(), before);
}
