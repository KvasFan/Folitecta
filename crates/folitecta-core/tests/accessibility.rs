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
