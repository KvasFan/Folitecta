//! Notes drawn with each of their icons.

use std::collections::HashSet;

use folitecta_core::{Document, NOTE_ICONS};

fn appearance(doc: &Document) -> Vec<u8> {
    let d = doc
        .inner()
        .get_dictionary(doc.annotations(0).unwrap()[0].id.unwrap())
        .unwrap();
    let n = d.get(b"AP").unwrap().as_dict().unwrap().get(b"N").unwrap();
    let s = doc.inner().get_object(n.as_reference().unwrap()).unwrap().as_stream().unwrap();
    s.decompressed_content().unwrap_or_else(|_| s.content.clone())
}

#[test]
fn every_icon_has_its_own_drawing() {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    doc.add_note_with_icon(0, 50.0, 250.0, "Hello", "Ann", [1.0, 0.85, 0.2], "Key")
        .unwrap();
    assert_eq!(doc.note_icon_of(0, 0).as_deref(), Some("Key"));
    let mut drawings = HashSet::new();
    for icon in NOTE_ICONS {
        doc.set_note_icon(0, 0, icon).unwrap();
        assert_eq!(doc.note_icon_of(0, 0).as_deref(), Some(*icon));
        assert!(drawings.insert(appearance(&doc)), "{icon} draws like another icon");
        doc.render_page_png(0, 1.0).unwrap();
    }
    // One undo step each.
    doc.undo().unwrap();
    assert_eq!(doc.note_icon_of(0, 0).as_deref(), Some(NOTE_ICONS[NOTE_ICONS.len() - 2]));
    assert!(doc.set_note_icon(0, 0, "Banana").is_err());
    // Unknown icons of new notes become the comment bubble.
    doc.add_note_with_icon(0, 150.0, 250.0, "", "Ann", [1.0, 0.85, 0.2], "Banana")
        .unwrap();
    assert_eq!(doc.note_icon_of(0, 1).as_deref(), Some("Comment"));
    // Only notes have icons.
    doc.add_stamp(0, [10.0, 10.0, 120.0, 50.0], "OK", [0.8, 0.1, 0.1], "Ann")
        .unwrap();
    assert_eq!(doc.note_icon_of(0, 2), None);
    assert!(doc.set_note_icon(0, 2, "Star").is_err());
}
