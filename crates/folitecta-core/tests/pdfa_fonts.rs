//! Fonts that are not embedded become embedded subsets of their GNU FreeFont
//! substitutes when a document is converted to PDF/A: the same family and
//! style, the widths the text was laid out with, the encoding kept; Symbol
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
    assert_eq!(fonts.len(), 5, "{fonts:?}");
    assert_eq!(fonts.iter().filter(|f| f.fixable).count(), 4, "{fonts:?}");

    let (copy, outcome) = original.convert_to_pdfa().unwrap();
    let left: Vec<_> = outcome
        .after
        .findings
        .iter()
        .filter(|f| f.rule == "fonts")
        .collect();
    assert_eq!(left.len(), 1, "{left:?}");
    assert!(left[0].detail.contains("Symbol"), "{left:?}");
    assert!(!outcome.claimed);
    assert_eq!(copy.page_text(0), original.page_text(0));

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

    // Symbol stays as it was.
    assert_eq!(name(font(&copy, b"S"), b"Subtype"), "Type1");

    // Saved and opened again, nothing else is left but Symbol.
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
    assert_eq!(a.text, b.text);
    assert_eq!(a.glyphs.len(), b.glyphs.len());
    for (i, (x, y)) in a.glyphs.iter().zip(b.glyphs.iter()).enumerate() {
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
