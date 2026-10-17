use folitecta_core::{Document, LinkTarget};

#[test]
fn links_open_files_go_to_destinations_and_run_scripts() {
    let mut doc = Document::new_blank(3, 300.0, 400.0).unwrap();
    let r = [20.0, 20.0, 120.0, 40.0];
    doc.add_named_destination("Summary", 2, None).unwrap();
    doc.add_named_link(0, r, "Summary").unwrap();
    doc.add_launch_link(0, r, r"C:\Reports\annex.pdf").unwrap();
    doc.add_script_link(0, r, "app.alert('Hello');").unwrap();
    doc.add_page_link(0, r, 1).unwrap();
    let targets: Vec<LinkTarget> = doc.links(0).unwrap().into_iter().map(|l| l.target).collect();
    assert_eq!(targets.len(), 4, "{targets:?}");
    assert!(
        matches!(&targets[0], LinkTarget::Named(n) if n == "Summary") || targets[0] == LinkTarget::Page(2),
        "{:?}",
        targets[0]
    );
    // Shown as typed (the Unicode name of the file specification).
    assert_eq!(targets[1], LinkTarget::Launch(r"C:\Reports\annex.pdf".into()));
    assert_eq!(targets[2], LinkTarget::Other("JavaScript".into()));
    assert_eq!(targets[3], LinkTarget::Page(1));
    // Nothing to point at: refused.
    assert!(doc.add_named_link(0, r, "Missing").is_err());
    assert!(doc.add_launch_link(0, r, "  ").is_err());
    assert!(doc.add_script_link(0, r, "").is_err());
    // Saved and opened again, the links stay.
    let bytes = doc.bytes().as_ref().clone();
    let again = Document::from_bytes(bytes, None, None).unwrap();
    assert_eq!(again.links(0).unwrap().len(), 4);
}
