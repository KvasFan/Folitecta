//! Documents generated in memory for measuring and testing: dense text,
//! vector drawings, photos, scanned pages, long documents with comments, and
//! the parts real files are made of (embedded TrueType, CFF and CID fonts,
//! forms, compressed object streams, soft masks, shadings and transparency
//! groups). The same bytes come out of every run (a fixed random sequence),
//! and nothing is read from the disk.
//!
//! The render benchmark (`examples/render_bench.rs`) and the performance step
//! of the interface self-test use them.

use image::ExtendedColorType;
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

/// A small random number generator (xorshift), so the documents are the same every run.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number from 0 to 1.
    pub fn f(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// A document built page by page from content streams, resources and
/// annotations.
pub struct Builder {
    pub doc: lopdf::Document,
    pages_id: ObjectId,
    kids: Vec<Object>,
    catalog: Dictionary,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        let mut doc = lopdf::Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        Builder {
            doc,
            pages_id,
            kids: Vec::new(),
            catalog: Dictionary::new(),
        }
    }

    /// Add a page; returns its object id.
    pub fn page(
        &mut self,
        size: (f32, f32),
        content: Vec<u8>,
        resources: Dictionary,
        annots: Vec<Object>,
    ) -> ObjectId {
        let mut stream = Stream::new(Dictionary::new(), content);
        let _ = stream.compress();
        let content_id = self.doc.add_object(stream);
        let mut page = dictionary! {
            "Type" => "Page",
            "Parent" => self.pages_id,
            "MediaBox" => vec![0.into(), 0.into(), size.0.into(), size.1.into()],
            "Contents" => content_id,
            "Resources" => resources,
        };
        if !annots.is_empty() {
            page.set("Annots", annots);
        }
        let id = self.doc.add_object(page);
        self.kids.push(id.into());
        id
    }

    pub fn image(&mut self, w: u32, h: u32, color: &str, bits: i64, filter: &str, data: Vec<u8>) -> ObjectId {
        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => w as i64,
            "Height" => h as i64,
            "ColorSpace" => color,
            "BitsPerComponent" => bits,
            "Filter" => filter,
        };
        self.doc.add_object(Stream::new(dict, data))
    }

    /// A form XObject drawing `content` in `bbox`, with extra dictionary entries.
    pub fn form(
        &mut self,
        bbox: [f32; 4],
        content: &str,
        resources: Dictionary,
        extra: Dictionary,
    ) -> ObjectId {
        let mut dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => bbox.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
            "Resources" => resources,
        };
        dict.extend(&extra);
        let mut stream = Stream::new(dict, content.as_bytes().to_vec());
        let _ = stream.compress();
        self.doc.add_object(stream)
    }

    /// An entry of the catalog (an `AcroForm`, for example).
    pub fn catalog_entry(&mut self, key: &str, value: Object) {
        self.catalog.set(key, value);
    }

    fn close(&mut self) {
        let count = self.kids.len() as i64;
        let kids = std::mem::take(&mut self.kids);
        self.doc.objects.insert(
            self.pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
        );
        let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => self.pages_id };
        catalog.extend(&self.catalog);
        let catalog = self.doc.add_object(catalog);
        self.doc.trailer.set("Root", catalog);
    }

    /// The file, with a classic cross-reference table.
    pub fn finish(mut self) -> Vec<u8> {
        self.close();
        let mut out = Vec::new();
        self.doc.save_to(&mut out).expect("save");
        out
    }

    /// The file with its objects in compressed object streams and a
    /// cross-reference stream, as most producers write them today.
    pub fn finish_compact(mut self) -> Vec<u8> {
        self.close();
        let options = lopdf::SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .max_objects_per_stream(100)
            .compression_level(6)
            .build();
        let mut out = Vec::new();
        self.doc.save_with_options(&mut out, options).expect("save");
        out
    }
}

const WORDS: [&str; 24] = [
    "document",
    "page",
    "render",
    "the",
    "of",
    "and",
    "picture",
    "quick",
    "brown",
    "fox",
    "jumps",
    "over",
    "lazy",
    "dog",
    "zoom",
    "scroll",
    "editor",
    "comment",
    "field",
    "signature",
    "layer",
    "outline",
    "text",
    "print",
];

/// Words up to about `chars` characters.
pub fn line_text(rng: &mut Rng, chars: usize) -> String {
    let mut s = String::new();
    while s.len() < chars {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(WORDS[(rng.next_u64() % WORDS.len() as u64) as usize]);
    }
    s
}

/// Three standard fonts (not embedded): F1 Helvetica, F2 Times, F3 Helvetica Bold.
pub fn standard_fonts() -> Dictionary {
    let font = |name: &str| {
        Object::Dictionary(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => name, "Encoding" => "WinAnsiEncoding"
        })
    };
    dictionary! { "F1" => font("Helvetica"), "F2" => font("Times-Roman"), "F3" => font("Helvetica-Bold") }
}

/// Two columns of dense text with fonts `/F1` and `/F2` and a title in
/// `/F3`: `lines` lines a column from `top` down (70 lines from 776 fill an
/// A4 page with about 9000 glyphs).
fn text_columns(rng: &mut Rng, page: usize, top: u32, lines: usize) -> String {
    let mut c = format!(
        "BT /F3 15 Tf 50 800 Td (Chapter {} - {}) Tj ET\n",
        page + 1,
        line_text(rng, 30)
    );
    for col in 0..2 {
        let x = 50 + col * 255;
        c.push_str(&format!("BT /F{} 8.5 Tf 10.4 TL {x} {top} Td\n", col + 1));
        for _ in 0..lines {
            c.push_str(&format!("({}) '\n", line_text(rng, 58)));
        }
        c.push_str("ET\n");
    }
    c
}

/// A4 pages of dense text in two columns (about 9000 glyphs a page), standard fonts.
pub fn text_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let mut rng = Rng(7);
    for p in 0..pages {
        let c = text_columns(&mut rng, p, 776, 70);
        b.page(
            (595.0, 842.0),
            c.into_bytes(),
            dictionary! { "Font" => standard_fonts() },
            vec![],
        );
    }
    b.finish()
}

/// The drawing of a vector page: a grid, `shapes` transparent hexagons and
/// `lines` polylines of 8 curves each, on an area of `w` × `h` points.
fn vector_content(rng: &mut Rng, w: f32, h: f32, shapes: usize, lines: usize) -> String {
    let mut c = String::from("0.2 w 0.8 0.8 0.85 RG\n");
    let step = 6.85;
    let cols = ((w - 20.0) / step) as usize;
    let rows = ((h - 20.0) / step) as usize;
    for i in 0..=cols {
        let x = 10.0 + i as f32 * step;
        c.push_str(&format!("{x:.2} 10 m {x:.2} {:.2} l\n", h - 10.0));
    }
    for i in 0..=rows {
        let y = 10.0 + i as f32 * step;
        c.push_str(&format!("10 {y:.2} m {:.2} {y:.2} l\n", w - 10.0));
    }
    c.push_str("S\n/GS1 gs\n");
    for _ in 0..shapes {
        let (cx, cy, r) = (rng.f() * w, rng.f() * h, 4.0 + rng.f() * 30.0);
        c.push_str(&format!("{:.3} {:.3} {:.3} rg\n", rng.f(), rng.f(), rng.f()));
        for k in 0..6 {
            let a = k as f32 * std::f32::consts::TAU / 6.0;
            let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
            c.push_str(&format!("{x:.2} {y:.2} {}\n", if k == 0 { "m" } else { "l" }));
        }
        c.push_str("h f\n");
    }
    c.push_str("/GS0 gs\n");
    for _ in 0..lines {
        c.push_str(&format!(
            "{:.2} w {:.3} {:.3} {:.3} RG\n",
            0.3 + rng.f(),
            rng.f(),
            rng.f(),
            rng.f()
        ));
        let (mut x, mut y) = (rng.f() * w, rng.f() * h);
        c.push_str(&format!("{x:.2} {y:.2} m\n"));
        for _ in 0..8 {
            let pts: Vec<f32> = (0..6).map(|_| (rng.f() - 0.5) * 40.0).collect();
            c.push_str(&format!(
                "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n",
                x + pts[0],
                y + pts[1],
                x + pts[2],
                y + pts[3],
                x + pts[4],
                y + pts[5]
            ));
            x += pts[4];
            y += pts[5];
        }
        c.push_str("S\n");
    }
    c
}

fn vector_states() -> Dictionary {
    dictionary! {
        "GS0" => dictionary! { "Type" => "ExtGState", "ca" => 1.0, "CA" => 1.0 },
        "GS1" => dictionary! { "Type" => "ExtGState", "ca" => 0.5 },
    }
}

/// Drawings: 2000 polylines of 8 curves each, 800 shapes filled with
/// transparency and a fine grid on every page (A4 landscape).
pub fn vector_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let mut rng = Rng(11);
    for _ in 0..pages {
        let c = vector_content(&mut rng, 842.0, 595.0, 800, 2000);
        b.page(
            (842.0, 595.0),
            c.into_bytes(),
            dictionary! { "ExtGState" => vector_states() },
            vec![],
        );
    }
    b.finish()
}

/// JPEG bytes of `pixels`.
pub fn jpeg(pixels: &[u8], w: u32, h: u32, color: ExtendedColorType, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode(pixels, w, h, color)
        .expect("JPEG");
    out
}

/// Landscape A4 pages, each covered by a different 4000×3000 photo (JPEG).
pub fn photo_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let (w, h) = (4000u32, 3000u32);
    for p in 0..pages {
        let mut rng = Rng(100 + p as u64);
        let mut px = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
                let wave = ((fx * 23.0 + p as f32).sin() * (fy * 17.0).cos() * 0.5 + 0.5) * 120.0;
                let n = (rng.next_u64() & 31) as f32;
                px.push((fx * 200.0 + n) as u8);
                px.push((wave + n) as u8);
                px.push((fy * 180.0 + 40.0 - n) as u8);
            }
        }
        let img = b.image(
            w,
            h,
            "DeviceRGB",
            8,
            "DCTDecode",
            jpeg(&px, w, h, ExtendedColorType::Rgb8, 85),
        );
        let content = b"q 842 0 0 595 0 0 cm /Im0 Do Q\n".to_vec();
        b.page(
            (842.0, 595.0),
            content,
            dictionary! { "XObject" => dictionary! { "Im0" => img } },
            vec![],
        );
    }
    b.finish()
}

/// Scanned A4 pages at 300 dpi (2480×3508): grey JPEG pages and a last page
/// in black and white (1 bit, Flate).
pub fn scan_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let (w, h) = (2480u32, 3508u32);
    for p in 0..pages {
        let mut rng = Rng(300 + p as u64);
        let mut grey = vec![0u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let paper = 235 + (rng.next_u64() % 16) as u8;
                // Lines of "words": dark blocks 40 px high every 70 px, between the margins.
                let in_line = y > 300 && y < 3300 && (y % 70) < 40 && x > 250 && x < 2230;
                let in_word = (x / 37 + y / 70 * 7) % 9 != 0 && ((x * 7 + y / 70) % 37) > 6;
                grey[(y * w + x) as usize] = if in_line && in_word {
                    30 + (rng.next_u64() % 40) as u8
                } else {
                    paper
                };
            }
        }
        let img = if p + 1 == pages && pages > 1 {
            // Black and white: 1 bit per pixel, rows padded to bytes.
            let row = w.div_ceil(8) as usize;
            let mut bits = vec![0u8; row * h as usize];
            for y in 0..h as usize {
                for x in 0..w as usize {
                    if grey[y * w as usize + x] > 128 {
                        bits[y * row + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            std::io::Write::write_all(&mut e, &bits).expect("deflate");
            b.image(w, h, "DeviceGray", 1, "FlateDecode", e.finish().expect("deflate"))
        } else {
            b.image(
                w,
                h,
                "DeviceGray",
                8,
                "DCTDecode",
                jpeg(&grey, w, h, ExtendedColorType::L8, 70),
            )
        };
        let content = b"q 595 0 0 842 0 0 cm /Im0 Do Q\n".to_vec();
        b.page(
            (595.0, 842.0),
            content,
            dictionary! { "XObject" => dictionary! { "Im0" => img } },
            vec![],
        );
    }
    b.finish()
}

/// Two comments of a page: a note with its pop-up and a rectangle.
fn page_comments(b: &mut Builder, page: usize) -> Vec<Object> {
    let popup = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Popup", "Rect" => vec![400.into(), 600.into(), 580.into(), 700.into()], "Open" => false
    });
    let note = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Text",
        "Rect" => vec![60.into(), 600.into(), 80.into(), 620.into()],
        "Contents" => Object::string_literal(format!("Note on page {}", page + 1)),
        "T" => Object::string_literal("Reviewer"),
        "M" => Object::string_literal("D:20261019120000Z"),
        "CreationDate" => Object::string_literal("D:20261019120000Z"),
        "Popup" => popup,
    });
    let square = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Square",
        "Rect" => vec![100.into(), 400.into(), 300.into(), 500.into()],
        "C" => vec![1.into(), 0.into(), 0.into()],
        "Contents" => Object::string_literal("A rectangle"),
        "T" => Object::string_literal("Reviewer"),
    });
    vec![note.into(), popup.into(), square.into()]
}

/// A long document: a few lines of text and two comments (a note with its
/// pop-up and a rectangle) on every page.
pub fn long_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let mut rng = Rng(5);
    for p in 0..pages {
        let mut c = String::from("BT /F1 11 Tf 13 TL 60 780 Td\n");
        for _ in 0..12 {
            c.push_str(&format!("({}) '\n", line_text(&mut rng, 70)));
        }
        c.push_str("ET\n");
        let annots = page_comments(&mut b, p);
        b.page(
            (595.0, 842.0),
            c.into_bytes(),
            dictionary! { "Font" => standard_fonts() },
            annots,
        );
    }
    b.finish()
}

/// A long document as [`long_doc`] with, in addition, a bookmark for every
/// page pointing to a named destination, and on every page a link to the
/// named destination of the next page (what the bookmarks and links panes
/// read).
pub fn long_doc_linked(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let mut rng = Rng(5);
    let mut page_ids = Vec::new();
    let mut links = Vec::new();
    for p in 0..pages {
        let mut c = String::from("BT /F1 11 Tf 13 TL 60 780 Td\n");
        for _ in 0..12 {
            c.push_str(&format!("({}) '\n", line_text(&mut rng, 70)));
        }
        c.push_str("ET\n");
        let mut annots = page_comments(&mut b, p);
        let link = b.doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Link",
            "Rect" => vec![60.into(), 560.into(), 200.into(), 575.into()],
            "Border" => vec![0.into(), 0.into(), 0.into()],
            "Dest" => Object::string_literal(format!("p{}", (p + 1) % pages + 1)),
        });
        annots.push(link.into());
        links.push(link);
        page_ids.push(b.page(
            (595.0, 842.0),
            c.into_bytes(),
            dictionary! { "Font" => standard_fonts() },
            annots,
        ));
    }
    // Named destinations (a flat name tree, the names in order) and bookmarks.
    let mut names = Vec::new();
    let mut sorted: Vec<(String, ObjectId)> = page_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (format!("p{}", i + 1), *id))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, id) in &sorted {
        names.push(Object::string_literal(name.clone()));
        names.push(Object::Array(vec![
            Object::Reference(*id),
            "XYZ".into(),
            Object::Null,
            Object::Null,
            Object::Null,
        ]));
    }
    let outlines = b.doc.new_object_id();
    let items: Vec<ObjectId> = (0..pages).map(|_| b.doc.new_object_id()).collect();
    for (i, id) in items.iter().enumerate() {
        let mut d = dictionary! {
            "Title" => Object::string_literal(format!("Page {}", i + 1)),
            "Parent" => outlines,
            "Dest" => Object::string_literal(format!("p{}", i + 1)),
        };
        if i > 0 {
            d.set("Prev", items[i - 1]);
        }
        if i + 1 < items.len() {
            d.set("Next", items[i + 1]);
        }
        b.doc.objects.insert(*id, Object::Dictionary(d));
    }
    if let (Some(first), Some(last)) = (items.first(), items.last()) {
        b.doc.objects.insert(
            outlines,
            Object::Dictionary(dictionary! {
                "Type" => "Outlines", "First" => *first, "Last" => *last, "Count" => pages as i64,
            }),
        );
        b.catalog_entry("Outlines", Object::Reference(outlines));
    }
    b.catalog_entry(
        "Names",
        Object::Dictionary(dictionary! { "Dests" => dictionary! { "Names" => names } }),
    );
    b.finish()
}

// ---- embedded fonts -------------------------------------------------------------------

/// The outline of glyph `g` as closed contours of points (font units, the
/// glyph box 50..`advance`-50 × 0..`top`): an outer loop of 8 curved
/// segments and, for most glyphs, an inner loop (a counter, drawn the other
/// way round), about as many points as the letters of a text font.
fn glyph_loops(g: u32, advance: f32, top: f32) -> Vec<Vec<(f32, f32)>> {
    let mut rng = Rng(0x5eed_0000 + g as u64 * 7919);
    let (cx, cy) = (advance / 2.0, top / 2.0);
    let (rx, ry) = ((advance - 100.0) / 2.0, top / 2.0);
    let mut outer = Vec::new();
    for k in 0..16 {
        let a = k as f32 * std::f32::consts::TAU / 16.0;
        let wobble = 0.82 + 0.18 * rng.f();
        outer.push((cx + rx * wobble * a.cos(), cy + ry * wobble * a.sin()));
    }
    let mut loops = vec![outer];
    if !g.is_multiple_of(3) {
        let mut inner = Vec::new();
        for k in (0..12).rev() {
            let a = k as f32 * std::f32::consts::TAU / 12.0;
            let wobble = 0.35 + 0.1 * rng.f();
            inner.push((cx + rx * wobble * a.cos(), cy + ry * wobble * a.sin()));
        }
        loops.push(inner);
    }
    loops
}

/// Advance width (font units, 1000 per em) and height of the glyph of the
/// character `c` of the synthetic fonts.
fn glyph_metrics(c: u8) -> (f32, f32) {
    if c == b' ' {
        return (280.0, 0.0);
    }
    let advance = 420.0 + ((c as u32 * 37) % 180) as f32;
    let top = if c.is_ascii_lowercase() { 520.0 } else { 700.0 };
    (advance, top)
}

/// The glyphs of the synthetic fonts: `.notdef`, then the printable ASCII
/// characters (space to tilde), 96 glyphs.
const GLYPHS: u32 = 96;

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// A TrueType font program (`glyf` outlines with quadratic curves, a Unicode
/// `cmap`) for the printable ASCII characters, made here so documents with
/// embedded TrueType fonts need no font file.
pub fn truetype_font() -> Vec<u8> {
    // glyf and loca.
    let mut glyf = Vec::new();
    let mut loca: Vec<u32> = Vec::new();
    let mut hmtx = Vec::new();
    let (mut max_points, mut max_contours) = (0u16, 0u16);
    for g in 0..GLYPHS {
        loca.push(glyf.len() as u32);
        let (advance, top) = if g == 0 {
            (500.0, 700.0)
        } else {
            glyph_metrics(31 + g as u8)
        };
        put16(&mut hmtx, advance as u16);
        put16(&mut hmtx, 50);
        if g != 0 && top == 0.0 {
            continue; // the space has no outline
        }
        let loops = if g == 0 {
            vec![vec![(50.0, 0.0), (50.0, 700.0), (450.0, 700.0), (450.0, 0.0)]]
        } else {
            glyph_loops(g, advance, top)
        };
        let points: Vec<(i16, i16, bool)> = loops
            .iter()
            .flat_map(|l| {
                // Every other point is the control point of a quadratic curve.
                l.iter()
                    .enumerate()
                    .map(|(i, p)| (p.0.round() as i16, p.1.round() as i16, g == 0 || i % 2 == 0))
            })
            .collect();
        let xs = points.iter().map(|p| p.0);
        let ys = points.iter().map(|p| p.1);
        let (x_min, x_max) = (xs.clone().min().unwrap_or(0), xs.max().unwrap_or(0));
        let (y_min, y_max) = (ys.clone().min().unwrap_or(0), ys.max().unwrap_or(0));
        glyf.extend_from_slice(&(loops.len() as i16).to_be_bytes());
        for v in [x_min, y_min, x_max, y_max] {
            glyf.extend_from_slice(&v.to_be_bytes());
        }
        let mut end = 0usize;
        for l in &loops {
            end += l.len();
            put16(&mut glyf, (end - 1) as u16);
        }
        put16(&mut glyf, 0); // no instructions
        // Flags: on-curve or not; coordinates as 16-bit deltas.
        for p in &points {
            glyf.push(u8::from(p.2));
        }
        let mut last = 0i16;
        for p in &points {
            glyf.extend_from_slice(&(p.0 - last).to_be_bytes());
            last = p.0;
        }
        let mut last = 0i16;
        for p in &points {
            glyf.extend_from_slice(&(p.1 - last).to_be_bytes());
            last = p.1;
        }
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
        max_points = max_points.max(points.len() as u16);
        max_contours = max_contours.max(loops.len() as u16);
    }
    loca.push(glyf.len() as u32);
    let loca_bytes: Vec<u8> = loca.iter().flat_map(|v| v.to_be_bytes()).collect();

    let mut head = Vec::new();
    put32(&mut head, 0x0001_0000);
    put32(&mut head, 0x0001_0000);
    put32(&mut head, 0); // checksum adjustment
    put32(&mut head, 0x5F0F_3CF5);
    put16(&mut head, 0x000B);
    put16(&mut head, 1000);
    head.extend_from_slice(&[0u8; 16]); // created, modified
    for v in [0i16, -10, 1000, 720] {
        head.extend_from_slice(&v.to_be_bytes());
    }
    put16(&mut head, 0); // mac style
    put16(&mut head, 8);
    head.extend_from_slice(&2i16.to_be_bytes());
    head.extend_from_slice(&1i16.to_be_bytes()); // long loca
    head.extend_from_slice(&0i16.to_be_bytes());

    let mut hhea = Vec::new();
    put32(&mut hhea, 0x0001_0000);
    for v in [800i16, -200, 0] {
        hhea.extend_from_slice(&v.to_be_bytes());
    }
    put16(&mut hhea, 1000);
    for v in [0i16, 0, 1000, 1, 0, 0, 0, 0, 0, 0, 0] {
        hhea.extend_from_slice(&v.to_be_bytes());
    }
    put16(&mut hhea, GLYPHS as u16);

    let mut maxp = Vec::new();
    put32(&mut maxp, 0x0001_0000);
    put16(&mut maxp, GLYPHS as u16);
    put16(&mut maxp, max_points);
    put16(&mut maxp, max_contours);
    for v in [0u16, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0] {
        put16(&mut maxp, v);
    }

    // cmap: Unicode (3, 1), format 4, one range for space to tilde.
    let mut sub = Vec::new();
    put16(&mut sub, 4);
    put16(&mut sub, 32); // length
    put16(&mut sub, 0);
    put16(&mut sub, 4); // segCountX2
    put16(&mut sub, 4);
    put16(&mut sub, 1);
    put16(&mut sub, 0);
    put16(&mut sub, 0x7E);
    put16(&mut sub, 0xFFFF);
    put16(&mut sub, 0);
    put16(&mut sub, 0x20);
    put16(&mut sub, 0xFFFF);
    put16(&mut sub, (1i32 - 0x20).rem_euclid(65536) as u16);
    put16(&mut sub, 1);
    put16(&mut sub, 0);
    put16(&mut sub, 0);
    let mut cmap = Vec::new();
    put16(&mut cmap, 0);
    put16(&mut cmap, 1);
    put16(&mut cmap, 3);
    put16(&mut cmap, 1);
    put32(&mut cmap, 12);
    cmap.extend_from_slice(&sub);

    let mut post = Vec::new();
    put32(&mut post, 0x0003_0000);
    put32(&mut post, 0);
    post.extend_from_slice(&(-100i16).to_be_bytes());
    post.extend_from_slice(&50i16.to_be_bytes());
    for _ in 0..5 {
        put32(&mut post, 0);
    }

    let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"cmap", cmap),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca_bytes),
        (b"maxp", maxp),
        (b"post", post),
    ];
    let mut out = Vec::new();
    put32(&mut out, 0x0001_0000);
    put16(&mut out, tables.len() as u16);
    put16(&mut out, 128);
    put16(&mut out, 3);
    put16(&mut out, (tables.len() as u16) * 16 - 128);
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, data) in &tables {
        let sum = data
            .chunks(4)
            .map(|c| {
                let mut w = [0u8; 4];
                w[..c.len()].copy_from_slice(c);
                u32::from_be_bytes(w)
            })
            .fold(0u32, u32::wrapping_add);
        out.extend_from_slice(*tag);
        put32(&mut out, sum);
        put32(&mut out, offset as u32);
        put32(&mut out, data.len() as u32);
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend_from_slice(&body);
    out
}

/// A Type 2 charstring number.
fn cs_num(out: &mut Vec<u8>, v: i32) {
    match v {
        -107..=107 => out.push((v + 139) as u8),
        108..=1131 => {
            let v = v - 108;
            out.push((v / 256 + 247) as u8);
            out.push((v % 256) as u8);
        }
        -1131..=-108 => {
            let v = -v - 108;
            out.push((v / 256 + 251) as u8);
            out.push((v % 256) as u8);
        }
        _ => {
            out.push(28);
            out.extend_from_slice(&(v as i16).to_be_bytes());
        }
    }
}

/// A CFF INDEX of `items` (4-byte offsets).
fn cff_index(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    put16(&mut out, items.len() as u16);
    if items.is_empty() {
        return out;
    }
    out.push(4);
    let mut off = 1u32;
    put32(&mut out, off);
    for it in items {
        off += it.len() as u32;
        put32(&mut out, off);
    }
    for it in items {
        out.extend_from_slice(it);
    }
    out
}

/// A DICT integer operand in its 5-byte form (so the sizes are known ahead).
fn dict_int(out: &mut Vec<u8>, v: i32) {
    out.push(29);
    out.extend_from_slice(&v.to_be_bytes());
}

/// A bare CFF font program (Type 2 charstrings with cubic curves) for the
/// printable ASCII characters, glyph names from the standard strings, made
/// here so documents with embedded CFF fonts need no font file.
pub fn cff_font() -> Vec<u8> {
    let mut charstrings = Vec::new();
    for g in 0..GLYPHS {
        let mut cs = Vec::new();
        let (advance, top) = if g == 0 {
            (500.0, 700.0)
        } else {
            glyph_metrics(31 + g as u8)
        };
        cs_num(&mut cs, advance as i32); // width (nominal width 0)
        if g != 0 && top == 0.0 {
            cs.push(14);
            charstrings.push(cs);
            continue;
        }
        let loops = if g == 0 {
            vec![vec![(50.0, 0.0), (50.0, 700.0), (450.0, 700.0), (450.0, 0.0)]]
        } else {
            glyph_loops(g, advance, top)
        };
        let mut cur = (0i32, 0i32);
        for l in &loops {
            let p0 = (l[0].0.round() as i32, l[0].1.round() as i32);
            cs_num(&mut cs, p0.0 - cur.0);
            cs_num(&mut cs, p0.1 - cur.1);
            cs.push(21); // rmoveto
            cur = p0;
            if g == 0 {
                for p in &l[1..] {
                    let p = (p.0.round() as i32, p.1.round() as i32);
                    cs_num(&mut cs, p.0 - cur.0);
                    cs_num(&mut cs, p.1 - cur.1);
                    cur = p;
                }
                cs.push(5); // rlineto
                continue;
            }
            // A cubic curve to every other point, through the point between.
            let n = l.len();
            let mut i = 0;
            while i < n {
                let mid = l[(i + 1) % n];
                let end = l[(i + 2) % n];
                let c1 = (
                    (cur.0 as f32 + 2.0 * (mid.0 - cur.0 as f32) / 3.0).round() as i32,
                    (cur.1 as f32 + 2.0 * (mid.1 - cur.1 as f32) / 3.0).round() as i32,
                );
                let e = (end.0.round() as i32, end.1.round() as i32);
                let c2 = (
                    (e.0 as f32 + 2.0 * (mid.0 - e.0 as f32) / 3.0).round() as i32,
                    (e.1 as f32 + 2.0 * (mid.1 - e.1 as f32) / 3.0).round() as i32,
                );
                for (a, b) in [(c1, cur), (c2, c1), (e, c2)] {
                    cs_num(&mut cs, a.0 - b.0);
                    cs_num(&mut cs, a.1 - b.1);
                }
                cs.push(8); // rrcurveto
                cur = e;
                i += 2;
            }
        }
        cs.push(14); // endchar
        charstrings.push(cs);
    }
    let name = cff_index(&[b"FolitectaBenchCFF".to_vec()]);
    let strings = cff_index(&[]);
    let gsubrs = cff_index(&[]);
    let charstrings = cff_index(&charstrings);
    // Charset format 0: the standard strings 1 (space) to 95 (asciitilde).
    let mut charset = vec![0u8];
    for sid in 1..GLYPHS as u16 {
        put16(&mut charset, sid);
    }
    // nominalWidthX 0.
    let private = vec![139u8, 21];
    // Top DICT: charset (15), CharStrings (17), Private (18), all with 5-byte
    // operands so its size does not depend on the offsets.
    let top_len = 6 + 6 + 11;
    let top_index_len = 2 + 1 + 4 * 2 + top_len;
    let header = [1u8, 0, 4, 4];
    let start = header.len() + name.len() + top_index_len + strings.len() + gsubrs.len();
    let charstrings_at = start as i32;
    let charset_at = charstrings_at + charstrings.len() as i32;
    let private_at = charset_at + charset.len() as i32;
    let mut top = Vec::new();
    dict_int(&mut top, charset_at);
    top.push(15);
    dict_int(&mut top, charstrings_at);
    top.push(17);
    dict_int(&mut top, private.len() as i32);
    dict_int(&mut top, private_at);
    top.push(18);
    debug_assert_eq!(top.len(), top_len);
    let top_index = cff_index(&[top]);
    debug_assert_eq!(top_index.len(), top_index_len);
    let mut out = header.to_vec();
    out.extend_from_slice(&name);
    out.extend_from_slice(&top_index);
    out.extend_from_slice(&strings);
    out.extend_from_slice(&gsubrs);
    out.extend_from_slice(&charstrings);
    out.extend_from_slice(&charset);
    out.extend_from_slice(&private);
    out
}

/// The widths of the synthetic fonts for the codes 32 to 126.
fn synthetic_widths() -> Vec<Object> {
    (32u8..=126)
        .map(|c| Object::Integer(glyph_metrics(c).0 as i64))
        .collect()
}

/// Font resources with the synthetic fonts embedded: `/F1` TrueType
/// (`FontFile2`), `/F2` CFF (`FontFile3`, Type1C), `/F3` a composite font
/// (Type0, Identity-H) over the TrueType program, `/F4` Helvetica.
pub fn embedded_fonts(b: &mut Builder) -> Dictionary {
    let mut tt = Stream::new(Dictionary::new(), truetype_font());
    let _ = tt.compress();
    let tt_len = truetype_font().len() as i64;
    tt.dict.set("Length1", tt_len);
    let tt_id = b.doc.add_object(tt);
    let mut cff = Stream::new(dictionary! { "Subtype" => "Type1C" }, cff_font());
    let _ = cff.compress();
    let cff_id = b.doc.add_object(cff);
    let descriptor = |name: &str, key: &str, file: ObjectId| {
        let mut d = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => name,
            "Flags" => 32,
            "FontBBox" => vec![0.into(), (-10).into(), 1000.into(), 720.into()],
            "ItalicAngle" => 0,
            "Ascent" => 800,
            "Descent" => -200,
            "CapHeight" => 700,
            "StemV" => 80,
        };
        d.set(key, file);
        d
    };
    let tt_desc = b
        .doc
        .add_object(descriptor("FolitectaBenchSans", "FontFile2", tt_id));
    let cff_desc = b
        .doc
        .add_object(descriptor("FolitectaBenchCFF", "FontFile3", cff_id));
    let cid_desc = b
        .doc
        .add_object(descriptor("FolitectaBenchCID", "FontFile2", tt_id));
    let f1 = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "FolitectaBenchSans",
        "FirstChar" => 32, "LastChar" => 126, "Widths" => synthetic_widths(),
        "Encoding" => "WinAnsiEncoding", "FontDescriptor" => tt_desc,
    });
    let f2 = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "FolitectaBenchCFF",
        "FirstChar" => 32, "LastChar" => 126, "Widths" => synthetic_widths(),
        "Encoding" => "WinAnsiEncoding", "FontDescriptor" => cff_desc,
    });
    // CID widths: glyph g is the character 31 + g.
    let w: Vec<Object> = (1..GLYPHS)
        .map(|g| Object::Integer(glyph_metrics(31 + g as u8).0 as i64))
        .collect();
    let descendant = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "FolitectaBenchCID",
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Folitecta"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => cid_desc, "DW" => 500, "W" => vec![1.into(), Object::Array(w)],
        "CIDToGIDMap" => "Identity",
    });
    let f3 = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "FolitectaBenchCID",
        "Encoding" => "Identity-H", "DescendantFonts" => vec![descendant.into()],
    });
    let helv = dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" };
    dictionary! { "F1" => f1, "F2" => f2, "F3" => f3, "F4" => helv }
}

/// The glyph ids of `text` in the composite font `/F3`, as a hex string.
fn cid_hex(text: &str) -> String {
    text.bytes()
        .filter(|c| (32..=126).contains(c))
        .map(|c| format!("{:04X}", c as u16 - 31))
        .collect()
}

/// A4 pages of dense text set in embedded fonts (TrueType, CFF and a
/// composite font), saved with compressed object streams and a
/// cross-reference stream.
pub fn font_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let fonts = embedded_fonts(&mut b);
    let fonts_id = b.doc.add_object(fonts);
    let mut rng = Rng(13);
    for p in 0..pages {
        let mut c = format!(
            "BT /F4 15 Tf 50 800 Td (Chapter {} - {}) Tj ET\n",
            p + 1,
            line_text(&mut rng, 30)
        );
        for col in 0..2 {
            let x = 50 + col * 255;
            c.push_str(&format!("BT /F{} 8.5 Tf 10.4 TL {x} 776 Td\n", col + 1));
            for _ in 0..70 {
                c.push_str(&format!("({}) '\n", line_text(&mut rng, 52)));
            }
            c.push_str("ET\n");
        }
        // A band of text in the composite font at the bottom.
        c.push_str("BT /F3 9 Tf 50 30 Td\n");
        c.push_str(&format!("<{}> Tj\n", cid_hex(&line_text(&mut rng, 90))));
        c.push_str("ET\n");
        b.page(
            (595.0, 842.0),
            c.into_bytes(),
            dictionary! { "Font" => fonts_id },
            vec![],
        );
    }
    b.finish_compact()
}

/// A form: text fields and check boxes with their appearances on every page
/// (`fields_per_page` of them), a few lines of text, saved with object
/// streams.
pub fn form_doc(pages: usize, fields_per_page: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let helv = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"
    });
    let mut fields: Vec<Object> = Vec::new();
    let mut rng = Rng(17);
    for p in 0..pages {
        // The page's id is known only once it is added: widgets point to it after.
        let mut widgets = Vec::new();
        let mut content = String::from("BT /F1 10 Tf 12 TL 50 800 Td\n");
        for i in 0..fields_per_page {
            let row = i / 2;
            let col = i % 2;
            let x = 50.0 + col as f32 * 260.0;
            let y = 760.0 - row as f32 * 30.0;
            if y < 40.0 {
                break;
            }
            let check = i % 5 == 4;
            let (w, h) = if check { (14.0, 14.0) } else { (200.0, 18.0) };
            let rect = vec![
                Object::Real(x + 60.0),
                Object::Real(y),
                Object::Real(x + 60.0 + w),
                Object::Real(y + h),
            ];
            content.push_str(&format!("({}) '\n", line_text(&mut rng, 8)));
            let name = format!("p{}f{}", p + 1, i + 1);
            let id = if check {
                let on = b.form(
                    [0.0, 0.0, w, h],
                    "0 g 2 2 10 10 re f\n",
                    Dictionary::new(),
                    Dictionary::new(),
                );
                let off = b.form(
                    [0.0, 0.0, w, h],
                    "0.5 G 0.5 0.5 13 13 re S\n",
                    Dictionary::new(),
                    Dictionary::new(),
                );
                let value = if i % 2 == 0 { "Yes" } else { "Off" };
                b.doc.add_object(dictionary! {
                    "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn",
                    "T" => Object::string_literal(name), "Rect" => rect, "F" => 4,
                    "V" => value, "AS" => value,
                    "AP" => dictionary! { "N" => dictionary! { "Yes" => on, "Off" => off } },
                    "MK" => dictionary! { "BC" => vec![0.into()] },
                })
            } else {
                let value = line_text(&mut rng, 20);
                let ap = b.form(
                    [0.0, 0.0, w, h],
                    &format!(
                        "/Tx BMC q 0.9 0.92 1 rg 0 0 {w} {h} re f 0.5 G 0.5 0.5 {} {} re S BT /Helv 10 Tf 0 g 2 5 Td ({value}) Tj ET Q EMC\n",
                        w - 1.0,
                        h - 1.0
                    ),
                    dictionary! { "Font" => dictionary! { "Helv" => helv } },
                    Dictionary::new(),
                );
                b.doc.add_object(dictionary! {
                    "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx",
                    "T" => Object::string_literal(name), "Rect" => rect, "F" => 4,
                    "V" => Object::string_literal(value), "DA" => Object::string_literal("/Helv 10 Tf 0 g"),
                    "AP" => dictionary! { "N" => ap },
                })
            };
            widgets.push(Object::Reference(id));
            fields.push(Object::Reference(id));
        }
        content.push_str("ET\n");
        let page = b.page(
            (595.0, 842.0),
            content.into_bytes(),
            dictionary! { "Font" => dictionary! { "F1" => helv } },
            widgets.clone(),
        );
        for w in widgets {
            if let Ok(id) = w.as_reference() {
                if let Ok(d) = b.doc.get_dictionary_mut(id) {
                    d.set("P", page);
                }
            }
        }
    }
    b.catalog_entry(
        "AcroForm",
        Object::Dictionary(dictionary! {
            "Fields" => fields,
            "DA" => Object::string_literal("/Helv 10 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => helv } },
        }),
    );
    b.finish_compact()
}

/// Pages with transparency: an axial shading behind the text, a soft mask
/// (luminosity) over a group of shapes, an isolated transparency group
/// drawn with a blend mode and radial shadings inside, and dense text.
pub fn transparency_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let mut rng = Rng(19);
    let axial = |c0: [f32; 3], c1: [f32; 3], coords: [f32; 4]| {
        Object::Dictionary(dictionary! {
            "ShadingType" => 2, "ColorSpace" => "DeviceRGB",
            "Coords" => coords.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
            "Function" => dictionary! {
                "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()],
                "C0" => c0.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
                "C1" => c1.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
                "N" => 1,
            },
            "Extend" => vec![true.into(), true.into()],
        })
    };
    let radial = |c0: [f32; 3], c1: [f32; 3], coords: [f32; 6]| {
        Object::Dictionary(dictionary! {
            "ShadingType" => 3, "ColorSpace" => "DeviceRGB",
            "Coords" => coords.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
            "Function" => dictionary! {
                "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()],
                "C0" => c0.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
                "C1" => c1.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
                "N" => 1,
            },
            "Extend" => vec![true.into(), true.into()],
        })
    };
    for p in 0..pages {
        // The mask: white at the top fading to black at the bottom.
        let mask_group = b.form(
            [0.0, 0.0, 595.0, 842.0],
            "/Sh0 sh\n",
            dictionary! { "Shading" => dictionary! { "Sh0" => axial([1.0; 3], [0.0; 3], [0.0, 800.0, 0.0, 300.0]) } },
            dictionary! { "Group" => dictionary! { "S" => "Transparency", "CS" => "DeviceRGB" } },
        );
        // An isolated group: radial shadings and shapes, blended as a whole.
        let mut inner = String::new();
        for k in 0..6 {
            let (cx, cy) = (120.0 + 70.0 * k as f32, 520.0 + 40.0 * (k % 2) as f32);
            inner.push_str(&format!(
                "q {} 0 0 {} {cx} {cy} cm 0 0 m 1 0 l 1 1 l 0 1 l h W n /R{k} sh Q\n",
                120.0, 120.0
            ));
        }
        for _ in 0..120 {
            let (x, y, r) = (rng.f() * 595.0, 300.0 + rng.f() * 300.0, 5.0 + rng.f() * 25.0);
            inner.push_str(&format!(
                "{:.3} {:.3} {:.3} rg {x:.1} {y:.1} {r:.1} {r:.1} re f\n",
                rng.f(),
                rng.f(),
                rng.f()
            ));
        }
        let mut shadings = Dictionary::new();
        for k in 0..6 {
            shadings.set(
                format!("R{k}"),
                radial(
                    [rng.f(), rng.f(), rng.f()],
                    [rng.f(), rng.f(), rng.f()],
                    [0.5, 0.5, 0.0, 0.5, 0.5, 0.5],
                ),
            );
        }
        let group = b.form(
            [0.0, 0.0, 595.0, 842.0],
            &inner,
            dictionary! { "Shading" => shadings },
            dictionary! { "Group" => dictionary! { "S" => "Transparency", "I" => true, "K" => false } },
        );
        let mut c = String::from("q /Sh0 sh Q\n");
        c.push_str("q /GSm gs /Fm0 Do Q\n");
        c.push_str("q /GSb gs /Fm0 Do Q\n");
        c.push_str(&text_columns(&mut rng, p, 280, 24));
        b.page(
            (595.0, 842.0),
            c.into_bytes(),
            dictionary! {
                "Font" => standard_fonts(),
                "Shading" => dictionary! { "Sh0" => axial([0.95, 0.97, 1.0], [0.85, 0.9, 0.8], [0.0, 0.0, 595.0, 842.0]) },
                "XObject" => dictionary! { "Fm0" => group },
                "ExtGState" => dictionary! {
                    "GSm" => dictionary! { "Type" => "ExtGState", "SMask" => dictionary! { "Type" => "Mask", "S" => "Luminosity", "G" => mask_group } },
                    "GSb" => dictionary! { "Type" => "ExtGState", "ca" => 0.6, "BM" => "Multiply" },
                },
            },
            vec![],
        );
    }
    b.finish_compact()
}

/// A typical document of `pages` pages: dense text set in embedded fonts
/// with two comments on every page and a vector figure on every fifth page,
/// saved with object streams.
pub fn report_doc(pages: usize) -> Vec<u8> {
    let mut b = Builder::new();
    let fonts = embedded_fonts(&mut b);
    let fonts_id = b.doc.add_object(fonts);
    let mut rng = Rng(23);
    for p in 0..pages {
        let mut c = String::new();
        if p % 5 == 4 {
            // A figure in the upper half, text below.
            c.push_str("q 1 0 0 1 0 420 cm\n");
            c.push_str(&vector_content(&mut rng, 595.0, 400.0, 200, 400));
            c.push_str("Q\n");
            c.push_str("BT /F1 9 Tf 11 TL 50 400 Td\n");
            for _ in 0..30 {
                c.push_str(&format!("({}) '\n", line_text(&mut rng, 100)));
            }
            c.push_str("ET\n");
        } else {
            c.push_str(&format!(
                "BT /F4 15 Tf 50 800 Td (Section {} - {}) Tj ET\n",
                p + 1,
                line_text(&mut rng, 30)
            ));
            c.push_str("BT /F1 10 Tf 12.5 TL 50 776 Td\n");
            for _ in 0..58 {
                c.push_str(&format!("({}) '\n", line_text(&mut rng, 96)));
            }
            c.push_str("ET\n");
        }
        let annots = page_comments(&mut b, p);
        b.page(
            (595.0, 842.0),
            c.into_bytes(),
            dictionary! { "Font" => fonts_id, "ExtGState" => vector_states() },
            annots,
        );
    }
    b.finish_compact()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Renderer};

    /// The share of dark pixels in the picture of page 1.
    fn ink(bytes: Vec<u8>) -> f64 {
        let r = Renderer::new(std::sync::Arc::new(bytes)).unwrap();
        let px = r.render_rgba(0, 0.5, Default::default()).unwrap();
        if let Ok(dir) = std::env::var("TESTDOCS_DUMP") {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let _ = px.save(format!("{dir}/ink{n}.png"));
        }
        let dark = px.pixels().filter(|p| p.0[0] < 128).count();
        dark as f64 / (px.width() * px.height()) as f64
    }

    #[test]
    fn the_generated_documents_open_and_draw() {
        for (name, bytes, pages) in [
            ("fonts", font_doc(2), 2),
            ("form", form_doc(2, 20), 2),
            ("transparency", transparency_doc(2), 2),
            ("report", report_doc(5), 5),
            ("long", long_doc(3), 3),
            ("linked", long_doc_linked(3), 3),
        ] {
            let doc = Document::from_bytes(bytes.clone(), None, None).unwrap();
            assert_eq!(doc.page_count(), pages, "{name}");
            assert!(ink(bytes) > 0.001, "{name} draws nothing");
        }
        let linked = Document::from_bytes(long_doc_linked(4), None, None).unwrap();
        assert_eq!(linked.outline().len(), 4);
        assert_eq!(linked.all_links().len(), 4);
        let form = Document::from_bytes(form_doc(2, 20), None, None).unwrap();
        assert_eq!(form.form_fields().len(), 40);
        assert_eq!(form.page_widgets(1).len(), 20);
    }

    #[test]
    fn the_embedded_fonts_are_drawn_from_their_own_outlines() {
        // The synthetic glyphs are rounded blobs: the page differs from the
        // same text in a standard font (what a font that cannot be read
        // falls back to) in many pixels.
        let text = |font: &str| {
            let mut b = Builder::new();
            let fonts = embedded_fonts(&mut b);
            let mut c = String::new();
            for i in 0..40 {
                let shown = if font == "F3" {
                    format!("<{}>", cid_hex("the quick brown fox jumps over the lazy dog"))
                } else {
                    "(the quick brown fox jumps over the lazy dog)".to_string()
                };
                c.push_str(&format!(
                    "BT /{font} 14 Tf 20 {} Td {shown} Tj ET\n",
                    800 - i * 18
                ));
            }
            b.page(
                (595.0, 842.0),
                c.into_bytes(),
                dictionary! { "Font" => fonts },
                vec![],
            );
            let r = Renderer::new(std::sync::Arc::new(b.finish())).unwrap();
            r.render_rgba(0, 0.5, Default::default()).unwrap()
        };
        let standard = text("F4");
        for font in ["F1", "F2", "F3"] {
            let own = text(font);
            let differ = own
                .pixels()
                .zip(standard.pixels())
                .filter(|(a, b)| a.0[0].abs_diff(b.0[0]) > 64)
                .count() as f64
                / (own.width() * own.height()) as f64;
            assert!(differ > 0.03, "{font}: {differ}");
        }
    }
}
