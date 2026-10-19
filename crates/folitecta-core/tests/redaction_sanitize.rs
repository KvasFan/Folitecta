//! Sanitizing with redaction, the clean-up every redaction does, and the
//! check of the bytes a save writes: every test reads the saved bytes again.

use std::collections::HashSet;

use folitecta_core::digsig::{DigitalId, IdentityInfo, KeyKind, SignOptions};
use folitecta_core::redaction::{
    CheckProblem, Overlay, RedactionLook, RedactionMethod, RedactionOptions, SanitizeOptions,
    check_redacted_bytes,
};
use folitecta_core::{BarcodeKind, Document, EncryptionProfile, SecurityPermissions, TextLayout};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

mod redaction_util;
use redaction_util::*;

fn sample() -> Document {
    Document::from_text(
        "Secret plan for Monday\nPublic notes stay here\u{c}Other page text",
        TextLayout::default(),
    )
    .unwrap()
}

fn mark_word(doc: &mut Document, page: usize, word: &str) -> [f32; 4] {
    let layer = doc.text_layer(page).unwrap();
    let r = pdf_rect(doc, page, word_box(&layer, word));
    doc.add_redaction(page, r, "Reviewer").unwrap();
    r
}

fn all_options() -> RedactionOptions {
    RedactionOptions::default()
}

fn no_sanitizing() -> RedactionOptions {
    RedactionOptions {
        method: RedactionMethod::Content,
        sanitize: SanitizeOptions::none(),
    }
}

/// Every key of every object of saved bytes.
fn keys(lo: &lopdf::Document) -> HashSet<Vec<u8>> {
    let mut out = HashSet::new();
    fn walk(o: &Object, out: &mut HashSet<Vec<u8>>) {
        match o {
            Object::Dictionary(d) => {
                for (k, v) in d.iter() {
                    out.insert(k.clone());
                    walk(v, out);
                }
            }
            Object::Stream(s) => walk(&Object::Dictionary(s.dict.clone()), out),
            Object::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    for o in lo.objects.values() {
        walk(o, &mut out);
    }
    out
}

/// Objects reachable from `start` without following `/Parent`, `/P`, `/Pg`.
fn reachable(lo: &lopdf::Document, start: ObjectId) -> HashSet<ObjectId> {
    let mut seen = HashSet::new();
    let mut todo = vec![start];
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Ok(o) = lo.get_object(id) else { continue };
        let mut refs = Vec::new();
        fn collect(o: &Object, refs: &mut Vec<ObjectId>) {
            match o {
                Object::Reference(r) => refs.push(*r),
                Object::Array(a) => a.iter().for_each(|x| collect(x, refs)),
                Object::Dictionary(d) => d
                    .iter()
                    .filter(|(k, _)| !matches!(k.as_slice(), b"Parent" | b"P" | b"Pg"))
                    .for_each(|(_, v)| collect(v, refs)),
                Object::Stream(s) => s
                    .dict
                    .iter()
                    .filter(|(k, _)| !matches!(k.as_slice(), b"Parent" | b"P" | b"Pg"))
                    .for_each(|(_, v)| collect(v, refs)),
                _ => {}
            }
        }
        collect(o, &mut refs);
        todo.extend(refs);
    }
    seen
}

/// An object holds `word` (strings, names or decoded stream data).
fn object_holds(o: &Object, word: &[u8]) -> bool {
    match o {
        Object::String(s, _) => holds(s, word),
        Object::Name(n) => holds(n, word),
        Object::Array(a) => a.iter().any(|x| object_holds(x, word)),
        Object::Dictionary(d) => d.iter().any(|(k, v)| holds(k, word) || object_holds(v, word)),
        Object::Stream(s) => {
            object_holds(&Object::Dictionary(s.dict.clone()), word)
                || holds(
                    &s.decompressed_content().unwrap_or_else(|_| s.content.clone()),
                    word,
                )
        }
        _ => false,
    }
}

fn check_now(
    doc: &mut Document,
    report: &folitecta_core::redaction::RedactionReport,
) -> folitecta_core::redaction::RedactionCheck {
    let prepared = doc.prepare_save().unwrap();
    check_redacted_bytes(&prepared.bytes, prepared.key.as_ref(), report).unwrap()
}

/// The full fixture: information, metadata, an attachment, a hidden layer,
/// bookmarks, a named destination with a link to it, a link quoting the
/// word, scripts, private data, thumbnails, a hidden comment, tags.
fn full_fixture() -> Document {
    let mut doc = sample();
    doc.set_info(
        "Secret report",
        "Jane Roe",
        "About the Secret plan",
        "secret, plan",
    )
    .unwrap();
    doc.set_xmp_packet("<?xpacket begin=\"\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>Secret report</dc:title></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>")
        .unwrap();
    doc.add_attachment(
        "notes.txt",
        b"the secret notes".to_vec(),
        "secret notes",
        Some("text/plain"),
    )
    .unwrap();
    let layer = Document::from_text("Layer secret words", TextLayout::default()).unwrap();
    let id = doc.import_as_layer(&layer, &[1], false, "Notes").unwrap();
    doc.set_layer_visible(id, false).unwrap();
    doc.add_bookmark("Secret plan", 0, &[]).unwrap();
    doc.add_bookmark("Other", 1, &[]).unwrap();
    doc.add_named_destination("Secret_plan", 0, None).unwrap();
    doc.add_named_link(1, [50.0, 50.0, 150.0, 70.0], "Secret_plan")
        .unwrap();
    doc.add_uri_link(1, [50.0, 100.0, 150.0, 120.0], "https://example.org/secret-plan")
        .unwrap();
    doc.set_document_scripts(&[("init".into(), "app.alert('x')".into())])
        .unwrap();
    doc.add_note(1, 300.0, 300.0, "hidden remark", "Reviewer", [1.0, 1.0, 0.0])
        .unwrap();
    // Private data, thumbnails, actions and tags written directly.
    let pages = doc.page_ids();
    let lo = doc.inner_mut();
    let thumb = lo.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
        vec![0],
    ));
    for pid in &pages {
        let d = lo.get_dictionary_mut(*pid).unwrap();
        d.set("Thumb", thumb);
        d.set("PieceInfo", dictionary! { "Editor" => dictionary! { "Private" => Object::string_literal("secret private data") } });
        d.set(
            "AA",
            dictionary! { "O" => dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("x()") } },
        );
    }
    // The note is hidden.
    let note = lo
        .objects
        .iter()
        .find(|(_, o)| {
            o.as_dict()
                .is_ok_and(|d| d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Text"))
        })
        .map(|(id, _)| *id)
        .unwrap();
    lo.get_dictionary_mut(note).unwrap().set("F", 2);
    let cat_id = lo.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let span = lo.add_object(dictionary! { "Type" => "StructElem", "S" => "Span", "Pg" => pages[0], "K" => 0, "ActualText" => lopdf::text_string("Secret plan") });
    let fig = lo.add_object(dictionary! { "Type" => "StructElem", "S" => "Figure", "Pg" => pages[1], "K" => 3, "Alt" => lopdf::text_string("Photo of the Secret plan") });
    let root =
        lo.add_object(dictionary! { "Type" => "StructTreeRoot", "K" => vec![span.into(), fig.into()] });
    for e in [span, fig] {
        lo.get_dictionary_mut(e).unwrap().set("P", root);
    }
    let cat = lo.get_dictionary_mut(cat_id).unwrap();
    cat.set("StructTreeRoot", root);
    cat.set(
        "OpenAction",
        dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("start()") },
    );
    cat.set("MarkInfo", dictionary! { "Marked" => true });
    doc.commit().unwrap();
    doc
}

#[test]
fn sanitizing_removes_metadata_attachments_hidden_layers_scripts_and_quotes() {
    let mut doc = full_fixture();
    let original_id = doc.inner().trailer.get(b"ID").ok().cloned();
    mark_word(&mut doc, 0, "Secret");
    let report = doc.apply_redactions_opts(None, &all_options()).unwrap();
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let s = &report.sanitized;
    assert!(s.metadata, "{s:?}");
    assert_eq!((s.attachments, s.hidden_layers, s.hidden), (1, 1, 1), "{s:?}");
    assert!(s.scripts >= 3, "{s:?}");
    assert!(s.quotes >= 4, "{s:?}");
    assert!(s.failed.is_empty(), "{s:?}");
    let check = check_now(&mut doc, &report);
    assert!(check.passed(), "{check:?}");
    let bytes = saved(&mut doc, "full");
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let info = lo
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|i| i.as_reference().ok())
        .and_then(|i| lo.get_dictionary(i).ok())
        .cloned()
        .unwrap_or_default();
    for k in [
        b"Title".as_slice(),
        b"Author",
        b"Subject",
        b"Keywords",
        b"Creator",
        b"CreationDate",
    ] {
        assert!(!info.has(k), "{}", String::from_utf8_lossy(k));
    }
    let k = keys(&lo);
    for key in [
        b"Metadata".as_slice(),
        b"PieceInfo",
        b"Thumb",
        b"AF",
        b"JS",
        b"OpenAction",
        b"AA",
        b"Collection",
        b"EmbeddedFiles",
        b"OCProperties",
    ] {
        assert!(!k.contains(key), "{}", String::from_utf8_lossy(key));
    }
    assert!(absent(&bytes, "secret notes"));
    assert!(absent(&bytes, "Layer secret"));
    assert!(absent(&bytes, "secret private"));
    let reopened = Document::from_bytes(bytes.clone(), None, None).unwrap();
    let titles: Vec<String> = reopened.outline().iter().map(|i| i.title.clone()).collect();
    assert_eq!(titles, vec!["[redacted] plan", "Other"]);
    assert_eq!(reopened.outline()[1].page, Some(1));
    let names: Vec<String> = reopened
        .named_destinations()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert!(!names.iter().any(|n| n.contains("Secret")), "{names:?}");
    let links = reopened.links(1).unwrap();
    assert_eq!(links.len(), 1, "the link quoting the word goes: {links:?}");
    // The link to the renamed destination still leads to the first page.
    match &links[0].target {
        folitecta_core::LinkTarget::Page(p) => assert_eq!(*p, 0),
        folitecta_core::LinkTarget::Named(name) => {
            assert!(!name.contains("Secret"));
            let dest = reopened
                .named_destinations()
                .into_iter()
                .find(|d| &d.name == name)
                .unwrap();
            assert_eq!(dest.page, Some(0));
        }
        other => panic!("{other:?}"),
    }
    let fig_alt = lo
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| d.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"Figure"))
        .and_then(|d| folitecta_core::util::dict_text(&lo, d, b"Alt"))
        .unwrap();
    assert_eq!(fig_alt, "Photo of the [redacted] plan");
    let span_text = lo
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| d.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"Span"))
        .and_then(|d| folitecta_core::util::dict_text(&lo, d, b"ActualText"));
    assert!(
        span_text.as_ref().is_none_or(|t| !t.contains("Secret")),
        "{span_text:?}"
    );
    assert_ne!(lo.trailer.get(b"ID").ok().cloned(), original_id);
    assert!(absent(&bytes, "Secret"));
}

#[test]
fn without_sanitizing_only_the_mandatory_parts_go() {
    let mut doc = full_fixture();
    // A comment over the mark with a reply, and a picture page 0 lists but never draws.
    doc.add_note(0, 60.0, 780.0, "about the word", "Reviewer", [1.0, 1.0, 0.0])
        .unwrap();
    let n = doc.annotations(0).unwrap().len() - 1;
    let r = doc.annotations(0).unwrap()[n].rect;
    doc.add_reply(0, n, "a reply", "Reviewer").unwrap();
    let pages = doc.page_ids();
    let unused = {
        let lo = doc.inner_mut();
        let im = lo.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
            b"UNUSEDPIX".to_vec(),
        ));
        let res = lo
            .get_dictionary(pages[0])
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .clone();
        match res {
            Object::Reference(rid) => {
                let d = lo.get_dictionary_mut(rid).unwrap();
                d.set("XObject", dictionary! { "Unused" => im });
            }
            Object::Dictionary(mut d) => {
                d.set("XObject", dictionary! { "Unused" => im });
                lo.get_dictionary_mut(pages[0]).unwrap().set("Resources", d);
            }
            _ => {}
        }
        im
    };
    doc.commit().unwrap();
    doc.add_redaction(0, [r[0] - 5.0, r[1] - 5.0, r[2] + 5.0, r[3] + 5.0], "Reviewer")
        .unwrap();
    mark_word(&mut doc, 0, "Secret");
    let report = doc.apply_redactions_opts(None, &no_sanitizing()).unwrap();
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert_eq!(report.sanitized, Default::default());
    // The comment and its reply (placed over it) go.
    assert_eq!(report.comments, 2, "{report:?}");
    let bytes = saved(&mut doc, "mandatory");
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let reopened = Document::from_bytes(bytes.clone(), None, None).unwrap();
    assert!(reopened.info().title.contains("Secret"));
    assert_eq!(reopened.attachments().len(), 1);
    assert!(!reopened.layers().is_empty());
    assert_eq!(reopened.outline()[0].title, "Secret plan");
    let p: Vec<ObjectId> = lo.get_pages().into_values().collect();
    let d0 = lo.get_dictionary(p[0]).unwrap();
    let d1 = lo.get_dictionary(p[1]).unwrap();
    assert!(!d0.has(b"Thumb") && !d0.has(b"PieceInfo"));
    assert!(d1.has(b"Thumb") && d1.has(b"PieceInfo"));
    // The reply went with its comment; nothing else on page 0 but the link-free rest.
    assert!(
        !reopened
            .annotations(0)
            .unwrap()
            .iter()
            .any(|a| a.contents == "a reply")
    );
    assert!(!lo.objects.contains_key(&unused) || !streams_of(&bytes).iter().any(|s| holds(s, b"UNUSEDPIX")));
}

#[test]
fn group_members_stay_when_their_head_goes() {
    let mut doc = sample();
    doc.add_note(0, 60.0, 780.0, "head", "Reviewer", [1.0, 1.0, 0.0])
        .unwrap();
    doc.add_note(0, 400.0, 400.0, "member", "Reviewer", [1.0, 1.0, 0.0])
        .unwrap();
    let annots = doc.annotations(0).unwrap();
    let head = annots.iter().find(|a| a.contents == "head").unwrap().id.unwrap();
    let member = annots
        .iter()
        .find(|a| a.contents == "member")
        .unwrap()
        .id
        .unwrap();
    let r = annots.iter().find(|a| a.contents == "head").unwrap().rect;
    {
        let d = doc.inner_mut().get_dictionary_mut(member).unwrap();
        d.set("IRT", head);
        d.set("RT", "Group");
    }
    doc.commit().unwrap();
    doc.add_redaction(0, [r[0] - 2.0, r[1] - 2.0, r[2] + 2.0, r[3] + 2.0], "Reviewer")
        .unwrap();
    doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    let bytes = saved(&mut doc, "group");
    let reopened = Document::from_bytes(bytes, None, None).unwrap();
    let annots = reopened.annotations(0).unwrap();
    assert!(annots.iter().all(|a| a.contents != "head"));
    let m = annots
        .iter()
        .find(|a| a.contents == "member")
        .expect("the member stays");
    let d = reopened.inner().get_dictionary(m.id.unwrap()).unwrap();
    assert!(!d.has(b"IRT") && !d.has(b"RT"));
}

fn signed() -> Vec<u8> {
    let id = DigitalId::create_self_signed(
        &IdentityInfo {
            name: "Reviewer".into(),
            ..IdentityInfo::default()
        },
        KeyKind::EcP256,
        1,
    )
    .unwrap();
    sample().sign(&id, &SignOptions::default()).unwrap()
}

#[test]
fn a_signed_document_loses_its_signatures_and_says_so() {
    let mut doc = Document::from_bytes(signed(), None, None).unwrap();
    assert_eq!(doc.redaction_summary(None).signatures, 1);
    mark_word(&mut doc, 0, "Secret");
    assert_eq!(doc.redaction_summary(None).signatures, 1);
    let report = doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    assert_eq!(report.signatures, 1);
    assert!(!doc.has_signatures());
    assert_eq!(doc.empty_signature_fields().len(), 1);
    let bytes = saved(&mut doc, "signed-cleared");
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let cat = lo.catalog().unwrap();
    assert!(!cat.has(b"Perms") && !cat.has(b"DSS"));
    let af = match cat.get(b"AcroForm").unwrap() {
        Object::Reference(r) => lo.get_dictionary(*r).unwrap().clone(),
        Object::Dictionary(d) => d.clone(),
        _ => panic!(),
    };
    // A signature field remains: only "signatures exist" is said.
    assert_eq!(af.get(b"SigFlags").unwrap().as_i64().unwrap(), 1);
    assert_eq!(bytes.windows(5).filter(|w| *w == b"%%EOF").count(), 1);
}

#[test]
fn undoing_a_redaction_of_a_signed_document_keeps_its_signatures_valid() {
    let mut doc = Document::from_bytes(signed(), None, None).unwrap();
    mark_word(&mut doc, 0, "Secret");
    doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    doc.undo().unwrap(); // the redaction
    doc.undo().unwrap(); // the mark
    let bytes = saved(&mut doc, "signed-undo");
    let reopened = Document::from_bytes(bytes, None, None).unwrap();
    let st = reopened.validate_signatures(&[]);
    assert_eq!(st.len(), 1);
    assert!(st[0].valid, "{:?}", st[0].problems);
}

#[test]
fn an_incremental_file_is_reported_as_keeping_an_earlier_version() {
    // A signed file changed and saved: an incremental update.
    let mut doc = Document::from_bytes(signed(), None, None).unwrap();
    doc.add_note(0, 300.0, 300.0, "later", "Reviewer", [1.0, 1.0, 0.0])
        .unwrap();
    let bytes = saved(&mut doc, "incremental");
    assert_eq!(bytes.windows(5).filter(|w| *w == b"%%EOF").count(), 2);
    let mut doc = Document::from_bytes(bytes.clone(), None, None).unwrap();
    mark_word(&mut doc, 0, "Secret");
    let report = doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    let check = check_redacted_bytes(&bytes, None, &report).unwrap();
    assert!(check.problems.contains(&CheckProblem::Revisions), "{check:?}");
}

fn protected(user: &str) -> (Document, Vec<u8>, std::path::PathBuf) {
    let path = temp(&format!("protected-{}", user.len()));
    sample()
        .save_encrypted(
            &path,
            user,
            "owner-pw",
            SecurityPermissions::default(),
            EncryptionProfile::Aes256,
        )
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let doc = if user.is_empty() {
        Document::open(&path).unwrap()
    } else {
        Document::open_with_password(&path, user).unwrap()
    };
    (doc, bytes, path)
}

#[test]
fn a_protected_document_keeps_its_identifier_and_is_checked_through_its_key() {
    for user in ["user-pw", ""] {
        let (mut doc, original, path) = protected(user);
        assert!(doc.is_protected());
        let id_before = doc.inner().trailer.get(b"ID").ok().cloned();
        mark_word(&mut doc, 0, "Secret");
        // Applied on a copy away from the document, then adopted.
        let mut work = doc.snapshot().open().unwrap();
        let report = work.apply_redactions_opts(None, &all_options()).unwrap();
        doc.adopt_state(work).unwrap();
        assert_eq!(doc.inner().trailer.get(b"ID").ok().cloned(), id_before);
        let prepared = doc.prepare_save().unwrap();
        assert!(holds(&prepared.bytes, b"/Encrypt"));
        let key = prepared.key.clone().expect("a protected document has a key");
        let check = check_redacted_bytes(&prepared.bytes, Some(&key), &report).unwrap();
        assert!(check.passed(), "{check:?}");
        // The original file, checked through the same key, still has the text.
        let check = check_redacted_bytes(&original, Some(&key), &report).unwrap();
        assert!(
            check.problems.contains(&CheckProblem::Text { page: 0 }),
            "{check:?}"
        );
        // Without the key the protected bytes cannot be checked.
        assert!(check_redacted_bytes(&prepared.bytes, None, &report).is_err());
        folitecta_core::document::write_pdf_file(&path, &prepared.bytes).unwrap();
        doc.finish_save(&path, &prepared);
        let reopened = if user.is_empty() {
            Document::open(&path).unwrap()
        } else {
            Document::open_with_password(&path, user).unwrap()
        };
        assert!(!reopened.page_text(0).contains("Secret"));
        assert!(reopened.page_text(0).contains("plan for Monday"));
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn the_check_finds_text_left_under_a_mark() {
    let mut doc = sample();
    let original = doc.bytes().to_vec();
    mark_word(&mut doc, 0, "Secret");
    let report = doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    let check = check_redacted_bytes(&original, None, &report).unwrap();
    assert!(
        check.problems.contains(&CheckProblem::Text { page: 0 }),
        "{check:?}"
    );
    assert!(check_now(&mut doc, &report).passed());
}

#[test]
fn blurred_marks_are_visual_only() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    let pic = image::RgbImage::from_fn(100, 100, |x, _| image::Rgb([(x * 2) as u8, 90, 200]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(pic)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    doc.add_image(0, png.into_inner(), Some([100.0, 100.0, 300.0, 300.0]))
        .unwrap();
    let look = RedactionLook {
        overlay: Overlay::Blur,
        ..RedactionLook::default()
    };
    doc.add_redaction_with(0, [120.0, 120.0, 200.0, 200.0], "Reviewer", &look)
        .unwrap();
    assert_eq!(doc.redaction_summary(None).blurred, 1);
    let report = doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    assert_eq!(report.blurred_areas, 1);
    let check = check_now(&mut doc, &report);
    assert_eq!(check.visual_only, 1);
    assert!(check.passed(), "{check:?}");
}

#[test]
fn quotes_elsewhere_are_reported_not_removed() {
    let mut doc =
        Document::from_text("Secret plan\u{c}The Secret stays here", TextLayout::default()).unwrap();
    mark_word(&mut doc, 0, "Secret");
    let report = doc.apply_redactions_opts(Some(&[0]), &all_options()).unwrap();
    let check = check_now(&mut doc, &report);
    assert!(check.passed(), "{check:?}");
    assert!(check.quoted_elsewhere >= 1, "{check:?}");
    assert!(doc.page_text(1).contains("Secret"));
}

#[test]
fn an_xfa_form_goes_with_any_redaction() {
    let mut doc = sample();
    {
        let lo = doc.inner_mut();
        let xfa = lo.add_object(Stream::new(
            dictionary! {},
            b"<template>Secret plan for Monday</template>".to_vec(),
        ));
        let af = lo.add_object(dictionary! { "Fields" => Vec::<Object>::new(), "XFA" => xfa });
        let cat = lo.trailer.get(b"Root").unwrap().as_reference().unwrap();
        lo.get_dictionary_mut(cat).unwrap().set("AcroForm", af);
    }
    doc.commit().unwrap();
    mark_word(&mut doc, 0, "Secret");
    let report = doc.apply_redactions_opts(None, &no_sanitizing()).unwrap();
    let check = check_now(&mut doc, &report);
    assert!(check.passed(), "{check:?}");
    let bytes = saved(&mut doc, "xfa");
    assert!(!keys(&lopdf::Document::load_mem(&bytes).unwrap()).contains(b"XFA".as_slice()));
    assert!(absent(&bytes, "Secret"));
}

#[test]
fn a_kept_attachment_holding_an_old_file_is_no_earlier_version() {
    let mut doc = sample();
    let pdf = Document::new_blank(1, 100.0, 100.0).unwrap().bytes().to_vec();
    assert!(holds(&pdf, b"startxref"));
    doc.add_attachment("old.pdf", pdf, "", Some("application/pdf"))
        .unwrap();
    mark_word(&mut doc, 0, "Secret");
    let report = doc.apply_redactions_opts(None, &no_sanitizing()).unwrap();
    let check = check_now(&mut doc, &report);
    assert!(!check.problems.contains(&CheckProblem::Revisions), "{check:?}");
    assert!(check.passed(), "{check:?}");
}

#[test]
fn a_barcode_under_a_mark_leaves_no_payload_anywhere() {
    let mut code = Document::from_text("Confidential code 4711 for Monday", TextLayout::default()).unwrap();
    code.add_barcode(
        0,
        [72.0, 140.0, 272.0, 200.0],
        BarcodeKind::Code128,
        "4711",
        false,
    )
    .unwrap();
    let mut doc = Document::from_text("Page one\u{c}Page two", TextLayout::default()).unwrap();
    doc.overlay_document(&code, &[0, 1], false).unwrap();
    mark_word(&mut doc, 0, "4711");
    // The barcode was placed in display space.
    let bars = pdf_rect(&doc, 0, [60.0, 130.0, 285.0, 210.0]);
    doc.add_redaction(0, bars, "Reviewer").unwrap();
    let report = doc.apply_redactions_opts(None, &all_options()).unwrap();
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert!(report.forms >= 2, "{report:?}");
    assert!(report.paths >= 1, "{report:?}");
    assert!(!doc.page_text(0).contains("4711"));
    assert!(doc.page_text(1).contains("4711"));
    let check = check_now(&mut doc, &report);
    assert!(check.passed(), "{check:?}");
    let bytes = saved(&mut doc, "barcode");
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let p: Vec<ObjectId> = lo.get_pages().into_values().collect();
    let mine = reachable(&lo, p[0]);
    let theirs = reachable(&lo, p[1]);
    for id in mine.difference(&theirs) {
        let o = lo.get_object(*id).unwrap();
        assert!(
            !object_holds(o, b"4711"),
            "object {id:?} of page 0 holds the code: {o:?}"
        );
        if let Ok(s) = o.as_stream() {
            assert!(!s.dict.has(b"FolitectaBarcode"));
        }
    }
}

/// Two pages sharing one resources object that lists a form with the word;
/// page 0 draws the form twice, both drawings under marks; page 1 draws none.
#[test]
fn an_original_replaced_on_every_drawing_leaves_the_file_even_through_shared_resources() {
    let mut lo = lopdf::Document::with_version("1.7");
    let f = helvetica(&mut lo);
    let fm = lo.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 200.into(), 50.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
        },
        b"BT /F1 12 Tf 10 20 Td (Classified) Tj ET".to_vec(),
    ));
    let res = lo.add_object(
        dictionary! { "XObject" => dictionary! { "Fm" => fm }, "Font" => dictionary! { "F1" => f } },
    );
    let pages_id = lo.new_object_id();
    let c0 = lo.add_object(Stream::new(
        dictionary! {},
        b"q 1 0 0 1 0 300 cm /Fm Do Q q 1 0 0 1 0 100 cm /Fm Do Q".to_vec(),
    ));
    let c1 = lo.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 50 50 Td (Plain page) Tj ET".to_vec(),
    ));
    let p0 = lo.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => c0 });
    let p1 = lo.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => c1 });
    // The resources and the media box are inherited from the page tree.
    lo.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![p0.into(), p1.into()], "Count" => 2,
            "Resources" => res, "MediaBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
        }),
    );
    let cat = lo.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    lo.trailer.set("Root", cat);
    let mut buf = Vec::new();
    lo.save_to(&mut buf).unwrap();
    let mut doc = Document::from_bytes(buf, None, None).unwrap();
    doc.add_redaction(0, [0.0, 300.0, 200.0, 350.0], "Reviewer")
        .unwrap();
    doc.add_redaction(0, [0.0, 100.0, 200.0, 150.0], "Reviewer")
        .unwrap();
    let report = doc.apply_redactions_opts(None, &no_sanitizing()).unwrap();
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let check = check_now(&mut doc, &report);
    assert!(check.passed(), "{check:?}");
    let bytes = saved(&mut doc, "shared-res");
    assert!(absent(&bytes, "Classified"), "the original form is gone");
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    assert!(!lo.objects.contains_key(&fm));
    assert!(
        Document::from_bytes(bytes, None, None)
            .unwrap()
            .page_text(1)
            .contains("Plain page")
    );
}

#[test]
fn a_page_inheriting_its_resources_loses_its_picture_and_form_data() {
    let mut lo = lopdf::Document::with_version("1.7");
    let f = helvetica(&mut lo);
    let fm = lo.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
        },
        b"BT /F1 12 Tf 50 300 Td (Hidden formula) Tj ET".to_vec(),
    ));
    let im = lo.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 4, "Height" => 4, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
        vec![200u8; 16],
    ));
    let res = lo.add_object(dictionary! { "XObject" => dictionary! { "Fm" => fm, "Im" => im } });
    let pages_id = lo.new_object_id();
    let c0 = lo.add_object(Stream::new(
        dictionary! {},
        b"/Fm Do q 100 0 0 100 50 150 cm /Im Do Q".to_vec(),
    ));
    let p0 = lo.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => c0 });
    lo.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![p0.into()], "Count" => 1,
            "Resources" => res, "MediaBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
        }),
    );
    let cat = lo.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    lo.trailer.set("Root", cat);
    let mut buf = Vec::new();
    lo.save_to(&mut buf).unwrap();
    let mut doc = Document::from_bytes(buf, None, None).unwrap();
    doc.add_redaction(0, [40.0, 140.0, 200.0, 330.0], "Reviewer")
        .unwrap();
    let report = doc.apply_redactions_opts(None, &no_sanitizing()).unwrap();
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert!(check_now(&mut doc, &report).passed());
    let bytes = saved(&mut doc, "inherited");
    assert!(absent(&bytes, "Hidden formula"));
    // No picture keeps its original pixels under the mark.
    let doc2 = Document::from_bytes(bytes, None, None).unwrap();
    let px = doc2.image_pixels(0, 0).unwrap();
    assert!(
        px.pixels().all(|p| p[0] == 0),
        "covered everywhere under the mark"
    );
}

#[test]
fn structure_references_follow_the_copy_of_a_form() {
    let mut lo = lopdf::Document::with_version("1.7");
    let f = helvetica(&mut lo);
    let fm = lo.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } }, "StructParents" => 0,
        },
        b"/Span <</MCID 0>> BDC BT /F1 12 Tf 50 300 Td (Secret entry) Tj ET EMC".to_vec(),
    ));
    let pages_id = lo.new_object_id();
    let mut pids = Vec::new();
    for _ in 0..2 {
        let c = lo.add_object(Stream::new(dictionary! {}, b"/Fm Do".to_vec()));
        pids.push(lo.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => c,
            "MediaBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
            "Resources" => dictionary! { "XObject" => dictionary! { "Fm" => fm } },
        }));
    }
    let root = lo.new_object_id();
    let mut elems = Vec::new();
    for (i, p) in pids.iter().enumerate() {
        elems.push(lo.add_object(dictionary! {
            "Type" => "StructElem", "S" => "P", "P" => root, "Pg" => *p,
            "ActualText" => lopdf::text_string(&format!("Secret entry {i}")),
            "K" => dictionary! { "Type" => "MCR", "Pg" => *p, "Stm" => fm, "MCID" => 0 },
        }));
    }
    lo.objects.insert(
        root,
        Object::Dictionary(dictionary! { "Type" => "StructTreeRoot", "K" => elems.iter().map(|e| Object::Reference(*e)).collect::<Vec<_>>() }),
    );
    lo.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => pids.iter().map(|p| Object::Reference(*p)).collect::<Vec<_>>(), "Count" => 2 }),
    );
    let cat = lo.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id, "StructTreeRoot" => root, "MarkInfo" => dictionary! { "Marked" => true } });
    lo.trailer.set("Root", cat);
    let mut buf = Vec::new();
    lo.save_to(&mut buf).unwrap();
    let mut doc = Document::from_bytes(buf, None, None).unwrap();
    mark_word(&mut doc, 0, "Secret");
    let report = doc.apply_redactions_opts(None, &no_sanitizing()).unwrap();
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert_eq!(report.forms, 1);
    let bytes = saved(&mut doc, "struct");
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let e0 = lo.get_dictionary(elems[0]).unwrap();
    let mcr = e0.get(b"K").unwrap().as_dict().unwrap();
    let stm = mcr.get(b"Stm").unwrap().as_reference().unwrap();
    assert_ne!(stm, fm, "page 0's reference points to its copy");
    let res0 = lo
        .get_dictionary(pids[0])
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap()
        .clone();
    let drawn: Vec<ObjectId> = res0
        .get(b"XObject")
        .unwrap()
        .as_dict()
        .unwrap()
        .iter()
        .map(|(_, v)| v.as_reference().unwrap())
        .collect();
    assert!(drawn.contains(&stm));
    // The element of the removed content lost its replacement text; page 1's kept it.
    assert!(!e0.has(b"ActualText"));
    assert!(lo.get_dictionary(elems[1]).unwrap().has(b"ActualText"));
    // Nothing only page 0 reaches holds the word.
    let mine = reachable(&lo, pids[0]);
    let theirs = reachable(&lo, pids[1]);
    for id in mine.difference(&theirs) {
        assert!(!object_holds(lo.get_object(*id).unwrap(), b"Secret"), "{id:?}");
    }
}

#[test]
fn applying_the_current_page_leaves_the_other_marks_pending() {
    let mut doc = sample();
    mark_word(&mut doc, 0, "Secret");
    mark_word(&mut doc, 1, "Other");
    let report = doc.apply_redactions_opts(Some(&[0]), &all_options()).unwrap();
    assert_eq!(report.areas, 1);
    assert_eq!(doc.pending_redactions().len(), 1);
    assert!(doc.page_text(1).contains("Other"));
}

#[test]
fn terms_are_the_words_under_the_marks() {
    let mut doc = sample();
    mark_word(&mut doc, 0, "Secret plan");
    let terms: Vec<String> = doc
        .redaction_terms(None, "[r]")
        .iter()
        .map(|t| t.shown().to_string())
        .collect();
    assert_eq!(terms, vec!["Secret plan", "Secret", "plan"]);
    let (_, preview) = doc.redaction_preview(None, &SanitizeOptions::default());
    assert_eq!(preview.attachments, 0);
    let _ = Dictionary::new();
}

#[test]
fn pages_drawn_as_pictures_pass_the_check_and_their_originals_go() {
    let mut doc = sample();
    let code = RedactionLook {
        text: "CODE-9".into(),
        ..RedactionLook::default()
    };
    let layer = doc.text_layer(0).unwrap();
    let r = pdf_rect(&doc, 0, word_box(&layer, "Secret plan"));
    doc.add_redaction_with(0, r, "Reviewer", &code).unwrap();
    let opts = RedactionOptions {
        method: RedactionMethod::Picture,
        sanitize: SanitizeOptions::none(),
    };
    let report = doc.apply_redactions_opts(None, &opts).unwrap();
    assert_eq!(report.pages_as_pictures, vec![0]);
    let check = check_now(&mut doc, &report);
    assert!(check.passed(), "{check:?}");
    let bytes = saved(&mut doc, "pictures");
    assert!(absent(&bytes, "Secret plan"));
    let reopened = Document::from_bytes(bytes, None, None).unwrap();
    assert!(reopened.page_text(0).contains("CODE-9"));
    assert!(reopened.page_text(1).contains("Other page text"));
}

/// 410 pages sharing one resources object; a 10 000-operation logo on every
/// page; page 0 also draws a picture and a form with a word under the mark.
fn big_shared() -> (Vec<u8>, ObjectId, ObjectId) {
    let mut d = lopdf::Document::with_version("1.7");
    let f = helvetica(&mut d);
    let mut px = vec![90u8; 32 * 8];
    for i in 0..20 {
        px[i * 12..i * 12 + 11].copy_from_slice(b"BIGSECRETPX");
    }
    let im = d.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 32, "Height" => 8, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
        px,
    ));
    let fx = d.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
        },
        b"BT /F1 12 Tf 100 600 Td (BIGSECRETFORM) Tj ET".to_vec(),
    ));
    let mut logo = String::new();
    for i in 0..2500 {
        logo.push_str(&format!(
            "{} 760 m {} 770 l {} 760 l h f\n",
            20 + i % 50,
            21 + i % 50,
            22 + i % 50
        ));
    }
    let lg = d.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 750.into(), 600.into(), 800.into()], "Resources" => dictionary! {} },
        logo.into_bytes(),
    ));
    let shared = d.add_object(dictionary! { "Font" => dictionary! { "F1" => f }, "XObject" => dictionary! { "Im" => im, "Fx" => fx, "Logo" => lg } });
    let pages_id = d.new_object_id();
    let mut kids = Vec::new();
    for i in 0..410 {
        let content = if i == 0 {
            // The form twice under the mark: each drawing gets a copy.
            "/Logo Do q 64 0 0 16 100 300 cm /Im Do Q /Fx Do q 1 0 0 1 0 -100 cm /Fx Do Q".to_string()
        } else {
            "/Logo Do BT /F1 12 Tf 100 100 Td (ordinary page) Tj ET".to_string()
        };
        let c = d.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        kids.push(Object::Reference(d.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => c, "Resources" => shared,
            "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
        })));
    }
    d.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => 410 }),
    );
    let cat = d.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    d.trailer.set("Root", cat);
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    (out, fx, shared)
}

#[test]
fn a_large_document_sharing_its_resources_keeps_no_original_and_the_check_says_when_one_is_back() {
    let (bytes, fx, shared) = big_shared();
    let original = lopdf::Document::load_mem(&bytes).unwrap();
    let mut doc = Document::from_bytes(bytes, None, None).unwrap();
    doc.add_redaction(0, [90.0, 290.0, 300.0, 620.0], "Reviewer")
        .unwrap();
    let report = doc.apply_redactions_opts(None, &all_options()).unwrap();
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let check = check_now(&mut doc, &report);
    assert!(check.passed(), "{check:?}");
    let saved_bytes = saved(&mut doc, "big");
    assert!(absent(&saved_bytes, "BIGSECRETPX") && absent(&saved_bytes, "BIGSECRETFORM"));
    // The original form put back into the shared resources: the check fails.
    let mut lo = lopdf::Document::load_mem(&saved_bytes).unwrap();
    lo.objects.insert(fx, original.get_object(fx).unwrap().clone());
    if let Ok(Object::Dictionary(x)) = lo.get_dictionary_mut(shared).unwrap().get_mut(b"XObject") {
        x.set("Back", fx);
    }
    let mut back = Vec::new();
    lo.save_to(&mut back).unwrap();
    let check = check_redacted_bytes(&back, None, &report).unwrap();
    assert!(
        check.problems.contains(&CheckProblem::Residual { page: Some(0) }),
        "{check:?}"
    );
}

/// A page whose picture carries metadata quoting the word, named by the
/// document's open action and a bookmark.
/// A page whose picture and page metadata quote the word under the mark,
/// named by a bookmark and the opening action when `named`.
fn targeted_page(named: bool) -> Document {
    let mut d = lopdf::Document::with_version("1.7");
    let f = helvetica(&mut d);
    let imd = d.add_object(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        b"<x:xmpmeta>photo of RESIDUALWORD</x:xmpmeta>".to_vec(),
    ));
    let img = d.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 4, "Height" => 4, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8, "Metadata" => imd },
        vec![200u8; 16],
    ));
    let pmd = d.add_object(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        b"<x:xmpmeta>page about RESIDUALWORD</x:xmpmeta>".to_vec(),
    ));
    let pages_id = d.new_object_id();
    let c = d.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 100 700 Td (RESIDUALWORD) Tj ET q 32 0 0 32 300 690 cm /Im Do Q".to_vec(),
    ));
    let p0 = d.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => c, "Metadata" => pmd,
        "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => f }, "XObject" => dictionary! { "Im" => img } },
    });
    d.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![p0.into()], "Count" => 1 }),
    );
    let mut cat = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    if named {
        let outlines = d.new_object_id();
        let item = d.add_object(dictionary! { "Title" => lopdf::text_string("Start"), "Parent" => outlines, "Dest" => vec![p0.into(), Object::Name(b"Fit".to_vec())] });
        d.objects.insert(
            outlines,
            Object::Dictionary(
                dictionary! { "Type" => "Outlines", "First" => item, "Last" => item, "Count" => 1 },
            ),
        );
        cat.set("Outlines", outlines);
        cat.set("OpenAction", vec![p0.into(), Object::Name(b"Fit".to_vec())]);
    }
    let cat = d.add_object(cat);
    d.trailer.set("Root", cat);
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    Document::from_bytes(out, None, None).unwrap()
}

/// The redacted page passes the check and loses the metadata quoting the
/// word; the original file is caught: the word stays in what only the page
/// uses, and the text layer under the mark does not count as shown.
fn page_checked_and_its_metadata_gone(named: bool) {
    let mut doc = targeted_page(named);
    let original = doc.bytes().to_vec();
    doc.add_redaction(0, [95.0, 690.0, 335.0, 725.0], "Reviewer")
        .unwrap();
    let quoting_only = RedactionOptions {
        method: RedactionMethod::Content,
        sanitize: SanitizeOptions {
            quoting: true,
            ..SanitizeOptions::none()
        },
    };
    let report = doc.apply_redactions_opts(None, &quoting_only).unwrap();
    let check = check_now(&mut doc, &report);
    assert!(check.passed(), "{check:?}");
    let bytes = saved(
        &mut doc,
        if named {
            "targeted-named"
        } else {
            "targeted-unnamed"
        },
    );
    // The metadata of the covered picture and of the page went (metadata
    // sanitizing off), with the word they quoted.
    assert!(absent(&bytes, "RESIDUALWORD"));
    // The original file: what only the page uses still quotes the word, and
    // the destinations naming the page do not hide it from the check.
    let check = check_redacted_bytes(&original, None, &report).unwrap();
    assert!(
        check.problems.contains(&CheckProblem::Residual { page: Some(0) }),
        "{check:?}"
    );
}

#[test]
fn a_page_named_by_destinations_is_still_checked_and_its_metadata_goes() {
    page_checked_and_its_metadata_gone(true);
}

#[test]
fn a_page_no_destination_names_is_checked_without_the_text_under_its_marks() {
    page_checked_and_its_metadata_gone(false);
}

#[test]
fn undrawn_resources_a_page_inherits_go_with_both_methods() {
    for method in [RedactionMethod::Content, RedactionMethod::Picture] {
        let mut d = lopdf::Document::with_version("1.7");
        let f = helvetica(&mut d);
        let form = |d: &mut lopdf::Document, text: &str| {
            d.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
                    "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
                },
                format!("BT /F1 12 Tf 100 600 Td ({text}) Tj ET").into_bytes(),
            ))
        };
        let fx = form(&mut d, "PICSECRETFORM");
        let fu = form(&mut d, "PICSECRETUNDRAWN");
        let pages_id = d.new_object_id();
        let c = d.add_object(Stream::new(dictionary! {}, b"/Fx Do".to_vec()));
        let p0 = d.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => c, "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()] });
        d.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![p0.into()], "Count" => 1,
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => f }, "XObject" => dictionary! { "Fx" => fx, "Fu" => fu } },
            }),
        );
        let cat = d.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        d.trailer.set("Root", cat);
        let mut out = Vec::new();
        d.save_to(&mut out).unwrap();
        let mut doc = Document::from_bytes(out, None, None).unwrap();
        doc.add_redaction(0, [50.0, 580.0, 560.0, 630.0], "Reviewer")
            .unwrap();
        let report = doc
            .apply_redactions_opts(
                None,
                &RedactionOptions {
                    method,
                    sanitize: SanitizeOptions::none(),
                },
            )
            .unwrap();
        assert!(check_now(&mut doc, &report).passed(), "{method:?}");
        let bytes = saved(&mut doc, "inherited-undrawn");
        assert!(
            absent(&bytes, "PICSECRETFORM") && absent(&bytes, "PICSECRETUNDRAWN"),
            "{method:?}"
        );
    }
}

#[test]
fn words_left_quoted_on_purpose_stay_known_to_the_check() {
    let mut doc = sample();
    doc.add_bookmark("Secret plan", 0, &[]).unwrap();
    mark_word(&mut doc, 0, "Secret");
    let opts = RedactionOptions {
        method: RedactionMethod::Content,
        sanitize: SanitizeOptions {
            quoting: true,
            skip_terms: vec!["secret".into()],
            ..SanitizeOptions::none()
        },
    };
    let report = doc.apply_redactions_opts(None, &opts).unwrap();
    assert!(
        report.terms().contains(&"Secret".to_string()),
        "{:?}",
        report.terms()
    );
    assert_eq!(doc.outline()[0].title, "Secret plan");
    // The bookmark left on purpose is reported, not a failure.
    let check = check_now(&mut doc, &report);
    assert!(check.passed(), "{check:?}");
    assert!(check.quoted_elsewhere >= 1, "{check:?}");
}

#[test]
fn a_protected_signed_document_is_written_whole_and_encrypted_without_its_signatures() {
    let path = temp("protected-signed");
    sample()
        .save_encrypted(
            &path,
            "user-pw",
            "owner-pw",
            SecurityPermissions::default(),
            EncryptionProfile::Aes256,
        )
        .unwrap();
    let opened = Document::open_with_password(&path, "user-pw").unwrap();
    let id = DigitalId::create_self_signed(
        &IdentityInfo {
            name: "Reviewer".into(),
            ..IdentityInfo::default()
        },
        KeyKind::EcP256,
        1,
    )
    .unwrap();
    let mut doc = opened
        .sign_to_file(&id, &SignOptions::default(), None)
        .unwrap()
        .into_document(None)
        .unwrap();
    assert!(doc.is_protected() && doc.has_signatures());
    mark_word(&mut doc, 0, "Secret");
    let mut work = doc.snapshot().open().unwrap();
    let report = work.apply_redactions_opts(None, &all_options()).unwrap();
    doc.adopt_state(work).unwrap();
    assert_eq!(report.signatures, 1);
    assert!(!doc.has_signatures());
    let prepared = doc.prepare_save().unwrap();
    // A whole file (the signed base holds what was removed), encrypted.
    assert!(holds(&prepared.bytes, b"/Encrypt"));
    assert_eq!(prepared.bytes.windows(5).filter(|w| *w == b"%%EOF").count(), 1);
    let check = check_redacted_bytes(&prepared.bytes, prepared.key.as_ref(), &report).unwrap();
    assert!(check.passed(), "{check:?}");
    folitecta_core::document::write_pdf_file(&path, &prepared.bytes).unwrap();
    doc.finish_save(&path, &prepared);
    assert!(matches!(
        Document::open(&path),
        Err(folitecta_core::Error::PasswordRequired)
    ));
    for pw in ["user-pw", "owner-pw"] {
        let back = Document::open_with_password(&path, pw).unwrap();
        assert!(back.is_protected());
        assert!(!back.has_signatures());
        assert!(!back.page_text(0).contains("Secret"));
        assert!(back.page_text(0).contains("plan for Monday"));
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_aes128_file_opened_with_a_non_ascii_owner_password_is_redacted_and_checked() {
    let path = temp("aes128-owner");
    let owner = "pässwörd€";
    sample()
        .save_encrypted(
            &path,
            "user-pw",
            owner,
            SecurityPermissions::default(),
            EncryptionProfile::Aes128,
        )
        .unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut doc = Document::open_with_password(&path, owner).unwrap();
    assert!(doc.is_protected());
    mark_word(&mut doc, 0, "Secret");
    let report = doc.apply_redactions_opts(None, &all_options()).unwrap();
    let prepared = doc.prepare_save().unwrap();
    let key = prepared.key.clone().unwrap();
    let check = check_redacted_bytes(&prepared.bytes, Some(&key), &report).unwrap();
    assert!(check.passed(), "{check:?}");
    // The original file, read through the same key, still has the text.
    let check = check_redacted_bytes(&original, Some(&key), &report).unwrap();
    assert!(
        check.problems.contains(&CheckProblem::Text { page: 0 }),
        "{check:?}"
    );
    folitecta_core::document::write_pdf_file(&path, &prepared.bytes).unwrap();
    for pw in ["user-pw", owner] {
        let back = Document::open_with_password(&path, pw).unwrap();
        assert!(back.is_protected());
        assert!(!back.page_text(0).contains("Secret"), "{pw}");
        assert!(back.page_text(0).contains("plan for Monday"));
    }
    let _ = std::fs::remove_file(&path);
}
