use folitecta_core::Document;
use folitecta_core::accessibility::{CheckResult, CheckStatus, Fix, RULES, rule};
use folitecta_core::structure::StructProps;
use folitecta_core::textdoc::TextLayout;

fn status(results: &[CheckResult], id: &str) -> CheckStatus {
    results.iter().find(|r| r.rule == id).map(|r| r.status).unwrap()
}

fn items(results: &[CheckResult], id: &str) -> usize {
    results
        .iter()
        .find(|r| r.rule == id)
        .map(|r| r.items.len())
        .unwrap()
}

#[test]
fn untagged_document_fails_and_fixes_pass() {
    let mut doc = Document::from_text("Report\n\nSome text for the reader.", TextLayout::default()).unwrap();
    doc.add_uri_link(0, [56.0, 700.0, 200.0, 715.0], "https://example.org")
        .unwrap();
    let r = doc.accessibility_check();
    assert_eq!(r.len(), RULES.len());
    for id in [
        "tagged",
        "language",
        "title",
        "tagged_content",
        "tagged_annotations",
        "tab_order",
    ] {
        assert_eq!(status(&r, id), CheckStatus::Failed, "{id}");
    }
    assert_eq!(status(&r, "logical_order"), CheckStatus::Manual);
    assert_eq!(status(&r, "figures_alt"), CheckStatus::Manual);
    assert_eq!(status(&r, "navigation_links"), CheckStatus::Manual);
    assert_eq!(status(&r, "permission"), CheckStatus::Passed);
    assert_eq!(status(&r, "bookmarks"), CheckStatus::Passed);
    assert_eq!(status(&r, "encoding"), CheckStatus::Passed);
    assert_eq!(rule("language").unwrap().fix, Fix::Ask);

    doc.auto_tag().unwrap();
    doc.set_language("en-US").unwrap();
    // No heading (one text size): the name given by the caller.
    doc.fix_title("report.pdf").unwrap();
    assert_eq!(doc.title(), "report.pdf");
    let r = doc.accessibility_check();
    for id in [
        "tagged",
        "language",
        "title",
        "tagged_content",
        "tagged_annotations",
        "tab_order",
        "figures_alt",
        "heading_nesting",
    ] {
        assert_eq!(
            status(&r, id),
            CheckStatus::Passed,
            "{id}: {:?}",
            r.iter().find(|x| x.rule == id)
        );
    }
    // Undo of the title fix is one step.
    doc.undo().unwrap();
    assert_eq!(doc.title(), "");
}

#[test]
fn figures_fields_and_headings() {
    let mut doc = Document::from_text("Title\n\nBody", TextLayout::default()).unwrap();
    doc.auto_tag().unwrap();
    let tree = doc.structure().unwrap();
    let document = tree.children().next().unwrap().id;
    let fig = doc.add_struct_elem(Some(document), None, "Figure", "").unwrap();
    let h3 = doc.add_struct_elem(Some(document), None, "H3", "").unwrap();
    let r = doc.accessibility_check();
    assert_eq!(status(&r, "figures_alt"), CheckStatus::Failed);
    assert_eq!(items(&r, "figures_alt"), 1);
    assert_eq!(status(&r, "heading_nesting"), CheckStatus::Failed);
    doc.set_struct_props(
        fig,
        &StructProps {
            tag: "Figure".into(),
            alt: "Chart".into(),
            ..StructProps::default()
        },
    )
    .unwrap();
    doc.set_struct_tag(h3, "H1").unwrap();
    let r = doc.accessibility_check();
    assert_eq!(status(&r, "figures_alt"), CheckStatus::Passed);
    // An element with alternate text and no content.
    assert_eq!(status(&r, "alt_content"), CheckStatus::Failed);
    // Lists and tables.
    let li = doc.add_struct_elem(Some(document), None, "LI", "").unwrap();
    doc.add_struct_elem(Some(li), None, "LBody", "").unwrap();
    let table = doc.add_struct_elem(Some(document), None, "Table", "").unwrap();
    let tr = doc.add_struct_elem(Some(table), None, "TR", "").unwrap();
    doc.add_struct_elem(Some(tr), None, "TD", "").unwrap();
    doc.add_struct_elem(Some(document), None, "TD", "").unwrap();
    let r = doc.accessibility_check();
    assert_eq!(status(&r, "list_items"), CheckStatus::Failed);
    assert_eq!(status(&r, "list_lbl_body"), CheckStatus::Passed);
    assert_eq!(status(&r, "table_headers"), CheckStatus::Failed);
    assert_eq!(status(&r, "table_cells"), CheckStatus::Failed);
    assert_eq!(status(&r, "table_rows"), CheckStatus::Passed);
    assert_eq!(status(&r, "table_summary"), CheckStatus::Failed);
}

#[test]
fn form_fields_tab_order_and_bookmarks() {
    let mut doc = Document::from_text("Form", TextLayout::default()).unwrap();
    let props = folitecta_core::FieldProps {
        name: "contact.firstName".into(),
        ..folitecta_core::FieldProps::default()
    };
    doc.add_form_field(
        0,
        folitecta_core::NewField::Text,
        [60.0, 600.0, 300.0, 624.0],
        &props,
    )
    .unwrap();
    doc.auto_tag().unwrap();
    let r = doc.accessibility_check();
    assert_eq!(status(&r, "tagged_fields"), CheckStatus::Passed);
    assert_eq!(status(&r, "field_descriptions"), CheckStatus::Failed);
    assert_eq!(doc.fill_field_descriptions().unwrap(), 1);
    let r = doc.accessibility_check();
    assert_eq!(status(&r, "field_descriptions"), CheckStatus::Passed);
    assert_eq!(doc.set_tab_order_structure().unwrap(), 0);
    // A long document without bookmarks.
    let text = (0..22).map(|i| format!("Chapter {i}\u{c}")).collect::<String>();
    let mut long = Document::from_text(&text, TextLayout::default()).unwrap();
    assert_eq!(
        status(&long.accessibility_check(), "bookmarks"),
        CheckStatus::Failed
    );
    let n = long.bookmarks_from_structure().unwrap();
    assert_eq!(n, 22);
    assert_eq!(
        status(&long.accessibility_check(), "bookmarks"),
        CheckStatus::Passed
    );
}

#[test]
fn bookmarks_follow_heading_levels() {
    let mut doc = Document::from_text("One\n\nTwo\n\nThree", TextLayout::default()).unwrap();
    doc.auto_tag().unwrap();
    let ps = doc.struct_elems_of("P");
    doc.set_struct_tag(ps[0].id, "H1").unwrap();
    doc.set_struct_tag(ps[1].id, "H2").unwrap();
    doc.set_struct_tag(ps[2].id, "H1").unwrap();
    assert_eq!(doc.bookmarks_from_structure().unwrap(), 3);
    let outline = doc.outline();
    assert_eq!(outline.len(), 2);
    assert_eq!(outline[0].title, "One");
    assert_eq!(outline[0].children[0].title, "Two");
    assert_eq!(outline[1].title, "Three");
}

/// The elements of the tags with their role, in order.
fn elems(doc: &Document) -> Vec<folitecta_core::structure::StructElem> {
    let mut out = Vec::new();
    doc.structure().unwrap().walk(&mut |e, _| out.push(e.clone()));
    out
}

/// A table of three rows tagged without content: three cells, three, two.
fn bare_table(doc: &mut Document) {
    let table = doc.add_struct_elem(None, None, "Table", "").unwrap();
    for count in [3, 3, 2] {
        let row = doc.add_struct_elem(Some(table), None, "TR", "").unwrap();
        for _ in 0..count {
            doc.add_struct_elem(Some(row), None, "TD", "").unwrap();
        }
    }
}

#[test]
fn tables_get_header_cells_and_spans_in_one_step_each() {
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    doc.set_tagged(true).unwrap();
    bare_table(&mut doc);
    doc.clear_history();
    let r = doc.accessibility_check();
    assert_eq!(status(&r, "table_headers"), CheckStatus::Failed);
    assert_eq!(status(&r, "table_regular"), CheckStatus::Failed);
    assert_eq!(rule("table_headers").unwrap().fix, Fix::Auto);

    assert_eq!(doc.fix_table_headers().unwrap(), 1);
    let cells: Vec<_> = elems(&doc)
        .into_iter()
        .filter(|e| matches!(e.role.as_str(), "TH" | "TD"))
        .collect();
    let roles: Vec<&str> = cells.iter().map(|e| e.role.as_str()).collect();
    assert_eq!(roles, ["TH", "TH", "TH", "TD", "TD", "TD", "TD", "TD"]);
    assert!(
        cells[0].attributes.contains(&"Table/Scope".to_string()),
        "{:?}",
        cells[0].attributes
    );

    // Nothing on the page tells where the cells are: the last cell of the
    // short row spans the column left.
    assert_eq!(doc.fix_table_spans().unwrap(), 1);
    let cells: Vec<_> = elems(&doc)
        .into_iter()
        .filter(|e| matches!(e.role.as_str(), "TH" | "TD"))
        .collect();
    let spanning: Vec<usize> = (0..cells.len())
        .filter(|i| cells[*i].attributes.iter().any(|a| a == "Table/ColSpan"))
        .collect();
    assert_eq!(spanning, [7]);
    let r = doc.accessibility_check();
    assert_eq!(status(&r, "table_headers"), CheckStatus::Passed);
    assert_eq!(status(&r, "table_regular"), CheckStatus::Passed);
    // Done once: nothing more to fix.
    assert_eq!(doc.fix_table_headers().unwrap(), 0);
    assert_eq!(doc.fix_table_spans().unwrap(), 0);
    doc.undo().unwrap();
    assert_eq!(
        status(&doc.accessibility_check(), "table_regular"),
        CheckStatus::Failed
    );
    doc.undo().unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn a_cell_over_two_columns_spans_them() {
    let mut doc = Document::from_csv(
        "Name;Amount;Note\nRossi;12;paid\nBianchi;20;due\n",
        Some(';'),
        TextLayout::default(),
        "",
    )
    .unwrap();
    doc.set_tagged(true).unwrap();
    let place = |doc: &Document, word: &str| {
        let hit = doc.search(word, folitecta_core::SearchOptions::default());
        let r = hit
            .first()
            .unwrap_or_else(|| panic!("{word} is on the page"))
            .rects[0];
        [r[0] - 1.0, r[1] - 1.0, r[2] + 1.0, r[3] + 1.0]
    };
    let table = doc.add_struct_elem(None, None, "Table", "").unwrap();
    for words in [["Name", "Amount", "Note"], ["Rossi", "12", "paid"]] {
        let row = doc.add_struct_elem(Some(table), None, "TR", "").unwrap();
        for w in words {
            let r = place(&doc, w);
            doc.tag_area(0, r, "TD", Some(row), None).unwrap();
        }
    }
    // The last row in two cells: Bianchi and 20 in one, then due.
    let row = doc.add_struct_elem(Some(table), None, "TR", "").unwrap();
    let (a, b) = (place(&doc, "Bianchi"), place(&doc, "20"));
    doc.tag_area(
        0,
        [a[0], a[1].min(b[1]), b[2], a[3].max(b[3])],
        "TD",
        Some(row),
        None,
    )
    .unwrap();
    let due = place(&doc, "due");
    doc.tag_area(0, due, "TD", Some(row), None).unwrap();

    assert_eq!(doc.fix_table_spans().unwrap(), 1);
    let cells: Vec<_> = elems(&doc).into_iter().filter(|e| e.role == "TD").collect();
    assert_eq!(cells.len(), 8);
    let spanning: Vec<usize> = (0..cells.len())
        .filter(|i| cells[*i].attributes.iter().any(|a| a == "Table/ColSpan"))
        .collect();
    // The first cell of the last row, not its last as the order alone would say.
    assert_eq!(spanning, [6]);
}

#[test]
fn tables_get_summaries_asked_for() {
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    doc.set_tagged(true).unwrap();
    bare_table(&mut doc);
    doc.clear_history();
    assert_eq!(rule("table_summary").unwrap().fix, Fix::Ask);
    assert_eq!(
        status(&doc.accessibility_check(), "table_summary"),
        CheckStatus::Failed
    );
    let tables = doc.table_summaries();
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].1, "");
    let id = tables[0].0.id;
    assert_eq!(
        doc.set_table_summaries(&[(id, "Sales by month".into())]).unwrap(),
        1
    );
    assert_eq!(doc.table_summaries()[0].1, "Sales by month");
    assert_eq!(
        status(&doc.accessibility_check(), "table_summary"),
        CheckStatus::Passed
    );
    // The same text again changes nothing; an empty one takes it away.
    assert_eq!(
        doc.set_table_summaries(&[(id, " Sales by month ".into())])
            .unwrap(),
        0
    );
    assert_eq!(doc.set_table_summaries(&[(id, String::new())]).unwrap(), 1);
    assert_eq!(
        status(&doc.accessibility_check(), "table_summary"),
        CheckStatus::Failed
    );
    doc.undo().unwrap();
    assert_eq!(doc.table_summaries()[0].1, "Sales by month");
}
