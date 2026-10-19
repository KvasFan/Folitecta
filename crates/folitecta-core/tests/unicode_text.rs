//! Generated text beyond WinAnsi: text boxes, typewriter text and callouts
//! drawn with an embedded subset of FreeSans, saved, reopened, edited,
//! exchanged, protected, signed and redacted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use folitecta_core::digsig::{DigitalId, IdentityInfo, KeyKind, SignOptions};
use folitecta_core::redaction::RedactionMethod;
use folitecta_core::spelling::{Misspelling, SpellPlace};
use folitecta_core::{
    AnnotStyle, CommentFormat, Document, Error, FlattenScope, GlyphProblem, MeasureKind, MeasureScale,
    TextLayout,
};
use lopdf::{Dictionary, Object};
use skrifa::MetadataProvider;
use skrifa::outline::{DrawSettings, pen::SvgPen};
use skrifa::prelude::{LocationRef, Size};

const MIXED: &str = "Città è perché — Łódź, Ελληνικά, Кириллица";
const BOX: [f32; 4] = [50.0, 600.0, 545.0, 640.0];
const BLACK: [f32; 3] = [0.0, 0.0, 0.0];

fn blank() -> Document {
    Document::new_blank(1, 595.0, 842.0).unwrap()
}

/// A file in a folder of its own, removed with the folder when the test
/// ends, passed or failed (tests run side by side: no folder is shared).
struct Temp(PathBuf);

impl Drop for Temp {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

impl std::ops::Deref for Temp {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Temp {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

fn temp(name: &str) -> Temp {
    let dir = std::env::temp_dir().join(format!("folitecta-unicode-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    Temp(dir.join(format!("{name}.pdf")))
}

#[test]
fn the_files_of_these_tests_are_removed_when_they_end() {
    let path = {
        let t = temp("cleanup");
        std::fs::write(&*t, b"%PDF-").unwrap();
        t.0.clone()
    };
    assert!(!path.exists() && !path.parent().unwrap().exists());
}

fn add_box(doc: &mut Document, text: &str) -> folitecta_core::Result<lopdf::ObjectId> {
    doc.add_free_text(0, BOX, text, "Reviewer", 12.0, BLACK, true)
}

fn get<'a>(doc: &'a lopdf::Document, obj: &'a Object) -> &'a Object {
    match obj {
        Object::Reference(id) => doc.get_object(*id).unwrap(),
        other => other,
    }
}

fn dict<'a>(doc: &'a lopdf::Document, obj: &'a Object) -> &'a Dictionary {
    match get(doc, obj) {
        Object::Dictionary(d) => d,
        Object::Stream(s) => &s.dict,
        other => panic!("not a dictionary: {other:?}"),
    }
}

fn stream_data(doc: &lopdf::Document, obj: &Object) -> Vec<u8> {
    let s = get(doc, obj).as_stream().unwrap();
    s.decompressed_content().unwrap_or_else(|_| s.content.clone())
}

/// The normal appearance stream of an annotation object.
fn appearance(doc: &lopdf::Document, annot: lopdf::ObjectId) -> &lopdf::Stream {
    let a = doc.get_dictionary(annot).unwrap();
    let ap = dict(doc, a.get(b"AP").unwrap());
    get(doc, ap.get(b"N").unwrap()).as_stream().unwrap()
}

/// The Type 0 font `FoliUni` of an appearance, with its object id.
fn unicode_font(doc: &lopdf::Document, annot: lopdf::ObjectId) -> (lopdf::ObjectId, &Dictionary) {
    let ap = appearance(doc, annot);
    let res = dict(doc, ap.dict.get(b"Resources").unwrap());
    let fonts = dict(doc, res.get(b"Font").unwrap());
    let font = fonts.get(b"FoliUni").unwrap();
    (font.as_reference().unwrap(), dict(doc, font))
}

fn name(d: &Dictionary, key: &[u8]) -> String {
    String::from_utf8_lossy(d.get(key).unwrap().as_name().unwrap()).to_string()
}

/// The code → text pairs of a ToUnicode CMap.
fn to_unicode(data: &[u8]) -> HashMap<u16, String> {
    let text = String::from_utf8_lossy(data);
    let mut out = HashMap::new();
    let mut inside = false;
    for line in text.lines() {
        if line.ends_with("beginbfchar") {
            inside = true;
        } else if line == "endbfchar" {
            inside = false;
        } else if inside {
            let parts: Vec<&str> = line.split(['<', '>', ' ']).filter(|s| !s.is_empty()).collect();
            let code = u16::from_str_radix(parts[0], 16).unwrap();
            let units: Vec<u16> = parts[1]
                .as_bytes()
                .chunks(4)
                .map(|c| u16::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
                .collect();
            out.insert(code, String::from_utf16(&units).unwrap());
        }
    }
    out
}

/// The hexadecimal string operands of the `Tj` operators of a content stream.
fn shown_codes(content: &[u8]) -> Vec<Vec<u16>> {
    let text = String::from_utf8_lossy(content);
    let mut out = Vec::new();
    let mut rest = text.as_ref();
    while let Some(at) = rest.find("> Tj") {
        let start = rest[..at].rfind('<').unwrap();
        let hex = &rest[start + 1..at];
        out.push(
            hex.as_bytes()
                .chunks(4)
                .map(|c| u16::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
                .collect(),
        );
        rest = &rest[at + 4..];
    }
    out
}

fn count_objects(doc: &lopdf::Document, test: impl Fn(&Object) -> bool) -> usize {
    doc.objects.values().filter(|o| test(o)).count()
}

fn has_font_file(o: &Object) -> bool {
    match o {
        Object::Dictionary(d) => d.has(b"FontFile2"),
        _ => false,
    }
}

fn is_type0(o: &Object) -> bool {
    matches!(o, Object::Dictionary(d) if d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Type0".as_slice()))
}

fn is_unicode_box(doc: &Document, page: usize, index: usize) -> bool {
    let fonts = doc.annotation_fonts(page, index).unwrap();
    fonts.len() == 1 && fonts[0].subtype == "Type0" && fonts[0].embedded && fonts[0].to_unicode
}

fn is_helvetica(doc: &Document, page: usize, index: usize) -> bool {
    let fonts = doc.annotation_fonts(page, index).unwrap();
    fonts.len() == 1 && fonts[0].base_font == "Helvetica" && !fonts[0].embedded
}

fn reopen(path: &Path) -> Document {
    Document::open(path).unwrap()
}

/// What a refused operation must leave unchanged.
struct Unchanged {
    revision: u64,
    bytes: Arc<Vec<u8>>,
    undo: bool,
    redo: bool,
    comments: usize,
}

impl Unchanged {
    fn of(doc: &Document) -> Self {
        Unchanged {
            revision: doc.revision(),
            bytes: doc.bytes(),
            undo: doc.can_undo(),
            redo: doc.can_redo(),
            comments: doc.comments().len(),
        }
    }

    fn check(&self, doc: &Document) {
        assert_eq!(doc.revision(), self.revision);
        assert_eq!(*doc.bytes(), *self.bytes);
        assert_eq!(doc.can_undo(), self.undo);
        assert_eq!(doc.can_redo(), self.redo, "the redo history is kept");
        assert_eq!(doc.comments().len(), self.comments);
    }
}

/// A document with a note made and undone, so it has something to redo.
fn with_redo() -> Document {
    let mut doc = blank();
    doc.add_note(0, 20.0, 820.0, "note", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    doc.undo().unwrap();
    assert!(doc.can_redo());
    doc
}

#[test]
fn unicode_text_box_embeds_a_font_subset_that_survives_saving() {
    let mut doc = blank();
    add_box(&mut doc, MIXED).unwrap();
    let path = temp("subset");
    doc.save_as(&path).unwrap();
    let doc = reopen(&path);
    let a = &doc.annotations(0).unwrap()[0];
    assert_eq!(a.contents, MIXED);
    let fonts = doc.annotation_fonts(0, 0).unwrap();
    assert_eq!(fonts.len(), 1);
    let f = &fonts[0];
    assert_eq!((f.resource.as_str(), f.subtype.as_str()), ("FoliUni", "Type0"));
    assert!(f.embedded && f.to_unicode);
    let (tag, rest) = f.base_font.split_once('+').unwrap();
    assert!(
        tag.len() == 6 && tag.chars().all(|c| c.is_ascii_uppercase()),
        "{tag}"
    );
    assert_eq!(rest, "FreeSans");

    // The objects, read with the PDF library.
    let raw = lopdf::Document::load(&path).unwrap();
    let (_, type0) = unicode_font(&raw, a.id.unwrap());
    assert_eq!(name(type0, b"Encoding"), "Identity-H");
    let descendants = get(&raw, type0.get(b"DescendantFonts").unwrap())
        .as_array()
        .unwrap();
    assert_eq!(descendants.len(), 1);
    let cid = dict(&raw, &descendants[0]);
    assert_eq!(name(cid, b"Subtype"), "CIDFontType2");
    assert_eq!(name(cid, b"BaseFont"), f.base_font);
    let info = dict(&raw, cid.get(b"CIDSystemInfo").unwrap());
    assert_eq!(info.get(b"Registry").unwrap().as_str().unwrap(), b"Adobe");
    assert_eq!(info.get(b"Ordering").unwrap().as_str().unwrap(), b"Identity");
    assert_eq!(info.get(b"Supplement").unwrap().as_i64().unwrap(), 0);
    let w = get(&raw, cid.get(b"W").unwrap()).as_array().unwrap();
    assert_eq!(w[0].as_i64().unwrap(), 1);
    let widths: Vec<i64> = w[1]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    let mut distinct: Vec<char> = Vec::new();
    for c in MIXED.chars() {
        if !distinct.contains(&c) {
            distinct.push(c);
        }
    }
    let n = distinct.len();
    assert_eq!(widths.len(), n);
    let map = stream_data(&raw, cid.get(b"CIDToGIDMap").unwrap());
    assert_eq!(map.len(), 2 * (n + 1));
    assert_eq!(&map[..2], &[0, 0]);
    let gids: Vec<u16> = map[2..]
        .chunks(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    let descriptor = dict(&raw, cid.get(b"FontDescriptor").unwrap());
    assert_eq!(name(descriptor, b"FontName"), f.base_font);
    assert_eq!(descriptor.get(b"Flags").unwrap().as_i64().unwrap(), 4);
    let file = descriptor.get(b"FontFile2").unwrap();
    let program = stream_data(&raw, file);
    assert_eq!(
        dict(&raw, file).get(b"Length1").unwrap().as_i64().unwrap(),
        program.len() as i64
    );
    // The font program: every drawn glyph has an outline and the advance written in /W.
    let font = skrifa::FontRef::new(&program).unwrap();
    let metrics = font.glyph_metrics(Size::new(1000.0), LocationRef::default());
    let outlines = font.outline_glyphs();
    // The codes, read back through the ToUnicode map.
    let tu = to_unicode(&stream_data(&raw, type0.get(b"ToUnicode").unwrap()));
    assert_eq!(tu.len(), n);
    for (k, gid) in gids.iter().enumerate() {
        let c = tu[&(k as u16 + 1)].chars().next().unwrap();
        let id = skrifa::GlyphId::new(*gid as u32);
        assert_eq!(
            widths[k],
            metrics.advance_width(id).unwrap().round() as i64,
            "{c}"
        );
        assert_eq!(font.charmap().map(c), Some(id), "{c}");
        if c != ' ' {
            let mut pen = SvgPen::default();
            outlines
                .get(id)
                .unwrap()
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                    &mut pen,
                )
                .unwrap();
            assert!(!pen.to_string().is_empty(), "{c} has an outline");
        }
    }
    let lines = shown_codes(&stream_data(
        &raw,
        &Object::Reference(appearance_id(&raw, a.id.unwrap())),
    ));
    assert!(!lines.is_empty());
    let mut seen = 0u16;
    for code in lines.iter().flatten() {
        assert!(
            *code >= 1 && *code <= seen + 1,
            "codes are given in order of first use"
        );
        seen = seen.max(*code);
    }
    let decoded: Vec<String> = lines
        .iter()
        .map(|l| l.iter().map(|c| tu[c].as_str()).collect())
        .collect();
    assert_eq!(decoded.join(" "), MIXED);
    assert!(!decoded.concat().contains('?'));

    // Flattened, the page text is the typed text, before and after saving again.
    let mut doc = reopen(&path);
    assert_eq!(doc.flatten(&[0], FlattenScope::Comments).unwrap(), 1);
    assert_eq!(doc.page_text(0), MIXED);
    let flat = temp("subset-flat");
    doc.save_as(&flat).unwrap();
    let doc = reopen(&flat);
    assert_eq!(doc.page_text(0), MIXED);
    // Drawn: dark pixels inside the box.
    let png = doc.render_page_png(0, 1.0).unwrap().png;
    let img = image::load_from_memory(&png).unwrap().to_rgba8();
    let dark = (55..540)
        .flat_map(|x| (842 - 638..842 - 602).map(move |y| (x, y)))
        .filter(|(x, y)| img.get_pixel(*x, *y).0[0] < 100)
        .count();
    assert!(dark > 50, "{dark}");
}

fn appearance_id(doc: &lopdf::Document, annot: lopdf::ObjectId) -> lopdf::ObjectId {
    let a = doc.get_dictionary(annot).unwrap();
    let ap = dict(doc, a.get(b"AP").unwrap());
    ap.get(b"N").unwrap().as_reference().unwrap()
}

/// Draw page 0 of a saved file with the renderer and count how often it asked
/// for a font of its own (a standard font, or a fallback for a font it could not load).
fn fonts_asked_for(path: &Path) -> usize {
    use hayro::hayro_interpret::InterpreterSettings;
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let settings = InterpreterSettings {
        font_resolver: Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            None
        }),
        ..InterpreterSettings::default()
    };
    let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(std::fs::read(path).unwrap())).unwrap();
    let pages = pdf.pages();
    let cache = hayro::RenderCache::new();
    let pixmap_settings = hayro::PixmapSettings {
        x_scale: 1.0,
        y_scale: 1.0,
        bg_color: hayro::vello_cpu::color::palette::css::WHITE,
    };
    let _ = hayro::render(
        &pages[0],
        &cache,
        &settings,
        &hayro::RenderSettings::default(),
        &pixmap_settings,
    );
    calls.load(Ordering::SeqCst)
}

#[test]
fn the_embedded_subset_is_drawn_without_any_other_font() {
    // A text box in Helvetica makes the renderer ask for its standard font...
    let mut doc = blank();
    add_box(&mut doc, "Hello").unwrap();
    let helvetica = temp("resolver-helvetica");
    doc.save_as(&helvetica).unwrap();
    assert!(fonts_asked_for(&helvetica) > 0);
    // ...the embedded subset is used as it is: no standard font, no fallback.
    for flattened in [false, true] {
        let mut doc = blank();
        add_box(&mut doc, MIXED).unwrap();
        if flattened {
            doc.flatten(&[0], FlattenScope::Comments).unwrap();
        }
        let path = temp(&format!("resolver-unicode-{flattened}"));
        doc.save_as(&path).unwrap();
        assert_eq!(fonts_asked_for(&path), 0, "flattened: {flattened}");
    }
}

#[test]
fn winansi_text_keeps_the_standard_helvetica_and_its_output() {
    let mut doc = blank();
    doc.add_free_text(0, BOX, "Città è perché € “ok”", "Reviewer", 12.0, BLACK, false)
        .unwrap();
    let fonts = doc.annotation_fonts(0, 0).unwrap();
    assert_eq!(fonts.len(), 1);
    assert_eq!(
        (
            fonts[0].resource.as_str(),
            fonts[0].base_font.as_str(),
            fonts[0].subtype.as_str()
        ),
        ("Helv", "Helvetica", "Type1")
    );
    assert!(!fonts[0].embedded && !fonts[0].to_unicode);
    let raw = doc.inner();
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    let content = String::from_utf8_lossy(&appearance(raw, id).content).to_string();
    assert!(
        content.contains("(Citt\\340 \\350 perch\\351 \\200 \\223ok\\224) Tj"),
        "{content}"
    );
    assert_eq!(count_objects(raw, is_type0), 0);
    assert_eq!(count_objects(raw, has_font_file), 0);
    let da = raw
        .get_dictionary(id)
        .unwrap()
        .get(b"DA")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(da, b"/Helv 12 Tf 0 0 0 rg");
}

#[test]
fn a_character_without_glyph_is_refused_before_any_change() {
    let mut doc = with_redo();
    let before = Unchanged::of(&doc);
    let err = add_box(&mut doc, "Łódź 漢字").unwrap_err();
    match &err {
        Error::Glyphs(GlyphProblem::Missing { font, chars }) => {
            assert_eq!(font, "FreeSans");
            assert_eq!(chars, &vec!['漢', '字']);
        }
        other => panic!("{other:?}"),
    }
    assert!(err.to_string().contains("U+6F22"), "{err}");
    before.check(&doc);
    assert!(doc.annotations(0).unwrap().is_empty());
}

#[test]
fn right_to_left_and_uncomposed_marks_are_refused_before_any_change() {
    let mut doc = with_redo();
    let before = Unchanged::of(&doc);
    assert!(matches!(
        add_box(&mut doc, "שלום"),
        Err(Error::Glyphs(GlyphProblem::NeedsLayout(c))) if c == vec!['ש', 'ל', 'ו', 'ם']
    ));
    assert!(matches!(
        add_box(&mut doc, "x\u{301}"),
        Err(Error::Glyphs(GlyphProblem::NeedsLayout(c))) if c == vec!['\u{301}']
    ));
    assert!(matches!(
        doc.add_callout(0, (20.0, 20.0), BOX, "مرحبا", &AnnotStyle::default(), "Reviewer"),
        Err(Error::Glyphs(GlyphProblem::NeedsLayout(_)))
    ));
    before.check(&doc);
}

#[test]
fn decomposed_accents_are_composed_and_drawn_with_helvetica() {
    let mut doc = blank();
    add_box(&mut doc, "Cafe\u{301}").unwrap();
    assert_eq!(doc.annotations(0).unwrap()[0].contents, "Cafe\u{301}");
    assert!(is_helvetica(&doc, 0, 0));
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    let content = String::from_utf8_lossy(&appearance(doc.inner(), id).content).to_string();
    assert!(content.contains("(Caf\\351) Tj"), "{content}");
}

#[test]
fn control_characters_are_not_stored_nor_drawn_as_accents() {
    let mut doc = blank();
    add_box(&mut doc, "a\u{18}b\u{1}c").unwrap();
    assert_eq!(doc.annotations(0).unwrap()[0].contents, "abc");
    assert!(is_helvetica(&doc, 0, 0));
}

#[test]
fn editing_the_text_redraws_it_and_one_undo_step_restores_it() {
    let mut doc = blank();
    add_box(&mut doc, "Hello").unwrap();
    assert!(is_helvetica(&doc, 0, 0));
    doc.set_annotation_contents(0, 0, "Łódź").unwrap();
    assert!(is_unicode_box(&doc, 0, 0));
    doc.undo().unwrap();
    assert!(is_helvetica(&doc, 0, 0));
    assert_eq!(doc.annotations(0).unwrap()[0].contents, "Hello");
    doc.redo().unwrap();
    assert!(is_unicode_box(&doc, 0, 0));
    // Something to redo, then a refused edit: text, drawing and history stay.
    doc.set_annotation_contents(0, 0, "Łódź!").unwrap();
    doc.undo().unwrap();
    let ap_before = appearance_id(doc.inner(), doc.annotations(0).unwrap()[0].id.unwrap());
    let before = Unchanged::of(&doc);
    assert!(matches!(
        doc.set_annotation_contents(0, 0, "漢"),
        Err(Error::Glyphs(GlyphProblem::Missing { .. }))
    ));
    before.check(&doc);
    assert_eq!(doc.annotations(0).unwrap()[0].contents, "Łódź");
    assert_eq!(
        appearance_id(doc.inner(), doc.annotations(0).unwrap()[0].id.unwrap()),
        ap_before
    );
    assert!(is_unicode_box(&doc, 0, 0));
}

#[test]
fn moving_resizing_and_restyling_keep_the_text() {
    let mut doc = blank();
    add_box(&mut doc, MIXED).unwrap();
    doc.set_annotation_rect(0, 0, [40.0, 500.0, 560.0, 660.0])
        .unwrap();
    assert!(is_unicode_box(&doc, 0, 0));
    let style = AnnotStyle {
        text_color: [1.0, 0.0, 0.0],
        ..doc.annotation_style(0, 0).unwrap()
    };
    doc.set_annotation_style(0, 0, &style).unwrap();
    assert!(is_unicode_box(&doc, 0, 0));
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    let da = doc
        .inner()
        .get_dictionary(id)
        .unwrap()
        .get(b"DA")
        .unwrap()
        .as_str()
        .unwrap()
        .to_vec();
    assert!(String::from_utf8_lossy(&da).ends_with("1 0 0 rg"));
    doc.undo().unwrap();
    assert_eq!(doc.annotation_style(0, 0).unwrap().text_color, BLACK);
    let mut flat = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    flat.flatten(&[0], FlattenScope::Comments).unwrap();
    assert_eq!(flat.page_text(0), MIXED);
}

#[test]
fn drawing_again_with_the_same_characters_uses_the_same_font() {
    let mut doc = blank();
    add_box(&mut doc, MIXED).unwrap();
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    let (font, _) = unicode_font(doc.inner(), id);
    for i in 0..5 {
        doc.move_annotation(0, 0, 3.0, -2.0).unwrap();
        // A taller box draws the same lines.
        let r = doc.annotations(0).unwrap()[0].rect;
        doc.set_annotation_rect(0, 0, [r[0], r[1] - 1.0, r[2], r[3]])
            .unwrap();
        assert_eq!(unicode_font(doc.inner(), id).0, font, "redraw {i}");
    }
    let path = temp("reuse");
    doc.save_as(&path).unwrap();
    let raw = lopdf::Document::load(&path).unwrap();
    assert_eq!(count_objects(&raw, has_font_file), 1);
    assert_eq!(count_objects(&raw, is_type0), 1);
    // Other characters make another subset.
    doc.set_annotation_contents(0, 0, "Łódź").unwrap();
    assert_ne!(unicode_font(doc.inner(), id).0, font);
}

#[test]
fn a_font_changed_behind_its_key_is_not_used_again() {
    let mut doc = blank();
    add_box(&mut doc, "łabba").unwrap();
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    let (font, type0) = unicode_font(doc.inner(), id);
    let raw = doc.inner();
    let tu_ref = type0.get(b"ToUnicode").unwrap().as_reference().unwrap();
    let cid = dict(
        raw,
        &get(raw, type0.get(b"DescendantFonts").unwrap())
            .as_array()
            .unwrap()[0],
    );
    let map_ref = cid.get(b"CIDToGIDMap").unwrap().as_reference().unwrap();
    // Code 1 (ł) made to show and read as the glyph of code 2 (a); the key stays.
    let mut map = stream_data(raw, &Object::Reference(map_ref));
    let a_glyph = [map[4], map[5]];
    map[2..4].copy_from_slice(&a_glyph);
    let tu = String::from_utf8(stream_data(raw, &Object::Reference(tu_ref)))
        .unwrap()
        .replace("<0001> <0142>", "<0001> <0061>");
    doc.begin_edit();
    let objects = &mut doc.inner_mut().objects;
    objects.insert(
        map_ref,
        Object::Stream(lopdf::Stream::new(Dictionary::new(), map)),
    );
    objects.insert(
        tu_ref,
        Object::Stream(lopdf::Stream::new(Dictionary::new(), tu.into_bytes())),
    );
    doc.commit().unwrap();
    let flattened = |doc: &Document| {
        let mut copy = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
        copy.flatten(&[0], FlattenScope::Comments).unwrap();
        copy.page_text(0)
    };
    assert_eq!(flattened(&doc), "aabba", "the changed font reads as other text");
    // Drawn again: the changed objects are not taken for the subset of the text.
    doc.move_annotation(0, 0, 5.0, 0.0).unwrap();
    assert_ne!(unicode_font(doc.inner(), id).0, font);
    assert_eq!(flattened(&doc), "łabba");
    // An unchanged subset is still used again.
    let (again, _) = unicode_font(doc.inner(), id);
    doc.move_annotation(0, 0, 5.0, 0.0).unwrap();
    assert_eq!(unicode_font(doc.inner(), id).0, again);
}

/// A text box made by another program: its text (with characters FreeSans
/// lacks) and a drawing of its own.
fn foreign_box(text: &str) -> Document {
    let mut doc = blank();
    add_box(&mut doc, "placeholder").unwrap();
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    doc.begin_edit();
    doc.inner_mut()
        .get_dictionary_mut(id)
        .unwrap()
        .set("Contents", lopdf::text_string(text));
    doc.commit().unwrap();
    doc.clear_history();
    doc
}

#[test]
fn a_text_box_that_cannot_be_drawn_again_refuses_style_and_size_but_moves() {
    let mut doc = foreign_box("漢字");
    // Something to redo.
    doc.add_note(0, 20.0, 820.0, "note", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    doc.undo().unwrap();
    let before = Unchanged::of(&doc);
    let style = AnnotStyle {
        text_color: [1.0, 0.0, 0.0],
        ..doc.annotation_style(0, 0).unwrap()
    };
    assert!(matches!(
        doc.set_annotation_style(0, 0, &style),
        Err(Error::Glyphs(GlyphProblem::Missing { .. }))
    ));
    before.check(&doc);
    let resized = [BOX[0], BOX[1] - 40.0, BOX[2], BOX[3]];
    assert!(matches!(
        doc.set_annotation_rect(0, 0, resized),
        Err(Error::Glyphs(GlyphProblem::Missing { .. }))
    ));
    before.check(&doc);
    // A move keeps the drawing it has (no '?'), moved with the rectangle.
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    let ap = appearance_id(doc.inner(), id);
    doc.move_annotation(0, 0, 10.0, -20.0).unwrap();
    assert_eq!(appearance_id(doc.inner(), id), ap);
    let r = doc.annotations(0).unwrap()[0].rect;
    assert_eq!(r, [BOX[0] + 10.0, BOX[1] - 20.0, BOX[2] + 10.0, BOX[3] - 20.0]);
    doc.shift_annotations(0, &[0], 5.0, 5.0).unwrap();
    assert_eq!(appearance_id(doc.inner(), id), ap);
}

#[test]
fn typewriter_text_and_callouts_use_the_unicode_font() {
    let mut doc = blank();
    doc.add_free_text(
        0,
        [50.0, 700.0, 300.0, 730.0],
        "Ελληνικά",
        "Reviewer",
        12.0,
        BLACK,
        false,
    )
    .unwrap();
    doc.add_callout(
        0,
        (400.0, 400.0),
        [50.0, 500.0, 300.0, 540.0],
        "Кириллица",
        &AnnotStyle::default(),
        "Reviewer",
    )
    .unwrap();
    assert!(is_unicode_box(&doc, 0, 0));
    assert!(is_unicode_box(&doc, 0, 1));
    doc.flatten(&[0], FlattenScope::Comments).unwrap();
    let text = doc.page_text(0);
    assert!(text.contains("Ελληνικά") && text.contains("Кириллица"), "{text}");
}

#[test]
fn look_alike_letters_extract_as_typed() {
    let mut doc = blank();
    add_box(&mut doc, "AΑА").unwrap();
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    let raw = doc.inner();
    let lines = shown_codes(&appearance(raw, id).content);
    assert_eq!(lines, vec![vec![1, 2, 3]]);
    doc.flatten(&[0], FlattenScope::Comments).unwrap();
    assert_eq!(doc.page_text(0), "AΑА");
}

#[test]
fn wrapped_and_aligned_lines_are_measured_with_the_font() {
    let text = "Łódź Łódź Łódź Łódź Łódź Łódź Łódź Łódź";
    for align in [1u8, 2] {
        let mut doc = blank();
        let style = AnnotStyle {
            align,
            font_size: 12.0,
            ..AnnotStyle::default()
        };
        doc.add_free_text_styled(0, [100.0, 500.0, 220.0, 640.0], text, "Reviewer", &style)
            .unwrap();
        let id = doc.annotations(0).unwrap()[0].id.unwrap();
        let content = String::from_utf8_lossy(&appearance(doc.inner(), id).content).to_string();
        let lines = shown_codes(content.as_bytes());
        assert!(lines.len() >= 3, "{content}");
        // Each line ends at the right edge (2) or is centred (1): its x
        // position follows from the widths of /W.
        let xs: Vec<f32> = content
            .split(" Tm ")
            .filter_map(|part| part.rsplit("1 0 0 1 ").next())
            .filter_map(|tm| tm.split_whitespace().next().and_then(|x| x.parse().ok()))
            .collect();
        assert_eq!(xs.len(), lines.len());
        let width_of = |codes: &[u16]| -> f32 {
            // Ł, ó, d, ź and the space: widths of FreeSans in 1/1000 em.
            let raw = doc.inner();
            let (_, type0) = unicode_font(raw, id);
            let cid = dict(
                raw,
                &get(raw, type0.get(b"DescendantFonts").unwrap())
                    .as_array()
                    .unwrap()[0],
            );
            let w = get(raw, cid.get(b"W").unwrap()).as_array().unwrap()[1]
                .as_array()
                .unwrap()
                .clone();
            codes
                .iter()
                .map(|c| w[*c as usize - 1].as_float().unwrap())
                .sum::<f32>()
                * 12.0
                / 1000.0
        };
        for (line, x) in lines.iter().zip(&xs) {
            let lw = width_of(line);
            let expected = if align == 2 {
                220.0 - 3.0 - lw
            } else {
                100.0 + 3.0 + (120.0 - 6.0 - lw) / 2.0
            };
            assert!((x - expected).abs() < 0.01, "align {align}: {x} vs {expected}");
            assert!(lw <= 120.0 - 6.0 + 0.01);
        }
        doc.flatten(&[0], FlattenScope::Comments).unwrap();
        assert_eq!(doc.page_text(0), text);
    }
}

#[test]
fn text_on_a_turned_page_is_saved_and_read_back() {
    let mut doc = blank();
    doc.rotate_pages(&[0], 90).unwrap();
    add_box(&mut doc, MIXED).unwrap();
    let path = temp("turned");
    doc.save_as(&path).unwrap();
    let mut doc = reopen(&path);
    assert!(is_unicode_box(&doc, 0, 0));
    doc.flatten(&[0], FlattenScope::Comments).unwrap();
    assert_eq!(doc.page_text(0), MIXED);
}

fn xfdf_with(texts: &[&str]) -> String {
    let mut doc = blank();
    for (i, t) in texts.iter().enumerate() {
        let y = 700.0 - 60.0 * i as f32;
        doc.add_free_text(0, [50.0, y, 400.0, y + 40.0], t, "Reviewer", 12.0, BLACK, true)
            .unwrap();
    }
    String::from_utf8(doc.export_comments(CommentFormat::Xfdf, None).unwrap()).unwrap()
}

#[test]
fn xfdf_exchange_redraws_unicode_and_refuses_or_skips_what_cannot_be_drawn() {
    let xfdf = xfdf_with(&[MIXED]);
    let mut doc = blank();
    assert_eq!(doc.import_comments(xfdf.as_bytes()).unwrap(), 1);
    assert_eq!(doc.annotations(0).unwrap()[0].contents, MIXED);
    assert!(is_unicode_box(&doc, 0, 0));

    // Three comments, one of them in a script FreeSans lacks.
    let xfdf = xfdf_with(&["Łódź", "first", "Ελληνικά"]).replace("Łódź", "漢字");
    let mut doc = with_redo();
    let before = Unchanged::of(&doc);
    match doc.import_comments(xfdf.as_bytes()) {
        Err(Error::Undrawable(list)) => {
            assert_eq!(list.len(), 1);
            assert_eq!((list[0].page, list[0].author.as_str()), (0, "Reviewer"));
            assert_eq!(list[0].contents, "漢字");
            assert!(!list[0].name.is_empty());
            assert!(
                matches!(&list[0].problem, GlyphProblem::Missing { chars, .. } if chars == &vec!['漢', '字'])
            );
        }
        other => panic!("{other:?}"),
    }
    let message = doc.import_comments(xfdf.as_bytes()).unwrap_err().to_string();
    assert!(
        message.contains("U+6F22") && message.contains("page 1"),
        "{message}"
    );
    before.check(&doc);
    // The explicit choice: import the others and leave that one out.
    let (n, skipped) = doc.import_comments_skipping(xfdf.as_bytes()).unwrap();
    assert_eq!((n, skipped.len()), (2, 1));
    let contents: Vec<String> = doc.comments().iter().map(|c| c.contents.clone()).collect();
    assert_eq!(contents, vec!["first", "Ελληνικά"]);
    assert!(is_unicode_box(&doc, 0, 1));
    // One undo step takes the import back.
    doc.undo().unwrap();
    assert!(doc.comments().is_empty());
}

#[test]
fn replies_to_a_left_out_comment_are_left_out_with_it() {
    let mut source = blank();
    source
        .add_free_text(
            0,
            [50.0, 700.0, 400.0, 740.0],
            "PLACEHOLDER",
            "Reviewer",
            12.0,
            BLACK,
            true,
        )
        .unwrap();
    source
        .add_free_text(
            0,
            [50.0, 600.0, 400.0, 640.0],
            "first",
            "Reviewer",
            12.0,
            BLACK,
            true,
        )
        .unwrap();
    source.add_reply(0, 0, "agreed", "Second").unwrap();
    source.add_reply(0, 1, "fine", "Second").unwrap();
    // A reply to the reply goes with it too.
    let agreed = source
        .annotations(0)
        .unwrap()
        .iter()
        .position(|a| a.contents == "agreed")
        .unwrap();
    source.add_reply(0, agreed, "me too", "Third").unwrap();
    let xfdf = String::from_utf8(source.export_comments(CommentFormat::Xfdf, None).unwrap())
        .unwrap()
        .replace("PLACEHOLDER", "漢字");
    let mut doc = blank();
    match doc.import_comments(xfdf.as_bytes()) {
        Err(Error::Undrawable(list)) => {
            assert_eq!(list.len(), 1);
            let replies: Vec<(&str, &str)> = list[0]
                .replies
                .iter()
                .map(|r| (r.author.as_str(), r.contents.as_str()))
                .collect();
            assert_eq!(replies, vec![("Second", "agreed"), ("Third", "me too")]);
            let message = Error::Undrawable(list).to_string();
            assert!(
                message.contains("agreed") && message.contains("me too"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    let (n, skipped) = doc.import_comments_skipping(xfdf.as_bytes()).unwrap();
    assert_eq!((n, skipped.len()), (2, 1));
    let comments = doc.comments();
    let texts: Vec<(&str, bool)> = comments
        .iter()
        .map(|c| (c.contents.as_str(), c.is_reply))
        .collect();
    assert_eq!(texts, vec![("first", false), ("fine", true)]);
    // No pop-up is left without its comment.
    let all = doc.annotations(0).unwrap();
    let ids: Vec<lopdf::ObjectId> = all.iter().filter_map(|a| a.id).collect();
    for a in all.iter().filter(|a| a.subtype == "Popup") {
        let popup = doc.inner().get_dictionary(a.id.unwrap()).unwrap();
        let parent = popup.get(b"Parent").unwrap().as_reference().unwrap();
        assert!(ids.contains(&parent));
    }
}

#[test]
fn fdf_and_pdf_imports_keep_the_embedded_subset() {
    let mut source = blank();
    add_box(&mut source, MIXED).unwrap();
    let fdf = source.export_comments(CommentFormat::Fdf, None).unwrap();
    let pdf = source.bytes().to_vec();
    for data in [fdf, pdf] {
        let mut doc = blank();
        assert_eq!(doc.import_comments(&data).unwrap(), 1);
        assert!(is_unicode_box(&doc, 0, 0));
        let path = temp("imported");
        doc.save_as(&path).unwrap();
        let mut doc = reopen(&path);
        doc.flatten(&[0], FlattenScope::Comments).unwrap();
        assert_eq!(doc.page_text(0), MIXED);
    }
}

#[test]
fn copies_carry_their_own_font() {
    let mut doc = Document::new_blank(2, 595.0, 842.0).unwrap();
    add_box(&mut doc, MIXED).unwrap();
    let snap = doc.annotation_snapshot(0, 0).unwrap();
    doc.paste_annotation_snapshot(1, &snap, 0.0, 0.0).unwrap();
    doc.duplicate_annotation(0, 0, 0, 0.0, -100.0).unwrap();
    let ids: Vec<lopdf::ObjectId> = [(0, 0), (1, 0), (0, 1)]
        .iter()
        .map(|(p, i)| unicode_font(doc.inner(), doc.annotations(*p).unwrap()[*i].id.unwrap()).0)
        .collect();
    assert!(
        ids[0] != ids[1] && ids[0] != ids[2] && ids[1] != ids[2],
        "{ids:?}"
    );
    let path = temp("copies");
    doc.save_as(&path).unwrap();
    let mut doc = reopen(&path);
    doc.flatten(&[0, 1], FlattenScope::Comments).unwrap();
    assert_eq!(doc.page_text(1), MIXED);
    assert_eq!(doc.page_text(0), format!("{MIXED} {MIXED}"));
}

/// A blank document saved with the encryption of `version`.
fn encrypted_file(name: &str, version: &str) -> Temp {
    let path = temp(name);
    let plain = blank().bytes();
    let mut doc = lopdf::Document::load_mem(&plain).unwrap();
    let id = vec![7u8; 16];
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(id.clone(), lopdf::StringFormat::Hexadecimal),
            Object::String(id, lopdf::StringFormat::Hexadecimal),
        ]),
    );
    let state = match version {
        "RC4-128" => lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
            document: &doc,
            owner_password: "owner",
            user_password: "user",
            key_length: 128,
            permissions: lopdf::Permissions::all(),
        })
        .unwrap(),
        _ => {
            use lopdf::encryption::crypt_filters::{Aes128CryptFilter, Aes256CryptFilter, CryptFilter};
            let mut filters: std::collections::BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = Default::default();
            if version == "AES-128" {
                filters.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));
                lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V4 {
                    document: &doc,
                    encrypt_metadata: true,
                    crypt_filters: filters,
                    stream_filter: b"StdCF".to_vec(),
                    string_filter: b"StdCF".to_vec(),
                    owner_password: "owner",
                    user_password: "user",
                    permissions: lopdf::Permissions::all(),
                })
                .unwrap()
            } else {
                filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));
                lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V5 {
                    encrypt_metadata: true,
                    crypt_filters: filters,
                    file_encryption_key: &[3u8; 32],
                    stream_filter: b"StdCF".to_vec(),
                    string_filter: b"StdCF".to_vec(),
                    owner_password: "owner",
                    user_password: "user",
                    permissions: lopdf::Permissions::all(),
                })
                .unwrap()
            }
        }
    };
    doc.encrypt(&state).unwrap();
    doc.save(&path).unwrap();
    path
}

#[test]
fn protected_files_keep_unicode_text_encrypted() {
    for version in ["RC4-128", "AES-128", "AES-256"] {
        let original = encrypted_file(&format!("protected-{version}"), version);
        let mut doc = Document::open_with_password(&original, "user").unwrap();
        assert!(doc.is_protected(), "{version}");
        add_box(&mut doc, MIXED).unwrap();
        let saved = temp(&format!("protected-{version}-saved"));
        doc.save_as(&saved).unwrap();
        // Without the password, nothing of the font or its text can be read.
        assert!(
            matches!(Document::open(&saved), Err(Error::PasswordRequired)),
            "{version}"
        );
        let bytes = std::fs::read(&saved).unwrap();
        let raw = lopdf::Document::load_mem(&bytes).unwrap();
        for o in raw.objects.values() {
            if let Ok(s) = o.as_stream() {
                let plain = s.decompressed_content().unwrap_or_default();
                for secret in [&b"beginbfchar"[..], b"glyf", b"FreeSans"] {
                    assert!(!plain.windows(secret.len()).any(|w| w == secret), "{version}");
                    assert!(!s.content.windows(secret.len()).any(|w| w == secret), "{version}");
                }
            }
        }
        // With it, the font and its map decode and the text reads back.
        let mut doc = Document::open_with_password(&saved, "user").unwrap();
        assert!(is_unicode_box(&doc, 0, 0), "{version}");
        let id = doc.annotations(0).unwrap()[0].id.unwrap();
        let raw = doc.inner();
        let (_, type0) = unicode_font(raw, id);
        let tu = to_unicode(&stream_data(raw, type0.get(b"ToUnicode").unwrap()));
        assert!(tu.values().any(|t| t == "Ł"), "{version}");
        doc.flatten(&[0], FlattenScope::Comments).unwrap();
        assert_eq!(doc.page_text(0), MIXED, "{version}");
    }
}

#[test]
fn signed_files_get_only_new_objects_and_keep_their_signature() {
    let id = DigitalId::create_self_signed(
        &IdentityInfo {
            name: "Signer".into(),
            organization: "Example".into(),
            unit: "Legal".into(),
            email: "signer@example.org".into(),
            country: "IT".into(),
        },
        KeyKind::EcP256,
        3,
    )
    .unwrap();
    let signed = blank().sign(&id, &SignOptions::default()).unwrap();
    let before = Document::from_bytes(signed.clone(), None, None).unwrap();
    let mut doc = Document::from_bytes(signed.clone(), None, None).unwrap();
    add_box(&mut doc, MIXED).unwrap();
    let path = temp("signed");
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(
        saved.starts_with(&signed),
        "an incremental update keeps the signed bytes"
    );
    let edited = Document::from_bytes(saved, None, None).unwrap();
    let status = edited.validate_signatures(&[]);
    assert!(status[0].valid, "{status:?}");
    // Every object of the font is new: the signed revision had none of them.
    let annot = edited.comments()[0].id.unwrap();
    let raw = edited.inner();
    let (type0_id, type0) = unicode_font(raw, annot);
    let cid_ref = get(raw, type0.get(b"DescendantFonts").unwrap())
        .as_array()
        .unwrap()[0]
        .as_reference()
        .unwrap();
    let cid = raw.get_dictionary(cid_ref).unwrap();
    let descriptor_ref = cid.get(b"FontDescriptor").unwrap().as_reference().unwrap();
    let descriptor = raw.get_dictionary(descriptor_ref).unwrap();
    for id in [
        type0_id,
        cid_ref,
        descriptor_ref,
        type0.get(b"ToUnicode").unwrap().as_reference().unwrap(),
        cid.get(b"CIDToGIDMap").unwrap().as_reference().unwrap(),
        descriptor.get(b"FontFile2").unwrap().as_reference().unwrap(),
    ] {
        assert!(!before.inner().objects.contains_key(&id), "{id:?} is new");
    }
    let mut flat = edited;
    flat.flatten(&[0], FlattenScope::Comments).unwrap();
    assert!(flat.page_text(0).contains("Łódź"));
}

#[test]
fn redaction_removes_the_subset_of_a_removed_comment() {
    let mut doc = Document::from_text(
        "Secret plan for Monday\nPublic notes stay here",
        TextLayout::default(),
    )
    .unwrap();
    let size = doc.page_size(0).unwrap();
    let area = [40.0, size.height - 120.0, 400.0, size.height - 40.0];
    doc.add_free_text(0, area, "Łódź tajny plan", "Reviewer", 12.0, BLACK, true)
        .unwrap();
    assert!(is_unicode_box(&doc, 0, 0));
    doc.add_redaction(0, area, "Reviewer").unwrap();
    doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    let path = temp("redacted");
    doc.save_as(&path).unwrap();
    let raw = lopdf::Document::load(&path).unwrap();
    assert_eq!(count_objects(&raw, is_type0), 0);
    assert_eq!(count_objects(&raw, has_font_file), 0);
    for o in raw.objects.values() {
        if let Ok(s) = o.as_stream() {
            let plain = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
            assert!(!plain.windows(11).any(|w| w == b"beginbfchar"));
        }
        if let Ok(d) = o.as_dict() {
            if let Ok(base) = d.get(b"BaseFont").and_then(|b| b.as_name()) {
                assert!(!base.ends_with(b"+FreeSans"));
            }
        }
    }
}

#[test]
fn text_stamps_refuse_text_beyond_winansi() {
    let mut doc = with_redo();
    let before = Unchanged::of(&doc);
    assert!(matches!(
        doc.add_stamp(0, [100.0, 100.0, 300.0, 160.0], "Łódź", [0.8, 0.1, 0.1], "Reviewer"),
        Err(Error::Glyphs(GlyphProblem::StampText(chars))) if chars == vec!['Ł', 'ź']
    ));
    let message = doc
        .add_stamp(
            0,
            [100.0, 100.0, 300.0, 160.0],
            "Łódź",
            [0.8, 0.1, 0.1],
            "Reviewer",
        )
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("text stamps can draw only Western European characters"),
        "{message}"
    );
    assert!(!message.contains("Helvetica"), "{message}");
    before.check(&doc);
    doc.add_stamp(
        0,
        [100.0, 100.0, 300.0, 160.0],
        "APPROVED",
        [0.8, 0.1, 0.1],
        "Reviewer",
    )
    .unwrap();
    let before = Unchanged::of(&doc);
    assert!(matches!(
        doc.set_annotation_contents(0, 0, "Approvato à Łódź"),
        Err(Error::Glyphs(GlyphProblem::StampText(_)))
    ));
    before.check(&doc);
    assert_eq!(doc.annotations(0).unwrap()[0].contents, "APPROVED");
    // Text in WinAnsi is still drawn.
    doc.set_annotation_contents(0, 0, "Approvato à Milano").unwrap();
    let id = doc.annotations(0).unwrap()[0].id.unwrap();
    let content = appearance(doc.inner(), id).content.clone();
    assert!(!String::from_utf8_lossy(&content).contains('?'));
}

#[test]
fn measurement_labels_beyond_winansi_use_the_unicode_font() {
    let mut doc = blank();
    let scale = MeasureScale {
        unit: "мм".into(),
        ..MeasureScale::default()
    };
    doc.add_measurement(
        0,
        MeasureKind::Distance,
        &[(100.0, 400.0), (300.0, 400.0)],
        &AnnotStyle::default(),
        &scale,
        "Reviewer",
    )
    .unwrap();
    assert!(is_unicode_box(&doc, 0, 0));
    let contents = doc.annotations(0).unwrap()[0].contents.clone();
    assert!(contents.ends_with("мм"), "{contents}");
    doc.flatten(&[0], FlattenScope::Comments).unwrap();
    assert!(doc.page_text(0).contains("мм"), "{}", doc.page_text(0));
}

#[test]
fn spelling_corrections_redraw_unicode_text_and_refusals_change_nothing() {
    let mut doc = blank();
    add_box(&mut doc, "Łódź is beautifl").unwrap();
    let text = doc.annotations(0).unwrap()[0].contents.clone();
    let start = text.chars().position(|c| c == 'b').unwrap();
    let m = Misspelling {
        place: SpellPlace::Comment { page: 0, index: 0 },
        page: Some(0),
        order: 0,
        start,
        end: start + "beautifl".chars().count(),
        word: "beautifl".into(),
    };
    doc.correct_spelling(&m, "beautiful").unwrap();
    assert_eq!(doc.annotations(0).unwrap()[0].contents, "Łódź is beautiful");
    assert!(is_unicode_box(&doc, 0, 0));
    // A correction in a text box that cannot be drawn again is counted as refused.
    let mut doc = foreign_box("漢字 helo");
    let before = Unchanged::of(&doc);
    let m = Misspelling {
        place: SpellPlace::Comment { page: 0, index: 0 },
        page: Some(0),
        order: 0,
        start: 3,
        end: 7,
        word: "helo".into(),
    };
    let report = doc.correct_spellings(&[m], "hello");
    assert_eq!((report.replaced, report.refused), (0, 1));
    before.check(&doc);
}
