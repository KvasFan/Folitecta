//! Drawing on a layer: what is drawn or written while a layer is chosen
//! belongs to it (comments by `/OC`, page content by marked content), hidden
//! layers hide it, and comments move between layers. Every result is saved
//! and opened again.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use folitecta_core::calculator::{Tape, TapeLine};
use folitecta_core::stamp_design::{StampDesign, StampValues, TEMPLATES};
use folitecta_core::tables::Table;
use folitecta_core::{
    AnnotStyle, BarcodeKind, Document, EncryptionProfile, Error, FlattenScope, Markup, MeasureKind,
    MeasureScale, RenderOpts, Renderer, SecurityPermissions, Shape, SignatureMark,
};
use lopdf::{Object, ObjectId, dictionary};

const RED: [f32; 3] = [1.0, 0.0, 0.0];

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-layer-drawing-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Save the document and open the file again.
fn reopen(doc: &mut Document, name: &str) -> Document {
    let path = folder(name).join("saved.pdf");
    doc.save_as(&path).unwrap();
    Document::open(&path).unwrap()
}

/// A blank page of 300 × 300 points with one layer, "Ink".
fn with_layer() -> (Document, ObjectId) {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    let (id, _) = doc.add_layer("Ink", &[]).unwrap();
    (doc, id)
}

fn layer_named(doc: &Document, name: &str) -> ObjectId {
    doc.layers().into_iter().find(|l| l.name == name).unwrap().id
}

/// The `/OC` of every annotation of a page (subtype, entry).
fn oc_entries(doc: &Document, page: usize) -> Vec<(String, Option<Object>)> {
    doc.annotations(page)
        .unwrap()
        .into_iter()
        .map(|a| {
            let d = doc.inner().get_dictionary(a.id.unwrap()).unwrap();
            (a.subtype, d.get(b"OC").ok().cloned())
        })
        .collect()
}

fn no_membership_dictionaries(doc: &Document) -> bool {
    doc.inner()
        .objects
        .values()
        .all(|o| !o.as_dict().is_ok_and(|d| d.has_type(b"OCMD")))
}

fn content(doc: &Document) -> String {
    let pid = doc.page_id(0).unwrap();
    String::from_utf8_lossy(&doc.inner().get_page_content(pid)).to_string()
}

/// The pixel at `(x, y)` (PDF space) of page 0 of a 300-point page.
fn pixel(bytes: Arc<Vec<u8>>, x: u32, y: u32) -> [u8; 4] {
    let img = Renderer::new(bytes)
        .unwrap()
        .render_rgba(0, 1.0, RenderOpts::default())
        .unwrap();
    img.get_pixel(x, 300 - y).0
}

fn is_white(p: [u8; 4]) -> bool {
    p[0] > 240 && p[1] > 240 && p[2] > 240
}

fn png(rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(10, 10, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn tone_wav() -> Vec<u8> {
    let pcm: Vec<u8> = (0..400i32)
        .flat_map(|i| (((i as f32 * 0.2).sin() * 8000.0) as i16).to_le_bytes())
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&8000u32.to_le_bytes());
    out.extend_from_slice(&16000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(&pcm);
    out
}

/// One comment of every kind the tools draw or write.
fn draw_everything(doc: &mut Document) -> usize {
    let style = AnnotStyle {
        color: Some(RED),
        ..AnnotStyle::default()
    };
    let quad = [10.0, 290.0, 100.0, 290.0, 10.0, 280.0, 100.0, 280.0];
    doc.add_ink(
        0,
        &[vec![(10.0, 10.0), (50.0, 60.0), (90.0, 20.0)]],
        RED,
        2.0,
        "R",
    )
    .unwrap();
    doc.add_note(0, 120.0, 120.0, "Note", "R", RED).unwrap();
    doc.add_free_text(0, [20.0, 200.0, 120.0, 240.0], "Text box", "R", 12.0, RED, true)
        .unwrap();
    doc.add_free_text(
        0,
        [130.0, 200.0, 230.0, 240.0],
        "Typewriter",
        "R",
        12.0,
        RED,
        false,
    )
    .unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [10.0, 10.0, 60.0, 60.0],
        RED,
        None,
        1.0,
        "R",
        "",
    )
    .unwrap();
    doc.add_shape(
        0,
        Shape::Circle,
        [70.0, 10.0, 120.0, 60.0],
        RED,
        None,
        1.0,
        "R",
        "",
    )
    .unwrap();
    doc.add_line(0, (10.0, 100.0), (100.0, 100.0), RED, 1.0, false, "R", "")
        .unwrap();
    doc.add_line(0, (10.0, 110.0), (100.0, 110.0), RED, 1.0, true, "R", "")
        .unwrap();
    let pts = [(150.0, 10.0), (200.0, 10.0), (200.0, 60.0)];
    doc.add_polyline(0, &pts, true, &style, "R", "").unwrap();
    doc.add_polyline(0, &pts, false, &style, "R", "").unwrap();
    doc.add_cloud(0, &pts, &style, "R").unwrap();
    for kind in [
        Markup::Highlight,
        Markup::Underline,
        Markup::StrikeOut,
        Markup::Squiggly,
    ] {
        doc.add_text_markup(0, kind, &[quad], RED, "R", "").unwrap();
    }
    doc.add_measurement(
        0,
        MeasureKind::Distance,
        &[(10.0, 150.0), (100.0, 150.0)],
        &style,
        &MeasureScale::default(),
        "R",
    )
    .unwrap();
    doc.add_callout(
        0,
        (20.0, 170.0),
        [60.0, 160.0, 160.0, 190.0],
        "Callout",
        &style,
        "R",
    )
    .unwrap();
    doc.add_caret(0, (200.0, 150.0), 12.0, "", RED, "R").unwrap();
    doc.add_replacement(0, &[quad], "new", RED, "R").unwrap();
    doc.add_file_attachment(0, (250.0, 250.0), "a.txt", b"hello".to_vec(), "", "R")
        .unwrap();
    doc.add_sound(0, (260.0, 200.0), &tone_wav(), "R").unwrap();
    doc.add_media(
        0,
        [200.0, 100.0, 280.0, 140.0],
        "clip.mp4",
        b"\x00\x00\x00\x18ftypmp42 not a movie".to_vec(),
        "R",
    )
    .unwrap();
    doc.add_stamp(0, [150.0, 250.0, 250.0, 290.0], "APPROVED", RED, "R")
        .unwrap();
    doc.add_image_stamp(0, [10.0, 250.0, 40.0, 280.0], png([0, 0, 255]), "R", "")
        .unwrap();
    let design = StampDesign::template(TEMPLATES[0]).unwrap();
    doc.add_designed_stamp(
        0,
        [10.0, 10.0, 10.0 + design.width, 10.0 + design.height],
        &design,
        &StampValues::default(),
        "R",
        "Designed",
    )
    .unwrap();
    doc.add_table(0, [150.0, 160.0, 280.0, 200.0], &Table::new(2, 2), "R")
        .unwrap();
    let mut tape = Tape::default();
    tape.lines.push(TapeLine::new('+', 12.0));
    doc.add_calculator(0, [20.0, 280.0, 160.0, 290.0], &tape, "R")
        .unwrap();
    doc.add_signature_mark(
        0,
        [100.0, 60.0, 200.0, 90.0],
        &SignatureMark::Typed {
            text: "Ann".into(),
            color: [0.0, 0.0, 0.0],
        },
        "R",
    )
    .unwrap();
    doc.annotations(0).unwrap().len()
}

#[test]
fn every_kind_of_comment_drawn_goes_to_the_drawing_layer() {
    let (mut doc, ink) = with_layer();
    doc.set_drawing_layer(Some(ink)).unwrap();
    assert_eq!(doc.drawing_layer(), Some(ink));
    let undo_before = doc.can_undo();
    let count = draw_everything(&mut doc);
    assert!(undo_before && count >= 28, "{count} comments");
    assert_eq!(doc.take_layer_drawn(), Some(ink));
    assert_eq!(doc.take_layer_drawn(), None);
    let mut back = reopen(&mut doc, "kinds");
    let layer = layer_named(&back, "Ink");
    let entries = oc_entries(&back, 0);
    assert_eq!(entries.len(), count);
    for (subtype, oc) in &entries {
        assert_eq!(
            oc.as_ref().and_then(|o| o.as_reference().ok()),
            Some(layer),
            "{subtype}"
        );
    }
    // A layer named directly: no membership dictionary is made.
    assert!(no_membership_dictionaries(&back));
    let d = back.inner().get_dictionary(layer).unwrap();
    assert!(d.has_type(b"OCG"));
    // The drawing layer is a choice of the view: not saved.
    assert_eq!(back.drawing_layer(), None);
    back.add_ink(0, &[vec![(1.0, 1.0), (5.0, 5.0)]], RED, 1.0, "R")
        .unwrap();
    let last = oc_entries(&back, 0).pop().unwrap();
    assert_eq!(last.1, None);
}

#[test]
fn without_a_drawing_layer_nothing_goes_to_a_layer() {
    let (mut doc, _) = with_layer();
    doc.add_ink(0, &[vec![(10.0, 10.0), (50.0, 60.0)]], RED, 2.0, "R")
        .unwrap();
    doc.add_image(0, png([0, 0, 255]), Some([20.0, 20.0, 80.0, 80.0]))
        .unwrap();
    let back = reopen(&mut doc, "none");
    assert!(oc_entries(&back, 0).iter().all(|(_, oc)| oc.is_none()));
    assert!(!content(&back).contains("BDC"));
    assert_eq!(doc.take_layer_drawn(), None);
}

#[test]
fn marks_for_redaction_replies_and_states() {
    let (mut doc, ink) = with_layer();
    doc.set_drawing_layer(Some(ink)).unwrap();
    // A mark for redaction never hides on a layer.
    doc.add_redaction(0, [10.0, 10.0, 50.0, 50.0], "R").unwrap();
    doc.add_note(0, 100.0, 100.0, "Note", "R", RED).unwrap();
    // Replies, states and pop-up notes follow their comment, whatever the drawing layer.
    doc.set_drawing_layer(None).unwrap();
    doc.add_reply(0, 1, "Reply", "R").unwrap();
    doc.set_review_state(0, 1, "Review", "Accepted", "R", "").unwrap();
    doc.set_popup_open(0, 1, true).unwrap();
    let back = reopen(&mut doc, "replies");
    let layer = layer_named(&back, "Ink");
    let entries = oc_entries(&back, 0);
    assert_eq!(entries[0].0, "Redact");
    assert_eq!(entries[0].1, None);
    for (subtype, oc) in &entries[1..] {
        assert_eq!(
            oc.as_ref().and_then(|o| o.as_reference().ok()),
            Some(layer),
            "{subtype}"
        );
    }
    assert!(entries.iter().any(|(s, _)| s == "Popup"));
}

#[test]
fn page_content_drawn_on_a_layer_is_marked_content() {
    let (mut doc, ink) = with_layer();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_image(0, png([0, 0, 255]), Some([20.0, 20.0, 80.0, 80.0]))
        .unwrap();
    doc.add_barcode(0, [100.0, 100.0, 180.0, 180.0], BarcodeKind::Qr, "layer", false)
        .unwrap();
    let back = reopen(&mut doc, "content");
    let layer = layer_named(&back, "Ink");
    let text = content(&back);
    assert_eq!(text.matches("BDC").count(), 2, "{text}");
    assert_eq!(text.matches("EMC").count(), 2, "{text}");
    // Every sequence opens before it closes.
    let mut depth = 0i32;
    for word in text.split_whitespace() {
        match word {
            "BDC" | "BMC" => depth += 1,
            "EMC" => {
                depth -= 1;
                assert!(depth >= 0);
            }
            _ => {}
        }
    }
    assert_eq!(depth, 0);
    let name = format!("FoliOC{}", ink.0);
    assert!(text.contains(&format!("/OC /{name} BDC")), "{text}");
    let pid = back.page_id(0).unwrap();
    let res = back
        .inner()
        .get_dictionary(pid)
        .unwrap()
        .get(b"Resources")
        .unwrap();
    let res = match res {
        Object::Reference(r) => back.inner().get_dictionary(*r).unwrap(),
        o => o.as_dict().unwrap(),
    };
    let props = res.get(b"Properties").unwrap().as_dict().unwrap();
    assert_eq!(props.get(name.as_bytes()).unwrap().as_reference().unwrap(), layer);
    // The page lists the layer as used.
    assert!(back.layers_on_page(0).contains(&layer));
    assert!(no_membership_dictionaries(&back));
}

#[test]
fn hidden_layers_hide_what_was_drawn_on_them() {
    let (mut doc, ink) = with_layer();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [100.0, 100.0, 200.0, 200.0],
        RED,
        Some(RED),
        2.0,
        "R",
        "",
    )
    .unwrap();
    doc.add_image(0, png([0, 0, 255]), Some([20.0, 20.0, 80.0, 80.0]))
        .unwrap();
    let mut back = reopen(&mut doc, "visible");
    let layer = layer_named(&back, "Ink");
    let bytes = back.render_bytes().unwrap();
    assert!(!is_white(pixel(bytes.clone(), 150, 150)));
    assert!(!is_white(pixel(bytes, 50, 50)));
    // Hidden in the view only.
    let view = HashMap::from([(layer, false)]);
    let shown = back.view_bytes(&HashSet::new(), &view).unwrap();
    assert!(is_white(pixel(shown.clone(), 150, 150)));
    assert!(is_white(pixel(shown, 50, 50)));
    assert_eq!(back.annotations_in_hidden_layers(&view).len(), 1);
    assert_eq!(
        back.page_annotations_in_hidden_layers(0, &view),
        HashSet::from([0])
    );
    // Hidden by default: the pictures of the pages and the comments pane follow.
    back.set_layer_visible(layer, false).unwrap();
    let hidden = reopen(&mut back, "hidden");
    let bytes = hidden.render_bytes().unwrap();
    assert!(is_white(pixel(bytes.clone(), 150, 150)));
    assert!(is_white(pixel(bytes, 50, 50)));
    let infos = hidden.comment_infos();
    assert!(infos[0].in_hidden_layer);
    assert_eq!(infos[0].layer, Some(layer));
    let dir = folder("export");
    let files = hidden
        .export_pages_images(&[0], &dir, 72.0, folitecta_core::ImageFormat::Png, "p")
        .unwrap();
    let img = image::open(&files[0]).unwrap().to_rgba8();
    assert!(is_white(img.get_pixel(150, 150).0));
    // Shown again in the view.
    let shown = hidden
        .view_bytes(&HashSet::new(), &HashMap::from([(layer, true)]))
        .unwrap();
    assert!(!is_white(pixel(shown, 150, 150)));
    assert!(
        hidden
            .annotations_in_hidden_layers(&HashMap::from([(layer, true)]))
            .is_empty()
    );
}

#[test]
fn flattened_comments_stay_on_their_layer() {
    let (mut doc, ink) = with_layer();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [100.0, 100.0, 200.0, 200.0],
        RED,
        Some(RED),
        2.0,
        "R",
        "",
    )
    .unwrap();
    doc.set_drawing_layer(None).unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [10.0, 10.0, 60.0, 60.0],
        RED,
        Some(RED),
        2.0,
        "R",
        "",
    )
    .unwrap();
    assert_eq!(doc.flatten(&[0], FlattenScope::Comments).unwrap(), 2);
    let mut back = reopen(&mut doc, "flatten");
    let layer = layer_named(&back, "Ink");
    assert!(back.annotations(0).unwrap().is_empty());
    let text = content(&back);
    assert_eq!(
        text.matches(&format!("/OC /FoliOC{} BDC", ink.0)).count(),
        1,
        "{text}"
    );
    assert_eq!(text.matches("EMC").count(), 1);
    let bytes = back.render_bytes().unwrap();
    assert!(!is_white(pixel(bytes.clone(), 150, 150)));
    assert!(!is_white(pixel(bytes, 30, 30)));
    back.set_layer_visible(layer, false).unwrap();
    let back = reopen(&mut back, "flatten-hidden");
    let bytes = back.render_bytes().unwrap();
    assert!(is_white(pixel(bytes.clone(), 150, 150)));
    // The comment that was on no layer stays visible.
    assert!(!is_white(pixel(bytes, 30, 30)));
}

#[test]
fn undo_and_redo_restore_what_was_drawn() {
    let (mut doc, ink) = with_layer();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_ink(0, &[vec![(10.0, 10.0), (50.0, 60.0)]], RED, 2.0, "R")
        .unwrap();
    doc.add_image(0, png([0, 0, 255]), Some([20.0, 20.0, 80.0, 80.0]))
        .unwrap();
    doc.undo().unwrap();
    assert!(!content(&doc).contains("BDC"));
    doc.undo().unwrap();
    assert!(doc.annotations(0).unwrap().is_empty());
    // The layer is still there: it stays the drawing layer.
    assert_eq!(doc.drawing_layer(), Some(ink));
    doc.redo().unwrap();
    doc.redo().unwrap();
    let back = reopen(&mut doc, "undo");
    let layer = layer_named(&back, "Ink");
    assert_eq!(
        oc_entries(&back, 0)[0]
            .1
            .as_ref()
            .and_then(|o| o.as_reference().ok()),
        Some(layer)
    );
    assert!(content(&back).contains("BDC"));
    // Undoing the layer itself forgets it as the drawing layer.
    doc.undo().unwrap();
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert!(doc.layers().is_empty());
    assert_eq!(doc.drawing_layer(), None);
    doc.redo().unwrap();
    assert_eq!(doc.drawing_layer(), None);
}

#[test]
fn merged_layers_are_no_longer_drawn_on_and_locks_do_not_matter() {
    let (mut doc, ink) = with_layer();
    let (other, _) = doc.add_layer("Other", &[]).unwrap();
    doc.set_layers_locked(&[ink], true).unwrap();
    doc.set_drawing_layer(Some(ink)).unwrap();
    // A locked layer only keeps readers from showing or hiding it.
    doc.add_ink(0, &[vec![(10.0, 10.0), (50.0, 60.0)]], RED, 2.0, "R")
        .unwrap();
    assert_eq!(doc.annotation_layer(0, 0), Some(ink));
    doc.merge_layers(&[ink], other).unwrap();
    assert_eq!(doc.drawing_layer(), None);
    assert_eq!(doc.annotation_layer(0, 0), Some(other));
    // A layer that does not exist cannot be chosen.
    assert!(matches!(doc.set_drawing_layer(Some(ink)), Err(Error::Invalid(_))));
    assert!(doc.set_drawing_layer(Some((9999, 0))).is_err());
}

#[test]
fn comments_move_between_layers() {
    let (mut doc, ink) = with_layer();
    let (notes, _) = doc.add_layer("Notes", &[]).unwrap();
    doc.add_note(0, 100.0, 100.0, "Note", "R", RED).unwrap();
    doc.add_ink(0, &[vec![(10.0, 10.0), (50.0, 60.0)]], RED, 2.0, "R")
        .unwrap();
    doc.add_reply(0, 0, "Reply", "R").unwrap();
    doc.set_popup_open(0, 0, true).unwrap();
    assert_eq!(doc.set_annotations_layer(0, &[0, 1], Some(ink)).unwrap(), 2);
    let back = reopen(&mut doc, "moved");
    let layer = layer_named(&back, "Ink");
    assert!(
        oc_entries(&back, 0)
            .iter()
            .all(|(_, oc)| oc.as_ref().and_then(|o| o.as_reference().ok()) == Some(layer))
    );
    // One comment to another layer, then to none.
    doc.set_annotations_layer(0, &[0], Some(notes)).unwrap();
    assert_eq!(doc.annotation_layer(0, 0), Some(notes));
    assert_eq!(doc.annotation_layer(0, 1), Some(ink));
    // Its reply and pop-up note follow it.
    let entries = oc_entries(&doc, 0);
    assert_eq!(
        entries[2].1.as_ref().and_then(|o| o.as_reference().ok()),
        Some(notes)
    );
    assert_eq!(
        entries[3].1.as_ref().and_then(|o| o.as_reference().ok()),
        Some(notes)
    );
    doc.set_annotations_layer(0, &[0], None).unwrap();
    let back = reopen(&mut doc, "none-again");
    let entries = oc_entries(&back, 0);
    assert_eq!(entries[0].1, None);
    assert_eq!(entries[2].1, None);
    assert!(entries[1].1.is_some());
    // One undo step each.
    doc.undo().unwrap();
    assert_eq!(doc.annotation_layer(0, 0), Some(notes));
    // Fields, links and missing comments are refused.
    assert!(doc.set_annotations_layer(0, &[9], Some(ink)).is_err());
    assert!(doc.set_annotations_layer(0, &[], Some(ink)).is_err());
    assert!(doc.set_annotations_layer(0, &[0], Some((9999, 0))).is_err());
}

#[test]
fn comments_move_to_a_new_layer_in_one_step() {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    doc.add_note(0, 100.0, 100.0, "Note", "R", RED).unwrap();
    doc.add_ink(0, &[vec![(10.0, 10.0), (50.0, 60.0)]], RED, 2.0, "R")
        .unwrap();
    assert!(doc.move_annotations_to_new_layer(0, &[0], "  ").is_err());
    let id = doc.move_annotations_to_new_layer(0, &[0, 1], "Moved").unwrap();
    let back = reopen(&mut doc, "new-layer");
    let layer = layer_named(&back, "Moved");
    assert_eq!(back.layers().len(), 1);
    assert!(
        oc_entries(&back, 0)
            .iter()
            .all(|(_, oc)| oc.as_ref().and_then(|o| o.as_reference().ok()) == Some(layer))
    );
    assert!(no_membership_dictionaries(&back));
    doc.undo().unwrap();
    assert!(doc.layers().iter().all(|l| l.id != id));
    assert!(oc_entries(&doc, 0).iter().all(|(_, oc)| oc.is_none()));
}

#[test]
fn pasted_comments_keep_or_take_a_layer() {
    let (mut doc, ink) = with_layer();
    let (notes, _) = doc.add_layer("Notes", &[]).unwrap();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_ink(0, &[vec![(10.0, 10.0), (50.0, 60.0)]], RED, 2.0, "R")
        .unwrap();
    let ocgs = |d: &Document| {
        d.inner()
            .objects
            .values()
            .filter(|o| o.as_dict().is_ok_and(|d| d.has_type(b"OCG")))
            .count()
    };
    let before = ocgs(&doc);
    let snap = doc.annotation_snapshot(0, 0).unwrap();
    // Without a drawing layer the copy stays on the layer of the comment,
    // which is not copied.
    doc.set_drawing_layer(None).unwrap();
    doc.paste_annotation_snapshot(0, &snap, 5.0, 5.0).unwrap();
    assert_eq!(doc.annotation_layer(0, 1), Some(ink));
    assert_eq!(ocgs(&doc), before);
    // With one, the copy goes to it.
    doc.set_drawing_layer(Some(notes)).unwrap();
    doc.duplicate_annotation(0, 0, 0, 10.0, 10.0).unwrap();
    assert_eq!(doc.annotation_layer(0, 2), Some(notes));
    // Another document: the layer of the same name, or none.
    let mut other = Document::new_blank(1, 300.0, 300.0).unwrap();
    other.paste_annotation_snapshot(0, &snap, 0.0, 0.0).unwrap();
    assert_eq!(other.annotation_layer(0, 0), None);
    let (same, _) = other.add_layer("Ink", &[]).unwrap();
    other.paste_annotation_snapshot(0, &snap, 0.0, 0.0).unwrap();
    assert_eq!(other.annotation_layer(0, 1), Some(same));
    let back = reopen(&mut other, "pasted");
    assert_eq!(back.layers().len(), 1);
    assert!(back.annotation_layer(0, 0).is_none());
    assert_eq!(back.annotation_layer(0, 1), Some(layer_named(&back, "Ink")));
}

#[test]
fn membership_dictionaries_decide_what_they_hide() {
    let (mut doc, ink) = with_layer();
    let (notes, _) = doc.add_layer("Notes", &[]).unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [100.0, 100.0, 200.0, 200.0],
        RED,
        Some(RED),
        2.0,
        "R",
        "",
    )
    .unwrap();
    // A comment shown when every one of its layers is on.
    let aid = doc.annotations(0).unwrap()[0].id.unwrap();
    let ocmd = doc.inner_mut().add_object(dictionary! {
        "Type" => "OCMD",
        "OCGs" => vec![Object::Reference(ink), Object::Reference(notes)],
        "P" => "AllOn",
    });
    doc.inner_mut()
        .get_dictionary_mut(aid)
        .unwrap()
        .set("OC", Object::Reference(ocmd));
    doc.commit().unwrap();
    assert_eq!(doc.annotation_layers(0, 0), vec![ink, notes]);
    assert_eq!(doc.annotation_layer(0, 0), None);
    assert!(doc.annotations_in_hidden_layers(&HashMap::new()).is_empty());
    let one_off = HashMap::from([(notes, false)]);
    assert_eq!(doc.annotations_in_hidden_layers(&one_off), HashSet::from([aid]));
    let shown = doc.view_bytes(&HashSet::new(), &one_off).unwrap();
    assert!(is_white(pixel(shown, 150, 150)));
    // Flattened, it keeps the membership dictionary.
    doc.flatten(&[0], FlattenScope::Comments).unwrap();
    doc.set_layer_visible(notes, false).unwrap();
    let back = reopen(&mut doc, "ocmd");
    assert!(content(&back).contains("BDC"));
    assert!(is_white(pixel(back.render_bytes().unwrap(), 150, 150)));
}

#[test]
fn protected_documents_stay_protected() {
    let dir = folder("protected");
    let original = dir.join("protected.pdf");
    Document::new_blank(1, 300.0, 300.0)
        .unwrap()
        .save_encrypted(
            &original,
            "user-pw",
            "owner-pw",
            SecurityPermissions::default(),
            EncryptionProfile::Aes256,
        )
        .unwrap();
    let mut doc = Document::open_with_password(&original, "user-pw").unwrap();
    let (ink, _) = doc.add_layer("Secret layer", &[]).unwrap();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_ink(0, &[vec![(10.0, 10.0), (50.0, 60.0)]], RED, 2.0, "R")
        .unwrap();
    doc.add_image(0, png([0, 0, 255]), Some([20.0, 20.0, 80.0, 80.0]))
        .unwrap();
    doc.move_annotations_to_new_layer(0, &[0], "Moved").unwrap();
    let saved = dir.join("saved.pdf");
    doc.save_as(&saved).unwrap();
    // Still encrypted: no password, no document.
    assert!(matches!(Document::open(&saved), Err(Error::PasswordRequired)));
    let raw = std::fs::read(&saved).unwrap();
    assert!(!raw.windows(6).any(|w| w == b"Secret"), "a layer name in clear");
    let back = Document::open_with_password(&saved, "user-pw").unwrap();
    assert!(back.is_protected());
    assert_eq!(back.annotation_layer(0, 0), Some(layer_named(&back, "Moved")));
    assert!(
        back.layers_on_page(0)
            .contains(&layer_named(&back, "Secret layer"))
    );
    assert!(content(&back).contains("/OC /FoliOC"));
}

#[test]
fn pictures_move_to_a_new_layer() {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    doc.add_image(0, png([0, 0, 255]), Some([20.0, 20.0, 80.0, 80.0]))
        .unwrap();
    assert!(doc.move_picture_to_new_layer(0, 3, "Pictures").is_err());
    let id = doc.move_picture_to_new_layer(0, 0, "Pictures").unwrap();
    assert_eq!(
        content(&doc).matches(&format!("/OC /FoliOC{} BDC", id.0)).count(),
        1
    );
    // A picture placed on a layer changes layer: no second sequence.
    let (ink, _) = doc.add_layer("Ink", &[]).unwrap();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_image(0, png([255, 0, 0]), Some([150.0, 150.0, 250.0, 250.0]))
        .unwrap();
    doc.set_drawing_layer(None).unwrap();
    let moved = doc.move_picture_to_new_layer(0, 1, "Red").unwrap();
    let text = content(&doc);
    assert_eq!(text.matches("BDC").count(), 2, "{text}");
    assert!(text.contains(&format!("/OC /FoliOC{} BDC", moved.0)), "{text}");
    assert!(!text.contains(&format!("/OC /FoliOC{} BDC", ink.0)), "{text}");
    let mut back = reopen(&mut doc, "pictures");
    let (pictures, red) = (layer_named(&back, "Pictures"), layer_named(&back, "Red"));
    let bytes = back.render_bytes().unwrap();
    assert!(!is_white(pixel(bytes.clone(), 50, 50)));
    assert!(!is_white(pixel(bytes, 200, 200)));
    back.set_layer_visible(pictures, false).unwrap();
    let bytes = back.render_bytes().unwrap();
    assert!(is_white(pixel(bytes.clone(), 50, 50)));
    assert!(!is_white(pixel(bytes, 200, 200)));
    back.set_layer_visible(red, false).unwrap();
    assert!(is_white(pixel(back.render_bytes().unwrap(), 200, 200)));
    // One undo step: the layer and the change go together.
    doc.undo().unwrap();
    assert!(doc.layers().iter().all(|l| l.name != "Red"));
    let text = content(&doc);
    assert!(text.contains(&format!("/OC /FoliOC{} BDC", ink.0)), "{text}");
}

/// Every layer object of the document is listed in its optional content
/// properties (no layer copied on its own).
fn every_ocg_listed(doc: &Document) -> bool {
    let listed: HashSet<ObjectId> = doc.layers().iter().map(|l| l.id).collect();
    doc.inner()
        .objects
        .iter()
        .filter(|(_, o)| o.as_dict().is_ok_and(|d| d.has_type(b"OCG")))
        .all(|(id, _)| listed.contains(id))
}

fn drawn_on_ink() -> (Document, ObjectId) {
    let (mut doc, ink) = with_layer();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [100.0, 100.0, 200.0, 200.0],
        RED,
        Some(RED),
        2.0,
        "R",
        "x",
    )
    .unwrap();
    doc.set_popup_open(0, 0, true).unwrap();
    doc.set_drawing_layer(None).unwrap();
    (doc, ink)
}

#[test]
fn fdf_and_pdf_exchanges_carry_the_layer_by_name() {
    let (mut doc, ink) = drawn_on_ink();
    let fdf = doc.export_comments_fdf(None).unwrap();
    let text = String::from_utf8_lossy(&fdf).to_string();
    assert!(!text.contains("/OCG"), "the layer object travelled: {text}");
    assert!(
        text.replace("/FolitectaLayer (", "/FolitectaLayer(")
            .contains("/FolitectaLayer(Ink)"),
        "{text}"
    );
    // Into another document with a layer of that name.
    let mut other = Document::new_blank(1, 300.0, 300.0).unwrap();
    let (theirs, _) = other.add_layer("Ink", &[]).unwrap();
    assert_eq!(other.import_comments_fdf(&fdf).unwrap(), 1);
    assert_eq!(other.annotation_layer(0, 0), Some(theirs));
    assert!(every_ocg_listed(&other));
    other.set_layer_visible(theirs, false).unwrap();
    let back = reopen(&mut other, "fdf-other");
    assert!(every_ocg_listed(&back));
    assert!(is_white(pixel(back.render_bytes().unwrap(), 150, 150)));
    // The pop-up note follows its comment.
    let popup = oc_entries(&back, 0)
        .into_iter()
        .find(|(s, _)| s == "Popup")
        .unwrap();
    assert_eq!(
        popup.1.and_then(|o| o.as_reference().ok()),
        Some(layer_named(&back, "Ink"))
    );
    // Into a document without it: the layer is added.
    let mut bare = Document::new_blank(1, 300.0, 300.0).unwrap();
    bare.import_comments(&fdf).unwrap();
    let back = reopen(&mut bare, "fdf-bare");
    assert_eq!(back.layers().len(), 1);
    assert_eq!(back.annotation_layer(0, 0), Some(layer_named(&back, "Ink")));
    assert!(every_ocg_listed(&back));
    // Into the same document: updated in place, on the same layer.
    doc.import_comments_fdf(&fdf).unwrap();
    assert_eq!(doc.layers().len(), 1);
    assert_eq!(doc.annotation_layer(0, 0), Some(ink));
    assert_eq!(doc.annotations(0).unwrap().len(), 2);
    assert!(every_ocg_listed(&doc));
    // Comments of another PDF: their layer is matched by name as well.
    let path = folder("pdf-source").join("source.pdf");
    doc.save_as(&path).unwrap();
    let mut target = Document::new_blank(1, 300.0, 300.0).unwrap();
    let (mine, _) = target.add_layer("Ink", &[]).unwrap();
    target
        .import_comments_pdf(&std::fs::read(&path).unwrap())
        .unwrap();
    assert_eq!(target.annotation_layer(0, 0), Some(mine));
    assert_eq!(target.layers().len(), 1);
    assert!(every_ocg_listed(&target));
}

#[test]
fn xfdf_imports_keep_the_layer_of_a_comment_updated_in_place() {
    let (mut doc, ink) = drawn_on_ink();
    let xfdf = doc.export_comments_xfdf(None);
    assert_eq!(doc.import_comments_xfdf(&xfdf).unwrap(), 1);
    assert_eq!(doc.annotation_layer(0, 0), Some(ink));
    let back = reopen(&mut doc, "xfdf");
    assert_eq!(back.annotation_layer(0, 0), Some(layer_named(&back, "Ink")));
    assert!(every_ocg_listed(&back));
    // A new comment from XFDF goes to no layer (the format names none).
    let mut other = Document::new_blank(1, 300.0, 300.0).unwrap();
    other.add_layer("Ink", &[]).unwrap();
    other.import_comments_xfdf(&xfdf).unwrap();
    assert_eq!(other.annotation_layer(0, 0), None);
}

#[test]
fn views_cost_nothing_without_layers_and_are_kept_per_revision() {
    // No layers: the bytes of the document themselves.
    let mut plain = Document::new_blank(2, 300.0, 300.0).unwrap();
    plain
        .add_ink(0, &[vec![(10.0, 10.0), (50.0, 60.0)]], RED, 2.0, "R")
        .unwrap();
    let bytes = plain.bytes();
    assert!(Arc::ptr_eq(&plain.render_bytes().unwrap(), &bytes));
    assert!(Arc::ptr_eq(
        &plain.view_bytes(&HashSet::new(), &HashMap::new()).unwrap(),
        &bytes
    ));
    assert!(plain.annotations_in_hidden_layers(&HashMap::new()).is_empty());
    // Layers but nothing they hide: the same.
    let (mut doc, ink) = with_layer();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_ink(0, &[vec![(10.0, 10.0), (50.0, 60.0)]], RED, 2.0, "R")
        .unwrap();
    assert!(Arc::ptr_eq(&doc.render_bytes().unwrap(), &doc.bytes()));
    // Something hidden: an update appended to the bytes, kept for the revision.
    let view = HashMap::from([(ink, false)]);
    let first = doc.view_bytes(&HashSet::new(), &view).unwrap();
    assert!(
        first.starts_with(&doc.bytes()),
        "the view is not an update of the bytes"
    );
    assert!(Arc::ptr_eq(
        &first,
        &doc.view_bytes(&HashSet::new(), &view).unwrap()
    ));
    doc.set_layer_visible(ink, false).unwrap();
    let hidden = doc.render_bytes().unwrap();
    assert!(!Arc::ptr_eq(&first, &hidden));
    assert!(Arc::ptr_eq(&hidden, &doc.render_bytes().unwrap()));
    // A view showing the layer again changes its default state in the update.
    let shown = doc
        .view_bytes(&HashSet::new(), &HashMap::from([(ink, true)]))
        .unwrap();
    assert!(shown.starts_with(&doc.bytes()));
    let reread = Document::from_bytes(shown.to_vec(), None, None).unwrap();
    assert!(reread.layers()[0].visible);
}

/// A document of `pages` pages with 20 square comments each on 20 layers,
/// the first layer hidden by default.
fn many_layered_comments(pages: usize) -> Document {
    use lopdf::Stream;
    let mut d = lopdf::Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let ocgs: Vec<ObjectId> = (0..20)
        .map(|i| {
            d.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal(format!("L{i}")) })
        })
        .collect();
    let ap = d.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()] },
        b"1 0 0 rg 0 0 10 10 re f".to_vec(),
    ));
    let mut kids = Vec::new();
    for _ in 0..pages {
        let annots: Vec<Object> = (0..20)
            .map(|k| {
                let x = (k % 10) as f32 * 30.0 + 10.0;
                let y = (k / 10) as f32 * 30.0 + 100.0;
                Object::Reference(d.add_object(dictionary! {
                    "Type" => "Annot", "Subtype" => "Square", "F" => 4,
                    "Rect" => vec![x.into(), y.into(), (x + 10.0).into(), (y + 10.0).into()],
                    "AP" => dictionary! { "N" => ap }, "OC" => ocgs[k % 20],
                }))
            })
            .collect();
        kids.push(Object::Reference(d.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Annots" => annots,
            "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
        })));
    }
    let count = kids.len() as i64;
    d.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let refs: Vec<Object> = ocgs.iter().map(|o| Object::Reference(*o)).collect();
    let catalog = d.add_object(dictionary! {
        "Type" => "Catalog", "Pages" => pages_id,
        "OCProperties" => dictionary! { "OCGs" => refs.clone(), "D" => dictionary! { "Order" => refs, "OFF" => vec![Object::Reference(ocgs[0])] } },
    });
    d.trailer.set("Root", catalog);
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    Document::from_bytes(out, None, None).unwrap()
}

#[test]
fn views_of_large_layered_documents_stay_quick() {
    let doc = many_layered_comments(600);
    let started = std::time::Instant::now();
    let first = doc.render_bytes().unwrap();
    let made = started.elapsed();
    assert_eq!(doc.annotations_in_hidden_layers(&HashMap::new()).len(), 600);
    let again = std::time::Instant::now();
    for _ in 0..20 {
        assert!(Arc::ptr_eq(&first, &doc.render_bytes().unwrap()));
        doc.annotations_in_hidden_layers(&HashMap::new());
    }
    let kept = again.elapsed();
    // Generous bounds: an update of 600 comments, then views kept.
    assert!(made.as_millis() < 1500, "first view took {made:?}");
    assert!(kept.as_millis() < 500, "20 kept views took {kept:?}");
    assert!(first.len() < doc.bytes().len() * 2);
    // What the update hides is hidden, the rest is drawn.
    assert!(is_white(pixel(first.clone(), 15, 105)));
    assert!(!is_white(pixel(first, 45, 105)));
}

/// Two pages drawing one picture with the same content stream; their
/// resources are one object, or inherited from the page tree with one
/// `/Properties` object.
fn shared_pages(inherited: bool) -> Document {
    use lopdf::Stream;
    let mut d = lopdf::Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let img = d.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 },
        vec![0, 0, 255],
    ));
    let other = d.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Other") });
    let props = d.add_object(dictionary! { "Old" => other });
    let res = d.add_object(dictionary! { "XObject" => dictionary! { "Im0" => img }, "Properties" => props });
    let content = d.add_object(Stream::new(
        dictionary! {},
        b"q 100 0 0 100 50 50 cm /Im0 Do Q".to_vec(),
    ));
    let kids: Vec<Object> = (0..2)
        .map(|_| {
            let mut page = dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => content };
            if !inherited {
                page.set("Resources", res);
            }
            Object::Reference(d.add_object(page))
        })
        .collect();
    let mut pages = dictionary! {
        "Type" => "Pages", "Kids" => kids, "Count" => 2,
        "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
    };
    if inherited {
        pages.set("Resources", res);
    }
    d.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog = d.add_object(dictionary! {
        "Type" => "Catalog", "Pages" => pages_id,
        "OCProperties" => dictionary! { "OCGs" => vec![Object::Reference(other)], "D" => dictionary! { "Order" => vec![Object::Reference(other)] } },
    });
    d.trailer.set("Root", catalog);
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    Document::from_bytes(out, None, None).unwrap()
}

fn page_pixel(bytes: Arc<Vec<u8>>, page: usize, x: u32, y: u32) -> [u8; 4] {
    let img = Renderer::new(bytes)
        .unwrap()
        .render_rgba(page, 1.0, RenderOpts::default())
        .unwrap();
    img.get_pixel(x, 300 - y).0
}

#[test]
fn shared_resources_and_content_stay_with_the_page_drawn_on() {
    for inherited in [false, true] {
        let mut doc = shared_pages(inherited);
        let (ink, _) = doc.add_layer("Ink", &[]).unwrap();
        doc.set_drawing_layer(Some(ink)).unwrap();
        doc.add_image(0, png([255, 0, 0]), Some([200.0, 200.0, 250.0, 250.0]))
            .unwrap();
        let back = reopen(&mut doc, if inherited { "inherited" } else { "shared" });
        let ink = layer_named(&back, "Ink");
        assert!(back.layers_on_page(0).contains(&ink), "inherited {inherited}");
        assert!(!back.layers_on_page(1).contains(&ink), "inherited {inherited}");
        assert_eq!(back.layer_pages().get(&ink), Some(&vec![0]));
        assert!(!content_of_page(&back, 1).contains("BDC"));
        // The picture both pages draw, put on a new layer on page 0 only.
        let mut doc = shared_pages(inherited);
        let moved = doc.move_picture_to_new_layer(0, 0, "Moved").unwrap();
        assert!(content_of_page(&doc, 0).contains("BDC"));
        assert!(!content_of_page(&doc, 1).contains("BDC"));
        assert!(!doc.layers_on_page(1).contains(&moved));
        let view = doc
            .view_bytes(&HashSet::new(), &HashMap::from([(moved, false)]))
            .unwrap();
        assert!(is_white(page_pixel(view.clone(), 0, 100, 100)));
        assert!(!is_white(page_pixel(view, 1, 100, 100)));
    }
}

fn content_of_page(doc: &Document, page: usize) -> String {
    let pid = doc.page_id(page).unwrap();
    String::from_utf8_lossy(&doc.inner().get_page_content(pid)).to_string()
}

#[test]
fn rasterized_pages_keep_the_comments_of_hidden_layers() {
    let (mut doc, ink) = with_layer();
    doc.set_layer_visible(ink, false).unwrap();
    doc.set_drawing_layer(Some(ink)).unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [10.0, 10.0, 90.0, 90.0],
        RED,
        Some(RED),
        2.0,
        "R",
        "hidden",
    )
    .unwrap();
    doc.set_drawing_layer(None).unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [110.0, 10.0, 190.0, 90.0],
        RED,
        Some(RED),
        2.0,
        "R",
        "shown",
    )
    .unwrap();
    // The default visibility: the visible comment goes into the picture, the
    // other stays a comment on its layer.
    let mut plain = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    plain.rasterize_pages(&[0], 72.0, true).unwrap();
    let back = reopen(&mut plain, "raster-default");
    let left = back.annotations(0).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].contents, "hidden");
    assert_eq!(back.annotation_layer(0, 0), Some(layer_named(&back, "Ink")));
    let bytes = back.render_bytes().unwrap();
    assert!(!is_white(pixel(bytes.clone(), 150, 50)));
    assert!(is_white(pixel(bytes, 50, 50)));
    // As a view showing the layer: both go into the picture.
    doc.rasterize_pages_with_view(&[0], 72.0, true, &HashMap::from([(ink, true)]))
        .unwrap();
    let back = reopen(&mut doc, "raster-view");
    assert!(back.annotations(0).unwrap().is_empty());
    let bytes = back.bytes();
    assert!(!is_white(pixel(bytes.clone(), 150, 50)));
    assert!(!is_white(pixel(bytes, 50, 50)));
    // One undo step.
    doc.undo().unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 2);
}
