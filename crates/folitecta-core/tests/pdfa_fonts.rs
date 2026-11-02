//! Fonts that are not embedded become embedded subsets of their GNU FreeFont
//! substitutes when a document is converted to PDF/A: the same family and
//! style, the widths the text was laid out with, the encoding kept; Symbol
//! and ZapfDingbats in FreeSerif; a font with a character no substitute has
//! stays, and the copy is not declared conformant while it does.

use folitecta_core::Document;
use folitecta_core::testdocs::unembedded_fonts_doc;
use lopdf::{Dictionary, Object};
use skrifa::{FontRef, MetadataProvider, Tag};

fn font<'a>(doc: &'a Document, resource: &[u8]) -> &'a Dictionary {
    let inner = doc.inner();
    let page = inner.get_pages()[&1];
    let res = inner
        .get_dictionary(page)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap();
    let fonts = res.get(b"Font").unwrap().as_dict().unwrap();
    let id = fonts.get(resource).unwrap().as_reference().unwrap();
    inner.get_dictionary(id).unwrap()
}

fn program(doc: &Document, font: &Dictionary) -> Option<Vec<u8>> {
    let inner = doc.inner();
    let fd = inner
        .get_dictionary(font.get(b"FontDescriptor").ok()?.as_reference().ok()?)
        .ok()?;
    let file = inner
        .get_object(fd.get(b"FontFile2").ok()?.as_reference().ok()?)
        .ok()?;
    let s = file.as_stream().ok()?;
    Some(s.decompressed_content().unwrap_or_else(|_| s.content.clone()))
}

fn name(d: &Dictionary, key: &[u8]) -> String {
    d.get(key)
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
        .unwrap_or_default()
}

/// The advance of the glyph of `c` in the font program, in 1/1000 em.
fn advance(data: &[u8], c: char) -> f32 {
    let f = FontRef::new(data).unwrap();
    let gid = f.charmap().map(c).unwrap().to_u32() as usize;
    let head = f.table_data(Tag::new(b"head")).unwrap();
    let upem = u16::from_be_bytes([head.as_bytes()[18], head.as_bytes()[19]]) as f32;
    let hmtx = f.table_data(Tag::new(b"hmtx")).unwrap();
    let b = hmtx.as_bytes();
    u16::from_be_bytes([b[4 * gid], b[4 * gid + 1]]) as f32 * 1000.0 / upem
}

#[test]
fn simple_fonts_are_embedded_with_their_widths() {
    let original = Document::from_bytes(unembedded_fonts_doc(), None, None).unwrap();
    let before = original.preflight_pdfa();
    let fonts: Vec<_> = before.findings.iter().filter(|f| f.rule == "fonts").collect();
    assert_eq!(fonts.len(), 7, "{fonts:?}");
    assert_eq!(fonts.iter().filter(|f| f.fixable).count(), 7, "{fonts:?}");

    let (copy, outcome) = original.convert_to_pdfa().unwrap();
    let left: Vec<_> = outcome
        .after
        .findings
        .iter()
        .filter(|f| f.rule == "fonts")
        .collect();
    // The Chinese character has no glyph in any substitute: that font stays.
    assert_eq!(left.len(), 1, "{left:?}");
    assert!(left[0].detail.contains("Brush-Script"), "{left:?}");
    assert!(!outcome.claimed);
    // The same text; the dingbats, which nothing read before, are read now.
    assert_eq!(copy.page_text(0).replace(" ✁✔", ""), original.page_text(0));
    assert!(copy.page_text(0).contains("αβγ ✁✔"));

    // Helvetica: FreeSans, WinAnsi kept, its widths those of Helvetica.
    let h = font(&copy, b"H");
    assert_eq!(name(h, b"Subtype"), "TrueType");
    assert!(
        name(h, b"BaseFont").ends_with("+FreeSans"),
        "{}",
        name(h, b"BaseFont")
    );
    assert_eq!(name(h, b"Encoding"), "WinAnsiEncoding");
    let data = program(&copy, h).expect("embedded");
    assert!(
        (advance(&data, 'H') - 722.0).abs() < 1.0,
        "{}",
        advance(&data, 'H')
    );

    // Times-Bold: FreeSerif Bold, the apostrophe of StandardEncoding kept.
    let t = font(&copy, b"T");
    assert!(
        name(t, b"BaseFont").ends_with("+FreeSerifBold"),
        "{}",
        name(t, b"BaseFont")
    );
    let enc = t.get(b"Encoding").unwrap().as_dict().unwrap();
    let diff = enc.get(b"Differences").unwrap().as_array().unwrap();
    let at = diff
        .iter()
        .position(|o| *o == Object::Integer(39))
        .expect("code 39 listed");
    assert_eq!(diff[at + 1], Object::Name(b"quoteright".to_vec()));
    assert!(program(&copy, t).is_some());

    // Courier: FreeMono, every glyph 600 wide.
    let c = font(&copy, b"C");
    assert!(name(c, b"BaseFont").ends_with("+FreeMono"));
    let data = program(&copy, c).unwrap();
    assert!((advance(&data, 'W') - 600.0).abs() < 1.0);

    // Arial,Bold: FreeSans Bold, its letters as wide as the document said.
    let a = font(&copy, b"A");
    assert!(
        name(a, b"BaseFont").ends_with("+FreeSansBold"),
        "{}",
        name(a, b"BaseFont")
    );
    let data = program(&copy, a).unwrap();
    assert!(
        (advance(&data, 'I') - 700.0).abs() < 1.0,
        "{}",
        advance(&data, 'I')
    );
    let widths = a.get(b"Widths").unwrap().as_array().unwrap();
    let first = a.get(b"FirstChar").unwrap().as_i64().unwrap();
    let w_i = &widths[(b'I' as i64 - first) as usize];
    assert!(
        matches!(w_i, Object::Real(w) if (*w - 700.0).abs() < 0.01),
        "{w_i:?}"
    );

    // Symbol and ZapfDingbats: FreeSerif, symbolic, without an encoding,
    // their glyphs reached by code; the widths of their metrics.
    for (res, c, width) in [(b"S", '\u{F061}', 631.0), (b"Z", '\u{F021}', 974.0)] {
        let f = font(&copy, res);
        assert!(
            name(f, b"BaseFont").ends_with("+FreeSerif"),
            "{}",
            name(f, b"BaseFont")
        );
        assert!(!f.has(b"Encoding"));
        let data = program(&copy, f).unwrap();
        assert!((advance(&data, c) - width).abs() < 1.0, "{}", advance(&data, c));
    }
    assert_eq!(name(font(&copy, b"K"), b"Subtype"), "Type1");

    // Saved and opened again, nothing else is left but that font.
    let again = Document::from_bytes(copy.bytes().to_vec(), None, None).unwrap();
    let rules: Vec<_> = again.preflight_pdfa().findings.iter().map(|f| f.rule).collect();
    assert!(
        rules.iter().all(|r| *r == "fonts" || *r == "identification"),
        "{rules:?}"
    );
    // The original is not changed.
    assert_eq!(name(font(&original, b"H"), b"Subtype"), "Type1");
}

#[test]
fn text_stays_where_it_was() {
    let original = Document::from_bytes(unembedded_fonts_doc(), None, None).unwrap();
    let (copy, _) = original.convert_to_pdfa().unwrap();
    let a = original.text_layer(0).unwrap();
    let b = copy.text_layer(0).unwrap();
    // The dingbats, which nothing read before, are read now; the rest is the
    // same glyph for glyph.
    let dingbat = |t: &str| t == "✁" || t == "✔";
    assert_eq!(a.text, b.text.replace("✁✔", ""));
    let kept: Vec<_> = b.glyphs.iter().filter(|g| !dingbat(&g.text)).collect();
    let before: Vec<_> = a.glyphs.iter().filter(|g| !g.text.is_empty()).collect();
    assert_eq!(before.len(), kept.len());
    for (i, (x, y)) in before.iter().zip(kept.iter()).enumerate() {
        let near = x
            .quad
            .iter()
            .zip(y.quad.iter())
            .all(|(p, q)| (p - q).abs() < 0.05);
        assert!(near, "glyph {i} {:?}: {:?} became {:?}", x.text, x.quad, y.quad);
    }
}

#[test]
fn the_embedded_glyphs_are_drawn() {
    use folitecta_core::Renderer;
    let original = Document::from_bytes(unembedded_fonts_doc(), None, None).unwrap();
    let (copy, _) = original.convert_to_pdfa().unwrap();
    let ink = |bytes: Vec<u8>| {
        let px = Renderer::new(std::sync::Arc::new(bytes))
            .unwrap()
            .render_rgba(0, 2.0, Default::default())
            .unwrap();
        // The lines of Helvetica, Times and Courier, at the top of the page.
        let mut dark = 0usize;
        for y in 0..(px.height() as usize * 35 / 100) {
            for x in 0..px.width() as usize {
                if px.get_pixel(x as u32, y as u32)[0] < 128 {
                    dark += 1;
                }
            }
        }
        dark
    };
    let (a, b) = (ink(original.bytes().to_vec()), ink(copy.bytes().to_vec()));
    assert!(a > 500, "{a}");
    let ratio = b as f32 / a as f32;
    assert!((0.75..1.33).contains(&ratio), "ink {a} became {b}");
}

/// With `PDFA_DUMP` set to a folder, the converted documents are written there
/// for an independent validator.
#[test]
fn converted_documents_for_a_validator() {
    let Ok(dir) = std::env::var("PDFA_DUMP") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    use folitecta_core::TextLayout;
    let docs = [
        (
            "fonts-with-chinese",
            Document::from_bytes(unembedded_fonts_doc(), None, None).unwrap(),
        ),
        (
            "fonts-all-embeddable",
            Document::from_bytes(folitecta_core::testdocs::embeddable_fonts_doc(), None, None).unwrap(),
        ),
        (
            "plain-text",
            Document::from_text("Plain text in Helvetica, ‘quoted’ – €5.", TextLayout::default()).unwrap(),
        ),
        (
            "markdown",
            Document::from_markdown(
                "# Report\n\nSome *text* and `code`.\n\n- one\n- two\n",
                TextLayout::default(),
                "Report",
            )
            .unwrap(),
        ),
    ];
    let mut docs: Vec<(&str, Document)> = docs.into_iter().collect();
    use folitecta_core::testdocs as t;
    for (name, bytes) in [
        ("text", t::text_doc(2)),
        ("vector", t::vector_doc(1)),
        ("photo", t::photo_doc(1)),
        ("scan", t::scan_doc(1)),
        ("long-comments", t::long_doc(3)),
        ("embedded-fonts", t::font_doc(1)),
        ("form", t::form_doc(1, 6)),
        ("transparency", t::transparency_doc(1)),
        ("report", t::report_doc(2)),
        ("relations", t::relations_doc()),
        ("buttons", t::action_buttons_doc()),
        ("cropped", t::cropped_text_doc()),
    ] {
        docs.push((name, Document::from_bytes(bytes, None, None).unwrap()));
    }
    let mut noted = Document::from_text("A page with comments.", TextLayout::default()).unwrap();
    noted
        .add_note(0, 100.0, 700.0, "a note", "Reviewer", [1.0, 0.9, 0.2])
        .unwrap();
    docs.push(("noted", noted));
    for (name, doc) in docs {
        let Ok((copy, outcome)) = doc.convert_to_pdfa() else {
            println!("{name}: not converted");
            continue;
        };
        std::fs::write(dir.join(format!("{name}.pdf")), copy.bytes().as_slice()).unwrap();
        println!(
            "{name}: claimed {}, left {:?}",
            outcome.claimed,
            outcome.after.broken()
        );
    }
}

#[test]
fn text_in_the_initial_black_asks_for_an_output_intent() {
    // No colour is set before the text: it is painted in DeviceGray.
    let doc = Document::from_bytes(folitecta_core::testdocs::embeddable_fonts_doc(), None, None).unwrap();
    let before = doc.preflight_pdfa();
    assert!(
        before.findings.iter().any(|f| f.rule == "output-intent"),
        "{:?}",
        before.findings
    );
    let (copy, outcome) = doc.convert_to_pdfa().unwrap();
    assert!(outcome.claimed, "{:?}", outcome.after.findings);
    let root = copy.inner().trailer.get(b"Root").unwrap().as_reference().unwrap();
    assert!(copy.inner().get_dictionary(root).unwrap().has(b"OutputIntents"));
}

#[test]
fn fonts_written_inside_resources_are_found_and_embedded() {
    // The test documents write their standard fonts inside the resources.
    let doc = Document::from_bytes(folitecta_core::testdocs::text_doc(1), None, None).unwrap();
    let fonts: Vec<_> = doc
        .preflight_pdfa()
        .findings
        .into_iter()
        .filter(|f| f.rule == "fonts")
        .collect();
    assert!(!fonts.is_empty() && fonts.iter().all(|f| f.fixable), "{fonts:?}");
    let (copy, outcome) = doc.convert_to_pdfa().unwrap();
    assert!(outcome.claimed, "{:?}", outcome.after.findings);
    assert!(
        copy.used_fonts().iter().all(|f| f.embedded),
        "{:?}",
        copy.used_fonts()
    );
    assert_eq!(copy.page_text(0), doc.page_text(0));
}

#[test]
fn comments_and_buttons_get_what_pdfa_asks() {
    use folitecta_core::testdocs::{action_buttons_doc, long_doc};
    // Comments without appearances: drawn.
    let comments = Document::from_bytes(long_doc(2), None, None).unwrap();
    let before = comments.preflight_pdfa();
    assert!(
        before
            .findings
            .iter()
            .any(|f| f.rule == "annotation-appearance" && f.fixable),
        "{:?}",
        before.findings
    );
    let (_, outcome) = comments.convert_to_pdfa().unwrap();
    assert!(outcome.claimed, "{:?}", outcome.after.findings);

    // Push buttons: no action, their appearance a state.
    let buttons = Document::from_bytes(action_buttons_doc(), None, None).unwrap();
    let (copy, outcome) = buttons.convert_to_pdfa().unwrap();
    assert!(outcome.claimed, "{:?}", outcome.after.findings);
    let inner = copy.inner();
    for obj in inner.objects.values() {
        let Ok(d) = obj.as_dict() else { continue };
        if d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) != Some(b"Widget".as_slice()) {
            continue;
        }
        assert!(!d.has(b"A") && !d.has(b"AA"), "{d:?}");
        // Text fields may keep a single appearance; buttons have states.
        let mut field = d;
        let ft = loop {
            if let Ok(ft) = field.get(b"FT").and_then(|f| f.as_name()) {
                break Some(ft.to_vec());
            }
            match field.get(b"Parent").and_then(|p| p.as_reference()) {
                Ok(r) => field = inner.get_dictionary(r).unwrap(),
                Err(_) => break None,
            }
        };
        if ft.as_deref() != Some(b"Btn".as_slice()) {
            continue;
        }
        let ap = match d.get(b"AP").unwrap() {
            Object::Reference(r) => inner.get_dictionary(*r).unwrap(),
            Object::Dictionary(ap) => ap,
            other => panic!("{other:?}"),
        };
        let n = ap.get(b"N").unwrap();
        let n = match n {
            Object::Reference(r) => inner.get_object(*r).unwrap(),
            other => other,
        };
        assert!(
            n.as_dict().is_ok(),
            "the normal appearance is a dictionary of states: {n:?}"
        );
    }
}
