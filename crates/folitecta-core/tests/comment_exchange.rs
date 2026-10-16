//! Comment exchange: XFDF and FDF export/import and import from another PDF.

use folitecta_core::{AnnotStyle, CommentFormat, Document, Markup, Shape};

fn reviewed() -> Document {
    let mut doc = Document::new_blank(2, 400.0, 500.0).unwrap();
    doc.add_note(0, 30.0, 470.0, "Check this & that <now>", "Ann", [1.0, 0.8, 0.2])
        .unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [50.0, 50.0, 150.0, 120.0],
        [0.9, 0.1, 0.1],
        Some([1.0, 1.0, 0.8]),
        2.0,
        "Ann",
        "box",
    )
    .unwrap();
    doc.add_text_markup(
        0,
        Markup::Highlight,
        &[[60.0, 300.0, 200.0, 300.0, 60.0, 288.0, 200.0, 288.0]],
        [1.0, 1.0, 0.0],
        "Bob",
        "",
    )
    .unwrap();
    let style = AnnotStyle {
        color: Some([0.0, 0.3, 0.9]),
        width: 1.5,
        line_end: "ClosedArrow".into(),
        ..AnnotStyle::default()
    };
    doc.add_polyline(1, &[(20.0, 20.0), (200.0, 220.0)], false, &style, "Bob", "arrow")
        .unwrap();
    doc.add_polyline(
        1,
        &[(250.0, 20.0), (350.0, 20.0), (300.0, 90.0)],
        true,
        &style,
        "Bob",
        "",
    )
    .unwrap();
    doc.add_ink(
        1,
        &[vec![(10.0, 400.0), (40.0, 420.0), (80.0, 410.0)]],
        [0.2, 0.6, 0.2],
        2.0,
        "Cy",
    )
    .unwrap();
    doc.add_free_text(
        1,
        [100.0, 300.0, 300.0, 340.0],
        "Typed\nnote",
        "Cy",
        11.0,
        [0.1, 0.1, 0.1],
        true,
    )
    .unwrap();
    doc.add_stamp(
        0,
        [200.0, 400.0, 340.0, 440.0],
        "APPROVED",
        [0.1, 0.5, 0.1],
        "Ann",
    )
    .unwrap();
    doc
}

fn summary(doc: &Document) -> Vec<(usize, String, String, String)> {
    let mut v: Vec<_> = doc
        .comments()
        .into_iter()
        .map(|a| (a.page, a.subtype, a.author, a.contents))
        .collect();
    v.sort();
    v
}

#[test]
fn xfdf_round_trip_recreates_every_comment() {
    let src = reviewed();
    let xfdf = src.export_comments_xfdf(None);
    assert!(xfdf.starts_with("<?xml"), "{xfdf}");
    assert!(xfdf.contains("<square") && xfdf.contains("<ink") && xfdf.contains("<freetext"));
    assert!(xfdf.contains("Check this &amp; that &lt;now&gt;"));
    let mut dst = Document::new_blank(2, 400.0, 500.0).unwrap();
    let n = dst.import_comments(xfdf.as_bytes()).unwrap();
    assert_eq!(n, src.comments().len());
    assert_eq!(summary(&dst), summary(&src));
    // Geometry survives: the rectangle keeps its place.
    let sq = dst
        .comments()
        .into_iter()
        .find(|a| a.subtype == "Square")
        .unwrap();
    assert!(
        (sq.rect[0] - 50.0).abs() < 0.01 && (sq.rect[3] - 120.0).abs() < 0.01,
        "{:?}",
        sq.rect
    );
    // Styles survive.
    let line = dst.comments().into_iter().find(|a| a.subtype == "Line").unwrap();
    let style = dst.annotation_style(line.page, line.index).unwrap();
    assert_eq!(style.line_end, "ClosedArrow");
    assert!((style.width - 1.5).abs() < 0.01);
    // Every imported comment has an appearance and the pages render.
    for a in dst.comments() {
        let d = dst.inner().get_dictionary(a.id.unwrap()).unwrap();
        assert!(d.has(b"AP"), "{} has no appearance", a.subtype);
    }
    dst.render_page_png(0, 1.0).unwrap();
    dst.render_page_png(1, 1.0).unwrap();
    // Importing again replaces instead of duplicating.
    dst.import_comments(xfdf.as_bytes()).unwrap();
    assert_eq!(dst.comments().len(), src.comments().len());
    // The import is one undo step.
    dst.undo().unwrap();
    assert_eq!(dst.comments().len(), src.comments().len());
    dst.undo().unwrap();
    assert!(dst.comments().is_empty());
}

#[test]
fn fdf_round_trip_keeps_appearances() {
    let src = reviewed();
    let fdf = src.export_comments(CommentFormat::Fdf, None).unwrap();
    assert!(fdf.starts_with(b"%FDF-1.2"));
    assert_eq!(CommentFormat::sniff(&fdf), Some(CommentFormat::Fdf));
    let mut dst = Document::new_blank(2, 400.0, 500.0).unwrap();
    let n = dst.import_comments(&fdf).unwrap();
    assert_eq!(n, src.comments().len());
    assert_eq!(summary(&dst), summary(&src));
    let stamp = dst.comments().into_iter().find(|a| a.subtype == "Stamp").unwrap();
    let d = dst.inner().get_dictionary(stamp.id.unwrap()).unwrap();
    assert!(d.has(b"AP"));
    dst.render_page_png(0, 1.0).unwrap();
}

#[test]
fn selected_comments_export_only_those() {
    let src = reviewed();
    let xfdf = src.export_comments_xfdf(Some(&[(0, 1)]));
    assert_eq!(src.exchange_comment_count(Some(&[(0, 1)])), 1);
    let mut dst = Document::new_blank(2, 400.0, 500.0).unwrap();
    assert_eq!(dst.import_comments(xfdf.as_bytes()).unwrap(), 1);
    assert_eq!(dst.comments()[0].subtype, "Square");
}

#[test]
fn comments_of_another_pdf_are_imported_page_by_page() {
    let mut src = reviewed();
    let dir = std::env::temp_dir().join(format!("folitecta-xchg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("reviewed.pdf");
    src.save_copy(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut dst = Document::new_blank(1, 400.0, 500.0).unwrap();
    let n = dst.import_comments(&bytes).unwrap();
    // Only the comments of the first page fit the one-page document.
    assert_eq!(n, src.comments().iter().filter(|a| a.page == 0).count());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn xfdf_from_another_application_with_popups_and_replies() {
    let xfdf = r##"<?xml version="1.0" encoding="UTF-8"?>
<xfdf xmlns="http://ns.adobe.com/xfdf/" xml:space="preserve">
  <annots>
    <text color="#FFFF00" date="D:20260101120000+01'00'" flags="print,nozoom,norotate" name="n1" icon="Comment" page="0" rect="100,400,120,420" subject="Note" title="Reviewer">
      <contents>First line&#13;Second line</contents>
      <popup flags="print,nozoom,norotate" open="yes" page="0" rect="130,300,330,420"/>
    </text>
    <text color="#FFFF00" name="r1" inreplyto="n1" icon="Comment" page="0" rect="100,400,120,420" title="Author" state="Accepted" statemodel="Review">
      <contents>Done</contents>
    </text>
    <square color="#FF0000" interior-color="#00FF00" width="3" style="dash" dashes="4,2" opacity="0.5" name="s1" page="1" rect="10,10,110,60" title="Reviewer"/>
    <polygon color="#0000FF" style="cloudy" intensity="2" name="c1" page="0" rect="0,0,200,200" title="Reviewer">
      <vertices>20,20;180,20;180,180;20,180</vertices>
    </polygon>
    <stamp name="st1" icon="NotApproved" page="0" rect="200,200,350,250" color="#FF0000" title="Boss"/>
    <highlight coords="10,100,90,100,10,88,90,88" color="#FFFF00" name="h1" page="9" rect="10,88,90,100"/>
    <caret symbol="paragraph" name="k1" page="0" rect="50,50,60,60"/>
  </annots>
  <f href="reviewed.pdf"/>
</xfdf>"##;
    let mut doc = Document::new_blank(2, 400.0, 500.0).unwrap();
    // The highlight on page 10 does not fit the document and is skipped.
    assert_eq!(doc.import_comments(xfdf.as_bytes()).unwrap(), 6);
    let inner = doc.inner();
    let comments = doc.comments();
    let note = comments.iter().find(|a| a.contents.starts_with("First")).unwrap();
    assert_eq!(note.contents, "First line\rSecond line");
    assert_eq!(note.author, "Reviewer");
    let note_dict = inner.get_dictionary(note.id.unwrap()).unwrap();
    let popup = note_dict.get(b"Popup").unwrap().as_reference().unwrap();
    let popup_dict = inner.get_dictionary(popup).unwrap();
    assert!(popup_dict.get(b"Open").unwrap().as_bool().unwrap());
    assert_eq!(
        popup_dict.get(b"Parent").unwrap().as_reference().unwrap(),
        note.id.unwrap()
    );
    let reply = comments.iter().find(|a| a.contents == "Done").unwrap();
    let reply_dict = inner.get_dictionary(reply.id.unwrap()).unwrap();
    assert_eq!(
        reply_dict.get(b"IRT").unwrap().as_reference().unwrap(),
        note.id.unwrap()
    );
    let sq = comments.iter().find(|a| a.subtype == "Square").unwrap();
    assert_eq!(sq.page, 1);
    let style = doc.annotation_style(sq.page, sq.index).unwrap();
    assert!(style.dashed && (style.width - 3.0).abs() < 0.01 && (style.opacity - 0.5).abs() < 0.01);
    assert_eq!(style.fill, Some([0.0, 1.0, 0.0]));
    let stamp = comments.iter().find(|a| a.subtype == "Stamp").unwrap();
    assert!(inner.get_dictionary(stamp.id.unwrap()).unwrap().has(b"AP"));
    doc.render_page_png(0, 1.0).unwrap();
    // Export and import again: names and the reply survive.
    let again = doc.export_comments_xfdf(None);
    assert!(
        again.contains("inreplyto=\"n1\"") && again.contains("name=\"st1\""),
        "{again}"
    );
    assert!(again.contains("<popup") && again.contains("style=\"cloudy\""));
}

#[test]
fn malformed_files_are_rejected() {
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    assert!(doc.import_comments(b"<xfdf><annots><square").is_err());
    assert!(doc.import_comments(b"<other/>").is_err());
    assert!(doc.import_comments(b"plain text").is_err());
    assert!(doc.import_comments(b"<xfdf><annots/></xfdf>").is_err());
    assert!(!doc.can_undo());
}
