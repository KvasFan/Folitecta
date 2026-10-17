//! Tables: created, edited, moved and resized as comments, flattened.

use folitecta_core::Document;
use folitecta_core::tables::Table;

fn ap(doc: &Document, page: usize, index: usize) -> String {
    let id = doc.annotations(page).unwrap()[index].id.unwrap();
    let d = doc.inner().get_dictionary(id).unwrap();
    let n = d
        .get(b"AP")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"N")
        .unwrap()
        .as_reference()
        .unwrap();
    let s = doc.inner().get_object(n).unwrap().as_stream().unwrap();
    String::from_utf8_lossy(&s.decompressed_content().unwrap_or_else(|_| s.content.clone())).to_string()
}

#[test]
fn tables_are_edited_moved_and_flattened() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    let mut t = Table::new(2, 3);
    t.cells[0][0].text = "Item".into();
    t.cells[0][0].bold = true;
    t.cells[0][1].text = "Qty".into();
    t.cells[1][0].text = "Paper".into();
    t.cells[1][1].text = "500".into();
    t.cells[1][1].align = "right".into();
    t.cells[0][2].fill = Some([0.9, 0.9, 0.9]);
    doc.add_table(0, [50.0, 250.0, 350.0, 330.0], &t, "Ann").unwrap();
    assert_eq!(doc.tables(), vec![(0, 0)]);
    let a = ap(&doc, 0, 0);
    for text in ["(Item)", "(Qty)", "(Paper)", "(500)", "/TB", " re f", " re S"] {
        assert!(a.contains(text), "{text} in {a}");
    }
    // Edit: a new row and a merged header.
    let mut t2 = doc.table_at(0, 0).unwrap();
    t2.insert_row(2);
    t2.cells[2][0].text = "Ink".into();
    t2.merge(0, 1, 0, 2).unwrap();
    doc.set_table(0, 0, &t2).unwrap();
    let back = doc.table_at(0, 0).unwrap();
    assert_eq!(back.row_count(), 3);
    assert_eq!(back.cells[0][1].colspan, 2);
    assert!(ap(&doc, 0, 0).contains("(Ink)"));
    // Moving and resizing draw it again for the new rectangle.
    doc.move_annotation(0, 0, 10.0, -20.0).unwrap();
    doc.set_annotation_rect(0, 0, [40.0, 100.0, 380.0, 220.0])
        .unwrap();
    let moved = ap(&doc, 0, 0);
    assert!(moved.contains("(Ink)") && moved.contains("(Item)"), "{moved}");
    doc.render_page_png(0, 1.0).unwrap();
    // Not a table.
    doc.add_note(0, 10.0, 10.0, "note", "Ann", [1.0, 1.0, 0.0])
        .unwrap();
    assert!(doc.table_at(0, 1).is_none());
    assert!(doc.set_table(0, 1, &t).is_err());
    assert!(doc.add_table(0, [0.0, 0.0, 5.0, 5.0], &t, "Ann").is_err());
    // Flattened into the page with the other comments.
    doc.flatten_comments(&[0]).unwrap();
    assert!(doc.page_text(0).contains("Paper"));
}
