//! The normal appearance of every comment subtype Folitecta draws, generated
//! from the entries of its dictionary.

use lopdf::{Dictionary, Object, Stream, dictionary};

use super::measure::{measure_extra, measure_label};
use super::note_icons::note_icon;
use super::shapes::*;
use super::*;
use crate::document::Document;
use crate::error::Result;
use crate::fonts::{GlyphProblem, TextWriter, previous_font};
use crate::util::{deref, fmt_num, rect_of};

/// Text box contents: background, border and wrapped text inside `bx`.
fn free_text_body(
    style: &AnnotStyle,
    bx: [f32; 4],
    writer: &mut TextWriter,
) -> std::result::Result<String, GlyphProblem> {
    let f = fmt_num;
    let w = style.width.max(0.0);
    let mut body = String::new();
    if let Some(c) = style.fill {
        body.push_str(&rgb(c, "rg"));
        body.push_str(&format!(
            "{} {} {} {} re f ",
            f(bx[0]),
            f(bx[1]),
            f(bx[2] - bx[0]),
            f(bx[3] - bx[1])
        ));
    }
    if w > 0.0 {
        if let Some(c) = style.color {
            body.push_str(&format!("{} w ", f(w)));
            if style.dashed {
                body.push_str(&format!(
                    "[{} {}] 0 d ",
                    f((w * 3.0).max(3.0)),
                    f((w * 2.0).max(2.0))
                ));
            }
            body.push_str(&rgb(c, "RG"));
            let h = w / 2.0;
            body.push_str(&format!(
                "{} {} {} {} re S [] 0 d ",
                f(bx[0] + h),
                f(bx[1] + h),
                f(bx[2] - bx[0] - w),
                f(bx[3] - bx[1] - w)
            ));
        }
    }
    let fs = style.font_size.clamp(2.0, 400.0);
    let pad = 2.0 + w;
    let inner_w = (bx[2] - bx[0] - 2.0 * pad).max(fs);
    body.push_str(&format!(
        "q {} {} {} {} re W n ",
        f(bx[0] + w),
        f(bx[1] + w),
        f((bx[2] - bx[0] - 2.0 * w).max(0.0)),
        f((bx[3] - bx[1] - 2.0 * w).max(0.0))
    ));
    body.push_str(&format!("BT /{} {} Tf ", writer.resource_name(), f(fs)));
    body.push_str(&rgb(style.text_color, "rg"));
    let mut y = bx[3] - pad - fs * 0.88;
    for line in writer.wrap(fs, inner_w) {
        let lw = writer.width(&line, fs);
        let x = match style.align {
            1 => bx[0] + pad + (inner_w - lw) / 2.0,
            2 => bx[2] - pad - lw,
            _ => bx[0] + pad,
        };
        body.push_str(&format!("1 0 0 1 {} {} Tm ", f(x), f(y)));
        body.push_str(&writer.show(&line)?);
        body.push_str(" Tj ");
        y -= fs * 1.2;
    }
    body.push_str("ET Q ");
    Ok(body)
}

/// The text of `d` that its appearance cannot draw, if any: what
/// [`Document::generate_appearance`] would refuse, found without changing
/// anything (operations check it before their edit). `doc` resolves the
/// references of `d`.
pub(crate) fn text_problem(doc: &lopdf::Document, d: &Dictionary) -> Option<GlyphProblem> {
    let subtype = name_of(d, b"Subtype")?;
    match subtype.as_str() {
        "FreeText" => {
            crate::fonts::check_text(&crate::util::dict_text(doc, d, b"Contents").unwrap_or_default()).err()
        }
        "Line" | "PolyLine" | "Polygon" => {
            let key: &[u8] = if subtype == "Line" { b"L" } else { b"Vertices" };
            let pts = pairs(&numbers(doc, d.get(key).ok()));
            if pts.len() < 2 {
                return None;
            }
            let (label, _) = measure_label(doc, d, &pts)?;
            crate::fonts::check_text(&label).err()
        }
        "Stamp" => {
            let name = name_of(d, b"Name").unwrap_or_default();
            let drawing_kept = name == crate::signmarks::SIGNATURE_STAMP
                || name.starts_with("FolitectaDesign")
                || name == crate::tables::TABLE_STAMP
                || name == crate::calculator::CALC_STAMP
                || name == "FolitectaImage";
            if drawing_kept || !(name.starts_with("Folitecta") || !d.has(b"AP") || appearance_is_ours(doc, d))
            {
                return None;
            }
            let text = if name.starts_with("Folitecta") {
                crate::util::dict_text(doc, d, b"Contents").unwrap_or_default()
            } else {
                stamp_label(&name)
            };
            crate::fonts::check_text(&text).err()
        }
        _ => None,
    }
}

impl Document {
    /// Build the normal appearance of `d` from its entries and store it in `/AP`.
    /// Updates `/Rect` when the geometry needs room (line endings). Returns
    /// `false` (leaving `d` untouched) for unsupported subtypes.
    pub(crate) fn generate_appearance(&mut self, d: &mut Dictionary) -> Result<bool> {
        let subtype = name_of(d, b"Subtype").unwrap_or_default();
        if !GENERATED_SUBTYPES.contains(&subtype.as_str()) {
            return Ok(false);
        }
        let doc = &self.inner;
        let style = self.style_of(d);
        let mut rect = d
            .get(b"Rect")
            .ok()
            .and_then(|r| rect_of(doc, r))
            .unwrap_or([0.0; 4]);
        rect = crate::geom::normalize(rect);
        let w = style.width.max(0.0);
        let f = fmt_num;
        let mut body = String::new();
        let mut resources = Dictionary::new();
        let mut blend = "Normal";
        let stroke_setup = |body: &mut String| {
            body.push_str(&format!("{} w ", f(w)));
            if style.dashed {
                body.push_str(&format!(
                    "[{} {}] 0 d ",
                    f((w * 3.0).max(3.0)),
                    f((w * 2.0).max(2.0))
                ));
            }
            if let Some(c) = style.color {
                body.push_str(&rgb(c, "RG"));
            }
            if let Some(c) = style.fill {
                body.push_str(&rgb(c, "rg"));
            }
        };
        match subtype.as_str() {
            "Square" | "Circle" => {
                stroke_setup(&mut body);
                let paint = match (style.fill.is_some(), style.color.is_some() && w > 0.0) {
                    (true, true) => "B ",
                    (true, false) => "f ",
                    (false, true) => "S ",
                    (false, false) => "n ",
                };
                if let Some(intensity) = cloud_intensity(doc, d) {
                    // Bumps grow outwards: keep them inside the rectangle.
                    let inset = 5.0 + 3.0 * intensity + w;
                    let (x0, y0, x1, y1) =
                        (rect[0] + inset, rect[1] + inset, rect[2] - inset, rect[3] - inset);
                    let pts = if subtype == "Square" {
                        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                    } else {
                        ellipse_points(x0, y0, x1, y1, 24)
                    };
                    body.push_str("1 j ");
                    body.push_str(&cloud_path(&pts, intensity).0);
                } else {
                    let h = w / 2.0;
                    let (x0, y0, x1, y1) = (rect[0] + h, rect[1] + h, rect[2] - h, rect[3] - h);
                    if subtype == "Square" {
                        body.push_str(&format!(
                            "{} {} {} {} re ",
                            f(x0),
                            f(y0),
                            f((x1 - x0).max(0.0)),
                            f((y1 - y0).max(0.0))
                        ));
                    } else {
                        body.push_str(&ellipse(x0, y0, x1, y1));
                    }
                }
                body.push_str(paint);
            }
            "Line" | "PolyLine" | "Polygon" => {
                let key: &[u8] = if subtype == "Line" { b"L" } else { b"Vertices" };
                let pts = pairs(&numbers(doc, d.get(key).ok()));
                if pts.len() < 2 {
                    return Ok(false);
                }
                let closed = subtype == "Polygon";
                let cloud = if closed { cloud_intensity(doc, d) } else { None };
                let label = measure_label(doc, d, &pts);
                body.push_str("1 J 1 j ");
                stroke_setup(&mut body);
                let mut pad = ending_size(w) + w + 1.0;
                match cloud {
                    Some(intensity) => {
                        let (path, r) = cloud_path(&pts, intensity);
                        body.push_str(&path);
                        pad = pad.max(r * 1.05 + w + 1.0);
                    }
                    None => body.push_str(&poly(&pts, closed)),
                }
                body.push_str(if closed && style.fill.is_some() {
                    "B "
                } else {
                    "S "
                });
                // The arc of an angle, the circle of a radius.
                let extra = measure_extra(d, &pts);
                if let Some((path, _)) = &extra {
                    body.push_str(path);
                    body.push_str("S ");
                }
                if !closed {
                    body.push_str("[] 0 d ");
                    let n = pts.len();
                    let filled = style.fill.is_some();
                    body.push_str(&line_ending(
                        &style.line_start,
                        pts[0],
                        unit(pts[1], pts[0]),
                        w,
                        filled,
                    ));
                    body.push_str(&line_ending(
                        &style.line_end,
                        pts[n - 1],
                        unit(pts[n - 2], pts[n - 1]),
                        w,
                        filled,
                    ));
                }
                rect = bounds(&pts, pad);
                if let Some((_, b)) = extra {
                    rect = [
                        rect[0].min(b[0] - pad),
                        rect[1].min(b[1] - pad),
                        rect[2].max(b[2] + pad),
                        rect[3].max(b[3] + pad),
                    ];
                }
                if let Some((text, at)) = label {
                    let mut writer = TextWriter::new(&text)?;
                    let previous = previous_font(doc, d, &writer);
                    let fs = style.font_size.clamp(5.0, 48.0);
                    let shown = writer.text().to_string();
                    let tw = writer.width(&shown, fs);
                    let (bx, by) = (at.0 - tw / 2.0 - 3.0, at.1 + fs * 0.35);
                    let c = style.color.unwrap_or([0.0, 0.0, 0.0]);
                    body.push_str("1 1 1 rg ");
                    body.push_str(&format!(
                        "{} {} {} {} re f ",
                        f(bx),
                        f(by),
                        f(tw + 6.0),
                        f(fs + 4.0)
                    ));
                    body.push_str(&format!("BT /{} {} Tf ", writer.resource_name(), f(fs)));
                    body.push_str(&rgb(c, "rg"));
                    body.push_str(&format!("{} {} Td ", f(bx + 3.0), f(by + 2.0 + fs * 0.18)));
                    body.push_str(&writer.show(&shown)?);
                    body.push_str(" Tj ET ");
                    let name = writer.resource_name();
                    let font = writer.finish(self, previous)?;
                    resources.set("Font", dictionary! { name => font });
                    rect = [
                        rect[0].min(bx),
                        rect[1].min(by),
                        rect[2].max(bx + tw + 6.0),
                        rect[3].max(by + fs + 4.0),
                    ];
                    d.set("Contents", lopdf::text_string(&text));
                }
            }
            "Ink" => {
                let paths: Vec<Vec<(f32, f32)>> = d
                    .get(b"InkList")
                    .ok()
                    .map(|l| deref(doc, l))
                    .and_then(|l| l.as_array().ok())
                    .map(|paths| paths.iter().map(|p| pairs(&numbers(doc, Some(p)))).collect())
                    .unwrap_or_default();
                if paths.iter().all(|p| p.is_empty()) {
                    return Ok(false);
                }
                body.push_str("1 J 1 j ");
                stroke_setup(&mut body);
                for p in &paths {
                    match p.len() {
                        0 => continue,
                        1 => body.push_str(&poly(&[p[0], (p[0].0 + 0.01, p[0].1)], false)),
                        2 => body.push_str(&poly(p, false)),
                        // Freehand strokes are drawn as curves through their points.
                        _ => body.push_str(&curve_path(p)),
                    }
                    body.push_str("S ");
                }
                let all: Vec<(f32, f32)> = paths.into_iter().flatten().collect();
                rect = bounds(&all, w + 2.0);
            }
            "Highlight" | "Underline" | "StrikeOut" | "Squiggly" => {
                let quads: Vec<[f32; 8]> = numbers(doc, d.get(b"QuadPoints").ok())
                    .as_chunks::<8>()
                    .0
                    .to_vec();
                if quads.is_empty() {
                    return Ok(false);
                }
                let c = style.color.unwrap_or([1.0, 0.9, 0.2]);
                body.push_str(&rgb(c, "rg"));
                body.push_str(&rgb(c, "RG"));
                if subtype == "Highlight" {
                    blend = "Multiply";
                }
                let mut all = Vec::new();
                for q in &quads {
                    let (ul, ur, ll, lr) = ((q[0], q[1]), (q[2], q[3]), (q[4], q[5]), (q[6], q[7]));
                    all.extend([ul, ur, ll, lr]);
                    let (ux, uy) = (ul.0 - ll.0, ul.1 - ll.1);
                    let h = (ux * ux + uy * uy).sqrt().max(0.001);
                    let (nx, ny) = (ux / h, uy / h);
                    let t = (h * 0.07).max(0.8);
                    let along =
                        |a: (f32, f32), b: (f32, f32), k: f32| (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k);
                    match subtype.as_str() {
                        "Highlight" => body.push_str(&(poly(&[ul, ur, lr, ll], true) + "f ")),
                        "Underline" => body.push_str(
                            &(poly(
                                &[
                                    ll,
                                    lr,
                                    (lr.0 + nx * t, lr.1 + ny * t),
                                    (ll.0 + nx * t, ll.1 + ny * t),
                                ],
                                true,
                            ) + "f "),
                        ),
                        "StrikeOut" => {
                            let (m0, m1) = (along(ll, ul, 0.42), along(lr, ur, 0.42));
                            body.push_str(
                                &(poly(
                                    &[
                                        m0,
                                        m1,
                                        (m1.0 + nx * t, m1.1 + ny * t),
                                        (m0.0 + nx * t, m0.1 + ny * t),
                                    ],
                                    true,
                                ) + "f "),
                            );
                        }
                        _ => {
                            // Squiggly: a zigzag along the bottom edge.
                            let len = ((lr.0 - ll.0).powi(2) + (lr.1 - ll.1).powi(2)).sqrt();
                            let step = (h * 0.12).max(1.5);
                            let n = ((len / step).floor() as usize).max(2);
                            let wave: Vec<(f32, f32)> = (0..=n)
                                .map(|i| {
                                    let p = along(ll, lr, i as f32 / n as f32);
                                    let up = if i % 2 == 0 { 0.0 } else { t * 1.6 };
                                    (p.0 + nx * up, p.1 + ny * up)
                                })
                                .collect();
                            body.push_str(&format!("{} w ", f(t * 0.8)));
                            body.push_str(&(poly(&wave, false) + "S "));
                        }
                    }
                }
                // Keep the given rectangle, enlarged only to contain the marked text.
                let q = bounds(&all, 0.0);
                rect = [
                    rect[0].min(q[0]),
                    rect[1].min(q[1]),
                    rect[2].max(q[2]),
                    rect[3].max(q[3]),
                ];
            }
            "FreeText" => {
                let text = crate::util::dict_text(doc, d, b"Contents").unwrap_or_default();
                // The font is chosen (or the text refused) before anything is added.
                let mut writer = TextWriter::new(&text)?;
                let previous = previous_font(doc, d, &writer);
                let callout = name_of(d, b"IT").as_deref() == Some("FreeTextCallout");
                let cl = pairs(&numbers(doc, d.get(b"CL").ok()));
                let rd = numbers(doc, d.get(b"RD").ok());
                let bx = if rd.len() == 4 {
                    [rect[0] + rd[0], rect[1] + rd[1], rect[2] - rd[2], rect[3] - rd[3]]
                } else {
                    rect
                };
                if callout && cl.len() >= 2 {
                    // Leader line from the pointed spot (first point) to the box.
                    body.push_str("q 1 J 1 j ");
                    body.push_str(&format!("{} w ", f(w.max(0.5))));
                    let c = style.color.unwrap_or([0.0, 0.0, 0.0]);
                    body.push_str(&rgb(c, "RG"));
                    body.push_str(&rgb(c, "rg"));
                    body.push_str(&(poly(&cl, false) + "S "));
                    body.push_str(&line_ending(
                        &style.line_end,
                        cl[0],
                        unit(cl[1], cl[0]),
                        w.max(0.5),
                        true,
                    ));
                    body.push_str("Q ");
                }
                body.push_str(&free_text_body(&style, bx, &mut writer)?);
                if callout && cl.len() >= 2 {
                    let lead = bounds(&cl, ending_size(w) + w + 1.0);
                    rect = [
                        bx[0].min(lead[0]),
                        bx[1].min(lead[1]),
                        bx[2].max(lead[2]),
                        bx[3].max(lead[3]),
                    ];
                    d.set(
                        "RD",
                        vec![
                            (bx[0] - rect[0]).into(),
                            (bx[1] - rect[1]).into(),
                            (rect[2] - bx[2]).into(),
                            (rect[3] - bx[3]).into(),
                        ],
                    );
                }
                let fs = style.font_size.clamp(2.0, 400.0);
                let (r, g, b) = (style.text_color[0], style.text_color[1], style.text_color[2]);
                d.set(
                    "DA",
                    Object::string_literal(format!("/Helv {} Tf {} {} {} rg", f(fs), f(r), f(g), f(b))),
                );
                // The standard Helvetica, the same subset as before, or a new one.
                let name = writer.resource_name();
                let font = writer.finish(self, previous)?;
                resources.set("Font", dictionary! { name => font });
            }
            "Caret" => {
                let c = style.color.unwrap_or([0.1, 0.4, 0.95]);
                let (x0, y0, x1, y1) = (rect[0], rect[1], rect[2], rect[3]);
                let mid = (x0 + x1) / 2.0;
                body.push_str(&rgb(c, "rg"));
                body.push_str(
                    &(poly(
                        &[(x0, y0), (mid, y1), (x1, y0), (mid, y0 + (y1 - y0) * 0.35)],
                        true,
                    ) + "f "),
                );
            }
            "FileAttachment" => {
                // Paper-clip icon in the top-left corner of the rectangle.
                let c = style.color.unwrap_or([0.2, 0.4, 0.8]);
                let (x, y) = (rect[0], rect[3]);
                rect = [x, y - 20.0, x + 14.0, y];
                body.push_str(&rgb(c, "RG"));
                body.push_str("1.4 w 1 J 1 j ");
                body.push_str(&format!(
                    "{} {} m {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l S ",
                    f(x + 9.5),
                    f(y - 7.0),
                    f(x + 9.5),
                    f(y - 15.0),
                    f(x + 9.5),
                    f(y - 18.5),
                    f(x + 4.5),
                    f(y - 18.5),
                    f(x + 4.5),
                    f(y - 15.0),
                    f(x + 4.5),
                    f(y - 4.0),
                    f(x + 4.5),
                    f(y - 1.5),
                    f(x + 12.0),
                    f(y - 1.5),
                    f(x + 12.0),
                    f(y - 4.0),
                    f(x + 12.0),
                    f(y - 13.0)
                ));
            }
            "Text" => {
                // The icon of the note (its name) in the top-left corner of the rectangle.
                let c = style.color.unwrap_or([1.0, 0.85, 0.2]);
                let (x, y) = (rect[0], rect[3]);
                rect = [x, y - 20.0, x + 20.0, y];
                let icon = name_of(d, b"Name").unwrap_or_else(|| "Comment".into());
                body.push_str(&note_icon(&icon, x, y, c));
            }
            "Stamp" => {
                let name = name_of(d, b"Name").unwrap_or_default();
                if name == crate::signmarks::SIGNATURE_STAMP || name.starts_with("FolitectaDesign") {
                    // Handwritten signatures and designed stamps keep their drawing
                    // (it scales with the rectangle).
                    return Ok(false);
                }
                if name == crate::tables::TABLE_STAMP {
                    // Tables are drawn again from their data for the new rectangle.
                    let table: Option<crate::tables::Table> = match d.get(b"FolitectaTable") {
                        Ok(Object::String(s, _)) => serde_json::from_slice(s).ok(),
                        _ => None,
                    };
                    let Some(table) = table else { return Ok(false) };
                    let (content, writer) = table.drawing(rect)?;
                    let previous = previous_font(doc, d, &writer);
                    for (k, v) in self.drawing_resources(writer, previous)?.iter() {
                        resources.set(k.clone(), v.clone());
                    }
                    body.push_str(&content);
                } else if name == crate::calculator::CALC_STAMP {
                    // Calculator tapes are drawn again from their data.
                    let tape: Option<crate::calculator::Tape> = match d.get(b"FolitectaCalc") {
                        Ok(Object::String(s, _)) => serde_json::from_slice(s).ok(),
                        _ => None,
                    };
                    let Some(tape) = tape else { return Ok(false) };
                    let (content, writer) = tape.drawing(rect)?;
                    let previous = previous_font(doc, d, &writer);
                    for (k, v) in self.drawing_resources(writer, previous)?.iter() {
                        resources.set(k.clone(), v.clone());
                    }
                    body.push_str(&content);
                } else if name == "FolitectaImage" {
                    // Keep the picture of the existing appearance, change only the opacity.
                    let img = d
                        .get(b"AP")
                        .ok()
                        .map(|ap| deref(doc, ap))
                        .and_then(|ap| ap.as_dict().ok())
                        .and_then(|ap| ap.get(b"N").ok())
                        .map(|n| deref(doc, n))
                        .and_then(|n| n.as_stream().ok())
                        .and_then(|s| s.dict.get(b"Resources").ok())
                        .map(|r| deref(doc, r))
                        .and_then(|r| r.as_dict().ok())
                        .and_then(|r| r.get(b"XObject").ok())
                        .map(|x| deref(doc, x))
                        .and_then(|x| x.as_dict().ok())
                        .and_then(|x| x.get(b"Img").ok())
                        .cloned();
                    let Some(img) = img else { return Ok(false) };
                    resources.set("XObject", dictionary! { "Img" => img });
                    body.push_str(&format!(
                        "{} 0 0 {} {} {} cm /Img Do ",
                        f(rect[2] - rect[0]),
                        f(rect[3] - rect[1]),
                        f(rect[0]),
                        f(rect[1])
                    ));
                } else if name.starts_with("Folitecta") || !d.has(b"AP") || appearance_is_ours(doc, d) {
                    // Folitecta text stamps show their text; standard stamps without
                    // an appearance of their own show the stamp name.
                    let text = if name.starts_with("Folitecta") {
                        crate::util::dict_text(doc, d, b"Contents").unwrap_or_default()
                    } else {
                        stamp_label(&name)
                    };
                    // WinAnsi text with the standard Helvetica, as before; other
                    // text with the embedded Unicode font; text that cannot be
                    // drawn is refused, never drawn as `?`.
                    let mut writer = TextWriter::new(&text)?;
                    let previous = previous_font(doc, d, &writer);
                    let text = writer.text().to_string();
                    let c = style.color.unwrap_or([0.8, 0.1, 0.1]);
                    let (bw, bh) = (rect[2] - rect[0], rect[3] - rect[1]);
                    let unit_w = writer.width(&text, 1.0).max(1.0);
                    let fs = (bh * 0.55).min((bw - 16.0).max(4.0) / unit_w).max(4.0);
                    let tw = writer.width(&text, fs);
                    body.push_str(&rgb(c, "RG"));
                    body.push_str(&rgb(c, "rg"));
                    if let Some(fc) = style.fill {
                        body.push_str(&rgb(fc, "rg"));
                        body.push_str(&format!(
                            "{} {} {} {} re f ",
                            f(rect[0] + 1.0),
                            f(rect[1] + 1.0),
                            f(bw - 2.0),
                            f(bh - 2.0)
                        ));
                        body.push_str(&rgb(c, "rg"));
                    }
                    body.push_str(&format!(
                        "{} w {} {} {} {} re S ",
                        f(w.max(0.5) * 2.0),
                        f(rect[0] + 1.0),
                        f(rect[1] + 1.0),
                        f(bw - 2.0),
                        f(bh - 2.0)
                    ));
                    body.push_str(&format!(
                        "BT /{} {} Tf {} {} Td ",
                        writer.resource_name(),
                        f(fs),
                        f(rect[0] + (bw - tw) / 2.0),
                        f(rect[1] + (bh - fs * 0.72) / 2.0)
                    ));
                    body.push_str(&writer.show(&text)?);
                    body.push_str(" Tj ET ");
                    let name = writer.resource_name();
                    let font = writer.finish(self, previous)?;
                    resources.set("Font", dictionary! { name => font });
                } else {
                    return Ok(false);
                }
            }
            "Redact" => {
                let outline = style.color.unwrap_or([1.0, 0.0, 0.0]);
                body.push_str(&rgb(outline, "rg"));
                body.push_str("/GS1 gs ");
                body.push_str(&format!(
                    "{} {} {} {} re f ",
                    f(rect[0]),
                    f(rect[1]),
                    f(rect[2] - rect[0]),
                    f(rect[3] - rect[1])
                ));
                body.push_str("/GS0 gs ");
                body.push_str(&rgb(outline, "RG"));
                body.push_str(&format!(
                    "1.5 w {} {} {} {} re S ",
                    f(rect[0] + 0.75),
                    f(rect[1] + 0.75),
                    f(rect[2] - rect[0] - 1.5),
                    f(rect[3] - rect[1] - 1.5)
                ));
            }
            _ => return Ok(false),
        }
        let mut gs = dictionary! { "GS0" => dictionary! { "CA" => style.opacity, "ca" => style.opacity, "BM" => blend } };
        if subtype == "Redact" {
            gs.set(
                "GS1",
                dictionary! { "CA" => style.opacity, "ca" => 0.18 * style.opacity },
            );
        }
        resources.set("ExtGState", gs);
        let content = format!("q /GS0 gs {body}Q");
        let stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![rect[0].into(), rect[1].into(), rect[2].into(), rect[3].into()],
                "Resources" => resources,
                AP_MARKER => true,
            },
            content.into_bytes(),
        );
        let ap = self.inner.add_object(stream);
        d.set("AP", dictionary! { "N" => ap });
        d.set(
            "Rect",
            vec![rect[0].into(), rect[1].into(), rect[2].into(), rect[3].into()],
        );
        d.remove(b"AS");
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textdoc::wrap_text;
    use crate::util::{helvetica_width, pdf_literal};

    /// The text box drawing of Folitecta 0.2.0, kept to prove WinAnsi text is
    /// drawn exactly as before.
    fn free_text_body_0_2_0(style: &AnnotStyle, bx: [f32; 4], text: &str) -> String {
        let f = fmt_num;
        let w = style.width.max(0.0);
        let mut body = String::new();
        if let Some(c) = style.fill {
            body.push_str(&rgb(c, "rg"));
            body.push_str(&format!(
                "{} {} {} {} re f ",
                f(bx[0]),
                f(bx[1]),
                f(bx[2] - bx[0]),
                f(bx[3] - bx[1])
            ));
        }
        if w > 0.0 {
            if let Some(c) = style.color {
                body.push_str(&format!("{} w ", f(w)));
                if style.dashed {
                    body.push_str(&format!(
                        "[{} {}] 0 d ",
                        f((w * 3.0).max(3.0)),
                        f((w * 2.0).max(2.0))
                    ));
                }
                body.push_str(&rgb(c, "RG"));
                let h = w / 2.0;
                body.push_str(&format!(
                    "{} {} {} {} re S [] 0 d ",
                    f(bx[0] + h),
                    f(bx[1] + h),
                    f(bx[2] - bx[0] - w),
                    f(bx[3] - bx[1] - w)
                ));
            }
        }
        let fs = style.font_size.clamp(2.0, 400.0);
        let pad = 2.0 + w;
        let inner_w = (bx[2] - bx[0] - 2.0 * pad).max(fs);
        body.push_str(&format!(
            "q {} {} {} {} re W n ",
            f(bx[0] + w),
            f(bx[1] + w),
            f((bx[2] - bx[0] - 2.0 * w).max(0.0)),
            f((bx[3] - bx[1] - 2.0 * w).max(0.0))
        ));
        body.push_str(&format!("BT /Helv {} Tf ", f(fs)));
        body.push_str(&rgb(style.text_color, "rg"));
        let mut y = bx[3] - pad - fs * 0.88;
        for line in wrap_text(text, fs, inner_w) {
            let lw = helvetica_width(&line, fs);
            let x = match style.align {
                1 => bx[0] + pad + (inner_w - lw) / 2.0,
                2 => bx[2] - pad - lw,
                _ => bx[0] + pad,
            };
            body.push_str(&format!("1 0 0 1 {} {} Tm ", f(x), f(y)));
            body.push_str(&String::from_utf8_lossy(&pdf_literal(&line)));
            body.push_str(" Tj ");
            y -= fs * 1.2;
        }
        body.push_str("ET Q ");
        body
    }

    #[test]
    fn winansi_text_boxes_are_drawn_as_before() {
        let paragraph = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor \
                         incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud \
                         exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure.";
        let texts = [
            "",
            "Hello",
            "Città è perché € “ok” … ™",
            "tab\tand\nline\r\nend",
            paragraph,
            "(parentheses) and \\ backslash",
        ];
        let mut styles = Vec::new();
        for align in 0..=2 {
            for (width, dashed, fill) in [
                (1.0, false, Some([1.0, 1.0, 0.8])),
                (0.0, false, None),
                (2.0, true, None),
            ] {
                styles.push(AnnotStyle {
                    align,
                    width,
                    dashed,
                    fill,
                    font_size: if align == 1 { 9.5 } else { 12.0 },
                    text_color: [0.2, 0.1, 0.9],
                    ..AnnotStyle::default()
                });
            }
        }
        let bx = [50.0, 600.0, 245.5, 690.0];
        for text in texts {
            for style in &styles {
                let mut writer = TextWriter::new(text).unwrap();
                assert!(writer.is_standard());
                assert_eq!(
                    free_text_body(style, bx, &mut writer).unwrap(),
                    free_text_body_0_2_0(style, bx, text),
                    "{text:?} {style:?}"
                );
            }
        }
    }
}
