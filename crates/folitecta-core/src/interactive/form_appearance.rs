//! Drawing form widgets: the appearance of every kind of field is generated
//! from the field and widget entries (`/FT`, `/Ff`, `/V`, `/DA`, `/Q`,
//! `/MK`, `/BS`, `/Opt`, the pictures of push buttons) so values show in
//! every viewer, including Folitecta's own renderer.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use super::form_design::*;
use crate::appearance::parse_da;
use crate::document::Document;
use crate::error::Result;
use crate::textdoc::wrap_text;
use crate::util::{deref, fmt_num, helvetica_width, num, pdf_literal, rect_of, text_of};

/// Everything needed to draw one widget, with inherited entries resolved.
pub(super) struct WidgetInfo {
    pub(super) ft: String,
    pub(super) ff: i64,
    pub(super) value: Vec<String>,
    pub(super) da: String,
    pub(super) q: i64,
    pub(super) rect: [f32; 4],
    pub(super) border: Option<[f32; 3]>,
    pub(super) fill: Option<[f32; 3]>,
    pub(super) width: f32,
    pub(super) caption: String,
    /// Push buttons: the pictures (form XObjects) of the normal, pressed
    /// and pointed-at states (`/MK` `/I`, `/IX`, `/RI`).
    pub(super) icons: [Option<ObjectId>; 3],
    pub(super) options: Vec<String>,
    pub(super) max_len: u32,
    pub(super) on_state: String,
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> String {
    let k = 0.5523;
    let f = fmt_num;
    let (ox, oy) = (rx * k, ry * k);
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h ",
        f(cx + rx),
        f(cy),
        f(cx + rx),
        f(cy + oy),
        f(cx + ox),
        f(cy + ry),
        f(cx),
        f(cy + ry),
        f(cx - ox),
        f(cy + ry),
        f(cx - rx),
        f(cy + oy),
        f(cx - rx),
        f(cy),
        f(cx - rx),
        f(cy - oy),
        f(cx - ox),
        f(cy - ry),
        f(cx),
        f(cy - ry),
        f(cx + ox),
        f(cy - ry),
        f(cx + rx),
        f(cy - oy),
        f(cx + rx),
        f(cy)
    )
}

impl Document {
    pub(super) fn widget_info(&self, id: ObjectId) -> Option<WidgetInfo> {
        let doc = &self.inner;
        let d = doc.get_dictionary(id).ok()?;
        // Inherited entries from the field hierarchy.
        let mut chain = vec![d];
        let mut cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        while let Some(p) = cur {
            let pd = doc.get_dictionary(p).ok()?;
            chain.push(pd);
            cur = pd.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
            if chain.len() > 32 {
                break;
            }
        }
        let find = |key: &[u8]| chain.iter().find_map(|c| c.get(key).ok());
        let ft = find(b"FT")
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_string())?;
        let ff = find(b"Ff").and_then(|o| o.as_i64().ok()).unwrap_or(0);
        let value: Vec<String> = match find(b"V").map(|v| deref(doc, v)) {
            Some(Object::Array(a)) => a.iter().filter_map(|o| text_of(doc, o)).collect(),
            Some(Object::Name(n)) => vec![String::from_utf8_lossy(n).to_string()],
            Some(other) => text_of(doc, other).into_iter().collect(),
            None => Vec::new(),
        };
        let acro_da = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .map(|f| deref(doc, f))
            .and_then(|f| f.as_dict().ok())
            .and_then(|f| f.get(b"DA").ok())
            .and_then(|o| text_of(doc, o));
        let da = find(b"DA")
            .and_then(|o| text_of(doc, o))
            .or(acro_da)
            .unwrap_or_else(|| "/Helv 0 Tf 0 g".into());
        let q = find(b"Q").and_then(|o| o.as_i64().ok()).unwrap_or(0);
        let rect = d.get(b"Rect").ok().and_then(|r| rect_of(doc, r))?;
        let rect = crate::geom::normalize(rect);
        let mk = d
            .get(b"MK")
            .ok()
            .map(|m| deref(doc, m))
            .and_then(|m| m.as_dict().ok());
        let border = mk.and_then(|m| color_of(doc, m.get(b"BC").ok()));
        let fill = mk.and_then(|m| color_of(doc, m.get(b"BG").ok()));
        let caption = mk
            .and_then(|m| m.get(b"CA").ok())
            .and_then(|o| text_of(doc, o))
            .unwrap_or_default();
        let icon = |k: &[u8]| mk.and_then(|m| m.get(k).ok()).and_then(|o| o.as_reference().ok());
        let icons = [icon(b"I"), icon(b"IX"), icon(b"RI")];
        let width = d
            .get(b"BS")
            .ok()
            .map(|b| deref(doc, b))
            .and_then(|b| b.as_dict().ok())
            .and_then(|b| b.get(b"W").ok())
            .and_then(|w| num(deref(doc, w)))
            .unwrap_or(if border.is_some() { 1.0 } else { 0.0 });
        let options: Vec<String> = find(b"Opt")
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_array().ok())
            .map(|a| {
                a.iter()
                    .filter_map(|o| match deref(doc, o) {
                        Object::Array(pair) => pair.last().and_then(|x| text_of(doc, x)),
                        other => text_of(doc, other),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let max_len = find(b"MaxLen").and_then(|o| o.as_i64().ok()).unwrap_or(0).max(0) as u32;
        // The "on" state: our private marker, else the non-Off name of the existing appearance.
        let on_state = name_of(d, b"_On")
            .or_else(|| {
                d.get(b"AP")
                    .ok()
                    .map(|a| deref(doc, a))
                    .and_then(|a| a.as_dict().ok())
                    .and_then(|a| a.get(b"N").ok())
                    .map(|n| deref(doc, n))
                    .and_then(|n| n.as_dict().ok())
                    .and_then(|n| {
                        n.iter()
                            .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                            .find(|k| k != "Off")
                    })
            })
            .unwrap_or_else(|| "Yes".into());
        Some(WidgetInfo {
            ft,
            ff,
            value,
            da,
            q,
            rect,
            border,
            fill,
            width,
            caption,
            icons,
            options,
            max_len,
            on_state,
        })
    }

    /// Regenerate the appearance of one widget from its field data.
    pub(crate) fn refresh_widget(&mut self, id: ObjectId) -> Result<()> {
        let Some(mut w) = self.widget_info(id) else {
            return Ok(());
        };
        // A format script shows the value its way (the value itself stays).
        if w.ft == "Tx" {
            if let Some(shown) = w.value.first().cloned() {
                if let Some(run) = self.run_field_script(id, b"F", &shown) {
                    if let (true, Some(v)) = (run.ok, run.event_value) {
                        w.value = vec![v];
                    }
                }
            }
        }
        let font = self.helvetica_font_id();
        let (bw, bh) = (w.rect[2] - w.rect[0], w.rect[3] - w.rect[1]);
        let f = fmt_num;
        let (da_size, text_color) = parse_da(&w.da);
        let explicit_size =
            w.da.split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|p| p[1] == "Tf" && p[0].parse::<f32>().map(|v| v > 0.0).unwrap_or(false));
        let frame = |pressed: bool| -> String {
            let mut s = String::new();
            if let Some(c) = w.fill {
                let c = if pressed {
                    [c[0] * 0.85, c[1] * 0.85, c[2] * 0.85]
                } else {
                    c
                };
                s.push_str(&rgb(c, "rg"));
                s.push_str(&format!("0 0 {} {} re f ", f(bw), f(bh)));
            }
            if w.width > 0.0 {
                if let Some(c) = w.border {
                    s.push_str(&format!("{} w ", f(w.width)));
                    s.push_str(&rgb(c, "RG"));
                    let h = w.width / 2.0;
                    s.push_str(&format!(
                        "{} {} {} {} re S ",
                        f(h),
                        f(h),
                        f(bw - w.width),
                        f(bh - w.width)
                    ));
                }
            }
            s
        };
        let pad = 2.0 + w.width;
        let text_block = |text: &str, multiline: bool| -> String {
            let size = if explicit_size {
                da_size
            } else if multiline {
                10.0
            } else {
                ((bh - 2.0 * pad) * 0.75).clamp(4.0, 14.0)
            };
            let mut s = format!(
                "/Tx BMC q {} {} {} {} re W n BT /Helv {} Tf ",
                f(w.width),
                f(w.width),
                f((bw - 2.0 * w.width).max(0.0)),
                f((bh - 2.0 * w.width).max(0.0)),
                f(size)
            );
            s.push_str(&rgb(text_color, "rg"));
            let inner = (bw - 2.0 * pad).max(size);
            let lines: Vec<String> = if multiline {
                wrap_text(text, size, inner)
            } else {
                vec![text.replace(['\n', '\r'], " ")]
            };
            let comb = w.ff & FF_COMB != 0 && w.max_len > 0 && !multiline;
            let mut y = if multiline {
                bh - pad - size * 0.88
            } else {
                (bh - size * 0.72) / 2.0
            };
            for line in lines {
                if comb {
                    let cell = bw / w.max_len as f32;
                    for (i, ch) in line.chars().take(w.max_len as usize).enumerate() {
                        let cs = ch.to_string();
                        let cw = helvetica_width(&cs, size);
                        s.push_str(&format!(
                            "1 0 0 1 {} {} Tm ",
                            f(cell * i as f32 + (cell - cw) / 2.0),
                            f(y)
                        ));
                        s.push_str(&String::from_utf8_lossy(&pdf_literal(&cs)));
                        s.push_str(" Tj ");
                    }
                } else {
                    let lw = helvetica_width(&line, size);
                    let x = match w.q {
                        1 => (bw - lw) / 2.0,
                        2 => bw - pad - lw,
                        _ => pad,
                    };
                    s.push_str(&format!("1 0 0 1 {} {} Tm ", f(x), f(y)));
                    s.push_str(&String::from_utf8_lossy(&pdf_literal(&line)));
                    s.push_str(" Tj ");
                }
                y -= size * 1.15;
            }
            s.push_str("ET Q EMC ");
            s
        };
        let make_stream = |content: String, font: ObjectId| -> Stream {
            Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), bw.into(), bh.into()],
                    "Resources" => dictionary! { "Font" => dictionary! { "Helv" => font } },
                },
                content.into_bytes(),
            )
        };
        let ap = match w.ft.as_str() {
            "Tx" => {
                let mut text = w.value.first().cloned().unwrap_or_default();
                if w.ff & FF_PASSWORD != 0 {
                    text = "*".repeat(text.chars().count());
                }
                let content = frame(false) + &text_block(&text, w.ff & FF_MULTILINE != 0);
                dictionary! { "N" => self.inner.add_object(make_stream(content, font)) }
            }
            "Ch" => {
                let content = if w.ff & FF_COMBO != 0 {
                    let text = w.value.first().cloned().unwrap_or_default();
                    let mut c = frame(false) + &text_block(&text, false);
                    // Drop-down marker.
                    let s = (bh * 0.3).clamp(3.0, 6.0);
                    let (cx, cy) = (bw - pad - s, bh / 2.0);
                    c.push_str("0.35 0.35 0.4 rg ");
                    c.push_str(&format!(
                        "{} {} m {} {} l {} {} l h f ",
                        f(cx - s),
                        f(cy + s * 0.5),
                        f(cx + s),
                        f(cy + s * 0.5),
                        f(cx),
                        f(cy - s * 0.6)
                    ));
                    c
                } else {
                    let size = if explicit_size { da_size } else { 10.0 };
                    let mut c = frame(false);
                    c.push_str(&format!(
                        "q {} {} {} {} re W n ",
                        f(w.width),
                        f(w.width),
                        f((bw - 2.0 * w.width).max(0.0)),
                        f((bh - 2.0 * w.width).max(0.0))
                    ));
                    let mut y = bh - pad - size;
                    for opt in &w.options {
                        if w.value.contains(opt) {
                            c.push_str("0.6 0.75 0.95 rg ");
                            c.push_str(&format!(
                                "{} {} {} {} re f ",
                                f(w.width),
                                f(y - size * 0.25),
                                f(bw - 2.0 * w.width),
                                f(size * 1.2)
                            ));
                        }
                        c.push_str(&format!("BT /Helv {} Tf ", f(size)));
                        c.push_str(&rgb(text_color, "rg"));
                        c.push_str(&format!("{} {} Td ", f(pad), f(y)));
                        c.push_str(&String::from_utf8_lossy(&pdf_literal(opt)));
                        c.push_str(" Tj ET ");
                        y -= size * 1.2;
                    }
                    c.push_str("Q ");
                    c
                };
                dictionary! { "N" => self.inner.add_object(make_stream(content, font)) }
            }
            "Btn" if w.ff & FF_PUSH != 0 => {
                let has_icon = w.icons.iter().any(Option::is_some);
                let size = if explicit_size {
                    da_size
                } else if has_icon {
                    (bh * 0.2).clamp(4.0, 10.0)
                } else {
                    (bh * 0.45).clamp(4.0, 14.0)
                };
                let cw = helvetica_width(&w.caption, size);
                // The picture fills the button (above the caption when there is one), kept in shape.
                let caption_h = if has_icon && !w.caption.is_empty() {
                    size * 1.5
                } else {
                    0.0
                };
                // Each picture with its size (from its bounding box).
                let sized = |icon: Option<ObjectId>| -> Option<(ObjectId, (f32, f32))> {
                    let id = icon?;
                    let s = self.inner.get_object(id).ok()?.as_stream().ok()?;
                    let b = s.dict.get(b"BBox").ok().and_then(|r| rect_of(&self.inner, r))?;
                    Some((id, ((b[2] - b[0]).abs().max(1.0), (b[3] - b[1]).abs().max(1.0))))
                };
                // The pressed and pointed-at states fall back on the normal picture.
                let up = sized(w.icons[0]);
                let down = sized(w.icons[1]).or(up);
                let over = sized(w.icons[2]);
                let content = |pressed: bool, icon: Option<(ObjectId, (f32, f32))>| {
                    let mut c = frame(pressed);
                    if let Some((_, (iw, ih))) = icon {
                        let (aw, ah) = ((bw - 2.0 * pad).max(1.0), (bh - 2.0 * pad - caption_h).max(1.0));
                        let k = (aw / iw).min(ah / ih);
                        let (dw, dh) = (iw * k, ih * k);
                        let shift = if pressed { 1.0 } else { 0.0 };
                        c.push_str(&format!(
                            "q {} 0 0 {} {} {} cm /FoliIcon Do Q ",
                            f(k),
                            f(k),
                            f((bw - dw) / 2.0 + shift),
                            f(pad + caption_h + (ah - dh) / 2.0 - shift)
                        ));
                    }
                    if !w.caption.is_empty() || !has_icon {
                        let y = if has_icon {
                            pad + (caption_h - size * 0.72) / 2.0
                        } else {
                            (bh - size * 0.72) / 2.0
                        };
                        c.push_str(&format!("BT /Helv {} Tf ", f(size)));
                        c.push_str(&rgb(text_color, "rg"));
                        c.push_str(&format!("{} {} Td ", f((bw - cw) / 2.0), f(y)));
                        c.push_str(&String::from_utf8_lossy(&pdf_literal(&w.caption)));
                        c.push_str(" Tj ET ");
                    }
                    c
                };
                let stream = |pressed: bool, icon: Option<(ObjectId, (f32, f32))>| -> Stream {
                    let mut s = make_stream(content(pressed, icon), font);
                    if let Some((i, _)) = icon {
                        if let Ok(Object::Dictionary(res)) = s.dict.get_mut(b"Resources") {
                            res.set("XObject", dictionary! { "FoliIcon" => i });
                        }
                    }
                    s
                };
                let (n, d) = (stream(false, up), stream(true, down));
                let r = over.map(|o| stream(false, Some(o)));
                let n_id = self.inner.add_object(n);
                let d_id = self.inner.add_object(d);
                let mut ap = dictionary! { "N" => n_id, "D" => d_id };
                if let Some(r) = r {
                    let r_id = self.inner.add_object(r);
                    ap.set("R", r_id);
                }
                ap
            }
            "Btn" => {
                let radio = w.ff & FF_RADIO != 0;
                let mut off = String::new();
                let mut on = String::new();
                let s = bw.min(bh);
                let (cx, cy) = (bw / 2.0, bh / 2.0);
                if radio {
                    for content in [&mut off, &mut on] {
                        if let Some(c) = w.fill {
                            content.push_str(&rgb(c, "rg"));
                            content.push_str(&(ellipse(cx, cy, s / 2.0 - 0.5, s / 2.0 - 0.5) + "f "));
                        }
                        if let Some(c) = w.border {
                            content.push_str(&format!("{} w ", f(w.width.max(0.5))));
                            content.push_str(&rgb(c, "RG"));
                            content.push_str(
                                &(ellipse(
                                    cx,
                                    cy,
                                    s / 2.0 - w.width.max(0.5) / 2.0 - 0.5,
                                    s / 2.0 - w.width.max(0.5) / 2.0 - 0.5,
                                ) + "S "),
                            );
                        }
                    }
                    on.push_str(&rgb(text_color, "rg"));
                    on.push_str(&(ellipse(cx, cy, s * 0.22, s * 0.22) + "f "));
                } else {
                    off = frame(false);
                    on = frame(false);
                    // Check mark scaled into the box.
                    let k = s * 0.8;
                    let (x0, y0) = (cx - k / 2.0, cy - k / 2.0);
                    on.push_str(&rgb(text_color, "RG"));
                    on.push_str(&format!("{} w 1 J 1 j ", f((k * 0.12).max(1.0))));
                    on.push_str(&format!(
                        "{} {} m {} {} l {} {} l S ",
                        f(x0 + k * 0.12),
                        f(y0 + k * 0.5),
                        f(x0 + k * 0.4),
                        f(y0 + k * 0.18),
                        f(x0 + k * 0.9),
                        f(y0 + k * 0.85)
                    ));
                }
                let on_id = self.inner.add_object(make_stream(on, font));
                let off_id = self.inner.add_object(make_stream(off, font));
                let mut states = Dictionary::new();
                states.set(w.on_state.as_bytes().to_vec(), Object::Reference(on_id));
                states.set("Off", Object::Reference(off_id));
                dictionary! { "N" => states }
            }
            "Sig" => {
                let mut content = frame(false);
                content.push_str("0.55 0.55 0.6 RG 0.75 w ");
                content.push_str(&format!(
                    "{} {} m {} {} l S ",
                    f(pad + 4.0),
                    f(bh * 0.28),
                    f(bw - pad - 4.0),
                    f(bh * 0.28)
                ));
                dictionary! { "N" => self.inner.add_object(make_stream(content, font)) }
            }
            _ => return Ok(()),
        };
        let d = self.inner.get_dictionary_mut(id)?;
        d.set("AP", ap);
        if w.ft == "Btn" && w.ff & FF_PUSH == 0 {
            let checked = w.value.first().map(|v| *v == w.on_state).unwrap_or(false);
            d.set(
                "AS",
                Object::Name(if checked {
                    w.on_state.as_bytes().to_vec()
                } else {
                    b"Off".to_vec()
                }),
            );
        }
        Ok(())
    }

    /// Regenerate the appearances of every widget of a field.
    pub(crate) fn refresh_field(&mut self, field: ObjectId) -> Result<()> {
        for w in self.field_widgets(field) {
            self.refresh_widget(w)?;
        }
        Ok(())
    }
}
