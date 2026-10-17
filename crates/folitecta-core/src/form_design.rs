//! Creating, editing and drawing interactive form fields.
//!
//! Field appearances are generated from the field and widget entries (`/FT`,
//! `/Ff`, `/V`, `/DA`, `/Q`, `/MK`, `/BS`, `/Opt`) so values show in every
//! viewer, including Folitecta's own renderer.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::appearance::parse_da;
use crate::document::Document;
use crate::error::{Error, Result};
use crate::textdoc::wrap_text;
use crate::util::{deref, fmt_num, helvetica_width, num, pdf_literal, rect_of, text_of};

/// Kinds of fields that can be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewField {
    Text,
    CheckBox,
    RadioButton,
    ComboBox,
    ListBox,
    PushButton,
    Signature,
}

/// Editable properties of a field and its widget.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldProps {
    pub name: String,
    pub tooltip: String,
    pub required: bool,
    pub read_only: bool,
    /// Text fields: several lines.
    pub multiline: bool,
    /// Text fields: hide the typed characters.
    pub password: bool,
    /// Text fields: maximum length (0 = unlimited).
    pub max_len: u32,
    /// Text fields: one character per cell (needs `max_len`).
    pub comb: bool,
    /// Choice fields: the options.
    pub options: Vec<String>,
    /// Combo boxes: allow typing a value that is not in the list.
    pub editable: bool,
    /// List boxes: allow several selected options.
    pub multi_select: bool,
    /// Check boxes and radio buttons: name of the "on" state.
    pub export_value: String,
    /// Push buttons: the caption.
    pub caption: String,
    /// Font size in points; 0 sizes the text to the field.
    pub font_size: f32,
    pub text_color: [f32; 3],
    pub border_color: Option<[f32; 3]>,
    pub fill_color: Option<[f32; 3]>,
    pub border_width: f32,
    /// 0 left, 1 centre, 2 right.
    pub align: u8,
    pub default_value: String,
}

impl Default for FieldProps {
    fn default() -> Self {
        FieldProps {
            name: String::new(),
            tooltip: String::new(),
            required: false,
            read_only: false,
            multiline: false,
            password: false,
            max_len: 0,
            comb: false,
            options: Vec::new(),
            editable: false,
            multi_select: false,
            export_value: "Yes".into(),
            caption: String::new(),
            font_size: 0.0,
            text_color: [0.0, 0.0, 0.0],
            border_color: Some([0.45, 0.45, 0.5]),
            fill_color: Some([1.0, 1.0, 1.0]),
            border_width: 1.0,
            align: 0,
            default_value: String::new(),
        }
    }
}

const FF_READ_ONLY: i64 = 1;
const FF_REQUIRED: i64 = 2;
const FF_MULTILINE: i64 = 1 << 12;
const FF_PASSWORD: i64 = 1 << 13;
const FF_NO_TOGGLE_OFF: i64 = 1 << 14;
const FF_RADIO: i64 = 1 << 15;
const FF_PUSH: i64 = 1 << 16;
const FF_COMBO: i64 = 1 << 17;
const FF_EDIT: i64 = 1 << 18;
const FF_MULTI_SELECT: i64 = 1 << 21;
const FF_COMB: i64 = 1 << 24;

fn name_of(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
}

fn color_of(doc: &lopdf::Document, obj: Option<&Object>) -> Option<[f32; 3]> {
    let v: Vec<f32> = obj
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|x| num(deref(doc, x))).collect())
        .unwrap_or_default();
    match v.len() {
        1 => Some([v[0], v[0], v[0]]),
        3 => Some([v[0], v[1], v[2]]),
        4 => {
            let k = 1.0 - v[3];
            Some([(1.0 - v[0]) * k, (1.0 - v[1]) * k, (1.0 - v[2]) * k])
        }
        _ => None,
    }
}

fn color_arr(c: [f32; 3]) -> Object {
    Object::Array(vec![c[0].into(), c[1].into(), c[2].into()])
}

fn rgb(c: [f32; 3], op: &str) -> String {
    format!("{} {} {} {op} ", fmt_num(c[0]), fmt_num(c[1]), fmt_num(c[2]))
}

/// A PDF name made of safe characters (for appearance state names).
fn state_name(s: &str) -> String {
    let n: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if n.is_empty() || n == "Off" {
        "Yes".into()
    } else {
        n
    }
}

/// Everything needed to draw one widget, with inherited entries resolved.
struct WidgetInfo {
    ft: String,
    ff: i64,
    value: Vec<String>,
    da: String,
    q: i64,
    rect: [f32; 4],
    border: Option<[f32; 3]>,
    fill: Option<[f32; 3]>,
    width: f32,
    caption: String,
    options: Vec<String>,
    max_len: u32,
    on_state: String,
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
    /// The AcroForm dictionary id, creating the form when the document has none.
    pub(crate) fn ensure_acroform(&mut self) -> Result<ObjectId> {
        let font = self.helvetica_font_id();
        let existing = self.inner.catalog()?.get(b"AcroForm").ok().cloned();
        let id = match existing {
            Some(Object::Reference(r)) => r,
            Some(Object::Dictionary(d)) => {
                let id = self.inner.add_object(d);
                self.inner.catalog_mut()?.set("AcroForm", Object::Reference(id));
                id
            }
            _ => {
                let id = self.inner.add_object(dictionary! { "Fields" => Vec::<Object>::new(), "DA" => Object::string_literal("/Helv 0 Tf 0 g") });
                self.inner.catalog_mut()?.set("AcroForm", Object::Reference(id));
                id
            }
        };
        // Make sure the default resources contain the Helvetica font used by appearances.
        let form = self.inner.get_dictionary_mut(id)?;
        if !form.has(b"Fields") {
            form.set("Fields", Vec::<Object>::new());
        }
        if !form.has(b"DA") {
            form.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
        }
        let mut dr = form
            .get(b"DR")
            .ok()
            .and_then(|d| d.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        let mut fonts = dr
            .get(b"Font")
            .ok()
            .and_then(|f| f.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        if !fonts.has(b"Helv") {
            fonts.set("Helv", Object::Reference(font));
        }
        dr.set("Font", fonts);
        form.set("DR", dr);
        form.remove(b"NeedAppearances");
        Ok(id)
    }

    fn add_to_fields(&mut self, form: ObjectId, field: ObjectId) -> Result<()> {
        let form = self.inner.get_dictionary_mut(form)?;
        let mut fields = form
            .get(b"Fields")
            .ok()
            .and_then(|f| f.as_array().ok())
            .cloned()
            .unwrap_or_default();
        fields.push(Object::Reference(field));
        form.set("Fields", fields);
        Ok(())
    }

    /// A field name not used yet ("Text1", "Text2", ...).
    pub fn unique_field_name(&self, base: &str) -> String {
        let names: Vec<String> = self.form_fields().into_iter().map(|f| f.name).collect();
        (1..)
            .map(|i| format!("{base}{i}"))
            .find(|n| !names.contains(n))
            .unwrap_or_else(|| base.to_string())
    }

    /// Create a form field with one widget on `page` (PDF rectangle). Radio
    /// buttons with the name of an existing radio group join that group.
    pub fn add_form_field(
        &mut self,
        page: usize,
        kind: NewField,
        rect: [f32; 4],
        props: &FieldProps,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let rect = [
            rect[0].min(rect[2]),
            rect[1].min(rect[3]),
            rect[0].max(rect[2]),
            rect[1].max(rect[3]),
        ];
        if rect[2] - rect[0] < 4.0 || rect[3] - rect[1] < 4.0 {
            return Err(Error::Invalid("the field is too small".into()));
        }
        let name = if props.name.trim().is_empty() {
            let base = match kind {
                NewField::Text => "Text",
                NewField::CheckBox => "Check",
                NewField::RadioButton => "Group",
                NewField::ComboBox => "Combo",
                NewField::ListBox => "List",
                NewField::PushButton => "Button",
                NewField::Signature => "Signature",
            };
            self.unique_field_name(base)
        } else {
            props.name.trim().to_string()
        };
        let existing_group = if kind == NewField::RadioButton {
            self.form_fields()
                .into_iter()
                .find(|f| f.name == name && f.kind == crate::forms::FieldKind::RadioButton)
                .map(|f| f.id)
        } else {
            None
        };
        if kind != NewField::RadioButton && self.form_fields().iter().any(|f| f.name == name) {
            return Err(Error::Invalid(format!("a field named \"{name}\" already exists")));
        }
        self.begin_edit();
        let form = self.ensure_acroform()?;
        let mut widget = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => vec![rect[0].into(), rect[1].into(), rect[2].into(), rect[3].into()],
            "P" => page_id,
            "F" => 4i64,
        };
        let f = fmt_num;
        let da = format!(
            "/Helv {} Tf {}",
            f(props.font_size.max(0.0)),
            rgb(props.text_color, "rg").trim()
        );
        let mut mk = Dictionary::new();
        if let Some(c) = props.border_color {
            mk.set("BC", color_arr(c));
        }
        if let Some(c) = props.fill_color {
            mk.set("BG", color_arr(c));
        }
        if kind == NewField::PushButton {
            mk.set(
                "CA",
                lopdf::text_string(if props.caption.is_empty() {
                    &name
                } else {
                    &props.caption
                }),
            );
        }
        widget.set("MK", mk);
        widget.set(
            "BS",
            dictionary! { "W" => props.border_width.max(0.0), "S" => "S" },
        );
        widget.set("DA", Object::string_literal(da));
        if !props.tooltip.is_empty() {
            widget.set("TU", lopdf::text_string(&props.tooltip));
        }
        let mut ff = 0i64;
        if props.read_only {
            ff |= FF_READ_ONLY;
        }
        if props.required {
            ff |= FF_REQUIRED;
        }
        let on = state_name(&props.export_value);
        let id = match kind {
            NewField::RadioButton => {
                widget.set("AS", Object::Name(b"Off".to_vec()));
                widget.set("_On", Object::Name(on.as_bytes().to_vec()));
                let parent = match existing_group {
                    Some(p) => p,
                    None => {
                        let p = self.inner.add_object(dictionary! {
                            "FT" => "Btn",
                            "Ff" => ff | FF_RADIO | FF_NO_TOGGLE_OFF,
                            "T" => lopdf::text_string(&name),
                            "V" => Object::Name(b"Off".to_vec()),
                            "Kids" => Vec::<Object>::new(),
                        });
                        self.add_to_fields(form, p)?;
                        p
                    }
                };
                widget.set("Parent", Object::Reference(parent));
                let wid = self.inner.add_object(widget);
                let pd = self.inner.get_dictionary_mut(parent)?;
                let mut kids = pd
                    .get(b"Kids")
                    .ok()
                    .and_then(|k| k.as_array().ok())
                    .cloned()
                    .unwrap_or_default();
                kids.push(Object::Reference(wid));
                pd.set("Kids", kids);
                wid
            }
            _ => {
                widget.set("T", lopdf::text_string(&name));
                match kind {
                    NewField::Text => {
                        widget.set("FT", "Tx");
                        if props.multiline {
                            ff |= FF_MULTILINE;
                        }
                        if props.password {
                            ff |= FF_PASSWORD;
                        }
                        if props.comb && props.max_len > 0 {
                            ff |= FF_COMB;
                        }
                        if props.max_len > 0 {
                            widget.set("MaxLen", props.max_len as i64);
                        }
                        widget.set("Q", props.align.min(2) as i64);
                        if !props.default_value.is_empty() {
                            widget.set("V", lopdf::text_string(&props.default_value));
                            widget.set("DV", lopdf::text_string(&props.default_value));
                        }
                    }
                    NewField::CheckBox => {
                        widget.set("FT", "Btn");
                        widget.set("V", Object::Name(b"Off".to_vec()));
                        widget.set("AS", Object::Name(b"Off".to_vec()));
                        widget.set("_On", Object::Name(on.as_bytes().to_vec()));
                    }
                    NewField::ComboBox | NewField::ListBox => {
                        widget.set("FT", "Ch");
                        if kind == NewField::ComboBox {
                            ff |= FF_COMBO;
                            if props.editable {
                                ff |= FF_EDIT;
                            }
                        } else if props.multi_select {
                            ff |= FF_MULTI_SELECT;
                        }
                        widget.set(
                            "Opt",
                            Object::Array(props.options.iter().map(|o| lopdf::text_string(o)).collect()),
                        );
                        if !props.default_value.is_empty() {
                            widget.set("V", lopdf::text_string(&props.default_value));
                            widget.set("DV", lopdf::text_string(&props.default_value));
                        }
                    }
                    NewField::PushButton => {
                        widget.set("FT", "Btn");
                        ff |= FF_PUSH;
                    }
                    NewField::Signature => {
                        widget.set("FT", "Sig");
                    }
                    NewField::RadioButton => unreachable!(),
                }
                let wid = self.inner.add_object(widget);
                self.add_to_fields(form, wid)?;
                wid
            }
        };
        if kind != NewField::RadioButton {
            self.inner.get_dictionary_mut(id)?.set("Ff", ff);
        }
        self.push_page_annot(page_id, id)?;
        self.refresh_widget(id)?;
        self.commit()?;
        Ok(id)
    }

    fn push_page_annot(&mut self, page_id: ObjectId, id: ObjectId) -> Result<()> {
        let page = self.inner.get_dictionary(page_id)?;
        let mut annots = match page.get(b"Annots").ok().cloned() {
            Some(Object::Array(a)) => a,
            Some(Object::Reference(r)) => self
                .inner
                .get_object(r)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        annots.push(Object::Reference(id));
        self.inner.get_dictionary_mut(page_id)?.set("Annots", annots);
        Ok(())
    }

    /// Widget annotation ids of a field (the field itself when it is merged with its widget).
    pub fn field_widgets(&self, field: ObjectId) -> Vec<ObjectId> {
        let Ok(d) = self.inner.get_dictionary(field) else {
            return Vec::new();
        };
        let kids: Vec<ObjectId> = d
            .get(b"Kids")
            .ok()
            .map(|k| deref(&self.inner, k))
            .and_then(|k| k.as_array().ok())
            .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
            .unwrap_or_default();
        let widget_kids: Vec<ObjectId> = kids
            .into_iter()
            .filter(|k| {
                self.inner
                    .get_dictionary(*k)
                    .map(|kd| !kd.has(b"T"))
                    .unwrap_or(false)
            })
            .collect();
        if widget_kids.is_empty() {
            vec![field]
        } else {
            widget_kids
        }
    }

    fn widget_info(&self, id: ObjectId) -> Option<WidgetInfo> {
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
        let rect = [
            rect[0].min(rect[2]),
            rect[1].min(rect[3]),
            rect[0].max(rect[2]),
            rect[1].max(rect[3]),
        ];
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
            options,
            max_len,
            on_state,
        })
    }

    /// Regenerate the appearance of one widget from its field data.
    pub(crate) fn refresh_widget(&mut self, id: ObjectId) -> Result<()> {
        let Some(w) = self.widget_info(id) else {
            return Ok(());
        };
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
                let size = if explicit_size {
                    da_size
                } else {
                    (bh * 0.45).clamp(4.0, 14.0)
                };
                let cw = helvetica_width(&w.caption, size);
                let content = |pressed: bool| {
                    let mut c = frame(pressed);
                    c.push_str(&format!("BT /Helv {} Tf ", f(size)));
                    c.push_str(&rgb(text_color, "rg"));
                    c.push_str(&format!(
                        "{} {} Td ",
                        f((bw - cw) / 2.0),
                        f((bh - size * 0.72) / 2.0)
                    ));
                    c.push_str(&String::from_utf8_lossy(&pdf_literal(&w.caption)));
                    c.push_str(" Tj ET ");
                    c
                };
                let n = content(false);
                let dn = content(true);
                let n_id = self.inner.add_object(make_stream(n, font));
                let d_id = self.inner.add_object(make_stream(dn, font));
                dictionary! { "N" => n_id, "D" => d_id }
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

    /// Properties of a field (from its first widget).
    pub fn field_props(&self, field: ObjectId) -> Result<FieldProps> {
        let widget = *self
            .field_widgets(field)
            .first()
            .ok_or_else(|| Error::Invalid("field not found".into()))?;
        let w = self
            .widget_info(widget)
            .ok_or_else(|| Error::Invalid("field not found".into()))?;
        let doc = &self.inner;
        let fd = doc.get_dictionary(field)?;
        let wd = doc.get_dictionary(widget)?;
        let (size, color) = parse_da(&w.da);
        let explicit =
            w.da.split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|p| p[1] == "Tf" && p[0].parse::<f32>().map(|v| v > 0.0).unwrap_or(false));
        Ok(FieldProps {
            name: crate::util::dict_text(doc, fd, b"T").unwrap_or_default(),
            tooltip: crate::util::dict_text(doc, fd, b"TU")
                .or_else(|| crate::util::dict_text(doc, wd, b"TU"))
                .unwrap_or_default(),
            required: w.ff & FF_REQUIRED != 0,
            read_only: w.ff & FF_READ_ONLY != 0,
            multiline: w.ff & FF_MULTILINE != 0,
            password: w.ff & FF_PASSWORD != 0,
            max_len: w.max_len,
            comb: w.ff & FF_COMB != 0,
            options: w.options.clone(),
            editable: w.ff & FF_EDIT != 0,
            multi_select: w.ff & FF_MULTI_SELECT != 0,
            export_value: w.on_state.clone(),
            caption: w.caption.clone(),
            font_size: if explicit { size } else { 0.0 },
            text_color: color,
            border_color: w.border,
            fill_color: w.fill,
            border_width: w.width,
            align: w.q.clamp(0, 2) as u8,
            default_value: fd
                .get(b"DV")
                .ok()
                .and_then(|o| text_of(doc, o))
                .unwrap_or_default(),
        })
    }

    /// Change the properties of a field and redraw its widgets.
    pub fn set_field_props(&mut self, field: ObjectId, props: &FieldProps) -> Result<()> {
        let name = props.name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the field name is empty".into()));
        }
        if self.form_fields().iter().any(|f| f.name == name && f.id != field) {
            return Err(Error::Invalid(format!("a field named \"{name}\" already exists")));
        }
        let widgets = self.field_widgets(field);
        self.begin_edit();
        let f = fmt_num;
        {
            let fd = self.inner.get_dictionary_mut(field)?;
            fd.set("T", lopdf::text_string(name));
            if props.tooltip.is_empty() {
                fd.remove(b"TU");
            } else {
                fd.set("TU", lopdf::text_string(&props.tooltip));
            }
            let mut ff = fd.get(b"Ff").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
            let mut setf = |bit: i64, on: bool| {
                if on { ff |= bit } else { ff &= !bit }
            };
            setf(FF_READ_ONLY, props.read_only);
            setf(FF_REQUIRED, props.required);
            let ft = fd
                .get(b"FT")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| n.to_vec())
                .unwrap_or_default();
            if ft == b"Tx" {
                setf(FF_MULTILINE, props.multiline);
                setf(FF_PASSWORD, props.password);
                setf(FF_COMB, props.comb && props.max_len > 0);
                if props.max_len > 0 {
                    fd.set("MaxLen", props.max_len as i64);
                } else {
                    fd.remove(b"MaxLen");
                }
            }
            if ft == b"Ch" {
                setf(FF_EDIT, props.editable);
                setf(FF_MULTI_SELECT, props.multi_select);
                fd.set(
                    "Opt",
                    Object::Array(props.options.iter().map(|o| lopdf::text_string(o)).collect()),
                );
            }
            fd.set("Ff", ff);
            if props.default_value.is_empty() {
                fd.remove(b"DV");
            } else {
                fd.set("DV", lopdf::text_string(&props.default_value));
            }
            fd.set("Q", props.align.min(2) as i64);
            fd.set(
                "DA",
                Object::string_literal(format!(
                    "/Helv {} Tf {}",
                    f(props.font_size.max(0.0)),
                    rgb(props.text_color, "rg").trim()
                )),
            );
        }
        let on = state_name(&props.export_value);
        for wid in widgets {
            let wd = self.inner.get_dictionary_mut(wid)?;
            let mut mk = wd
                .get(b"MK")
                .ok()
                .and_then(|m| m.as_dict().ok())
                .cloned()
                .unwrap_or_default();
            match props.border_color {
                Some(c) => mk.set("BC", color_arr(c)),
                None => {
                    mk.remove(b"BC");
                }
            }
            match props.fill_color {
                Some(c) => mk.set("BG", color_arr(c)),
                None => {
                    mk.remove(b"BG");
                }
            }
            if !props.caption.is_empty() {
                mk.set("CA", lopdf::text_string(&props.caption));
            }
            wd.set("MK", mk);
            wd.set(
                "BS",
                dictionary! { "W" => props.border_width.max(0.0), "S" => "S" },
            );
            if wid != field {
                wd.set(
                    "DA",
                    Object::string_literal(format!(
                        "/Helv {} Tf {}",
                        f(props.font_size.max(0.0)),
                        rgb(props.text_color, "rg").trim()
                    )),
                );
            }
            if wd.has(b"_On") || wid == field {
                // Only our own buttons carry the private state marker.
                if wd.has(b"_On") {
                    wd.set("_On", Object::Name(on.as_bytes().to_vec()));
                }
            }
        }
        self.refresh_field(field)?;
        self.commit()
    }

    /// Delete a form field with its widgets.
    pub fn delete_form_field(&mut self, field: ObjectId) -> Result<()> {
        let widgets = self.field_widgets(field);
        self.begin_edit();
        // Remove widgets from their pages.
        for pid in self.page_ids() {
            if let Ok(page) = self.inner.get_dictionary_mut(pid) {
                if let Ok(Object::Array(arr)) = page.get_mut(b"Annots") {
                    arr.retain(|o| {
                        o.as_reference()
                            .map(|r| !widgets.contains(&r) && r != field)
                            .unwrap_or(true)
                    });
                }
            }
        }
        // Remove the field from its parent or from the form.
        let parent = self
            .inner
            .get_dictionary(field)?
            .get(b"Parent")
            .ok()
            .and_then(|p| p.as_reference().ok());
        let container = match parent {
            Some(p) => Some(p),
            None => self
                .inner
                .catalog()?
                .get(b"AcroForm")
                .ok()
                .and_then(|f| f.as_reference().ok()),
        };
        if let Some(c) = container {
            let key: &[u8] = if parent.is_some() { b"Kids" } else { b"Fields" };
            if let Ok(d) = self.inner.get_dictionary_mut(c) {
                if let Ok(Object::Array(arr)) = d.get_mut(key) {
                    arr.retain(|o| o.as_reference().map(|r| r != field).unwrap_or(true));
                }
            }
        }
        for w in widgets {
            self.inner.delete_object(w);
        }
        self.inner.delete_object(field);
        self.commit()
    }

    /// Set the tab order of the fields of every page: `"R"` rows, `"C"` columns,
    /// `"S"` structure, `""` unspecified.
    pub fn set_tab_order(&mut self, order: &str) -> Result<()> {
        if !["R", "C", "S", ""].contains(&order) {
            return Err(Error::Invalid(format!("unknown tab order {order}")));
        }
        self.begin_edit();
        for pid in self.page_ids() {
            let page = self.inner.get_dictionary_mut(pid)?;
            if order.is_empty() {
                page.remove(b"Tabs");
            } else {
                page.set("Tabs", Object::Name(order.as_bytes().to_vec()));
            }
        }
        self.commit()
    }

    /// Copies of a field in `rows` x `cols` (the field is the first), each
    /// `dx` / `dy` points apart (PDF space, `dy` downwards when negative);
    /// the copies get numbered names. One undo step; returns the copies made.
    pub fn copy_field(
        &mut self,
        field: ObjectId,
        rows: usize,
        cols: usize,
        dx: f32,
        dy: f32,
    ) -> Result<usize> {
        let f = self
            .form_fields()
            .into_iter()
            .find(|x| x.id == field)
            .ok_or_else(|| Error::Invalid("field not found".into()))?;
        let kind = match f.kind {
            crate::forms::FieldKind::Text => NewField::Text,
            crate::forms::FieldKind::CheckBox => NewField::CheckBox,
            crate::forms::FieldKind::ComboBox => NewField::ComboBox,
            crate::forms::FieldKind::ListBox => NewField::ListBox,
            crate::forms::FieldKind::PushButton => NewField::PushButton,
            crate::forms::FieldKind::Signature => NewField::Signature,
            _ => return Err(Error::Invalid("radio groups cannot be copied this way".into())),
        };
        let (Some(page), Some(rect)) = (f.page, f.rect) else {
            return Err(Error::Invalid("the field has no widget on a page".into()));
        };
        let (rows, cols) = (rows.clamp(1, 100), cols.clamp(1, 100));
        if rows * cols == 1 {
            return Ok(0);
        }
        let base = self.field_props(field)?;
        let mark = self.undo_mark();
        let mut made = 0;
        for r in 0..rows {
            for c in 0..cols {
                if r == 0 && c == 0 {
                    continue;
                }
                let (ox, oy) = (c as f32 * dx, r as f32 * dy);
                let mut props = base.clone();
                props.name = self.unique_field_name(&format!("{}.", f.name));
                let nr = [rect[0] + ox, rect[1] + oy, rect[2] + ox, rect[3] + oy];
                self.add_form_field(page, kind, nr, &props)?;
                made += 1;
            }
        }
        self.squash_undo(&mark);
        Ok(made)
    }

    /// Tab order of a page: `R` rows, `C` columns, `S` structure, or empty
    /// (the order of the page's annotations).
    pub fn page_tab_order(&self, page: usize) -> String {
        self.page_id(page)
            .ok()
            .and_then(|id| self.inner.get_dictionary(id).ok())
            .and_then(|d| d.get(b"Tabs").ok())
            .and_then(|t| t.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_string())
            .unwrap_or_default()
    }

    /// Fields in the order their values are calculated (`/CO`).
    pub fn calculation_order(&self) -> Vec<ObjectId> {
        let doc = &self.inner;
        doc.catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .and_then(|a| deref(doc, a).as_dict().ok())
            .and_then(|a| a.get(b"CO").ok())
            .and_then(|co| deref(doc, co).as_array().ok())
            .map(|arr| arr.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default()
    }

    /// Widgets of a page in tab order when the order follows the page's
    /// annotation list (no row, column or structure order).
    pub fn widget_order(&self, page: usize) -> Vec<ObjectId> {
        let Ok(pid) = self.page_id(page) else {
            return Vec::new();
        };
        let doc = &self.inner;
        let Ok(pd) = doc.get_dictionary(pid) else {
            return Vec::new();
        };
        pd.get(b"Annots")
            .ok()
            .and_then(|a| deref(doc, a).as_array().ok())
            .map(|arr| {
                arr.iter()
                    .filter_map(|o| o.as_reference().ok())
                    .filter(|id| {
                        doc.get_dictionary(*id)
                            .ok()
                            .and_then(|d| d.get(b"Subtype").ok())
                            .and_then(|s| s.as_name().ok())
                            == Some(b"Widget".as_slice())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Move a widget earlier (`delta < 0`) or later in the tab order of its
    /// page; the page then follows its annotation order. Returns whether it moved.
    pub fn move_widget_in_tab_order(&mut self, widget: ObjectId, delta: i32) -> Result<bool> {
        let page = (0..self.page_count())
            .find(|p| self.widget_order(*p).contains(&widget))
            .ok_or_else(|| Error::Invalid("the field has no widget on a page".into()))?;
        let order = self.widget_order(page);
        let pos = order.iter().position(|w| *w == widget).expect("found above");
        let target = pos as i64 + delta as i64;
        if target < 0 || target >= order.len() as i64 || delta == 0 {
            return Ok(false);
        }
        let other = order[target as usize];
        let pid = self.page_id(page)?;
        self.begin_edit();
        let annots_obj = self.inner.get_dictionary(pid)?.get(b"Annots")?.clone();
        let mut arr = deref(&self.inner, &annots_obj).as_array()?.clone();
        let a = arr.iter().position(|o| o.as_reference().ok() == Some(widget));
        let b = arr.iter().position(|o| o.as_reference().ok() == Some(other));
        if let (Some(a), Some(b)) = (a, b) {
            arr.swap(a, b);
        }
        match annots_obj {
            Object::Reference(r) => {
                self.inner.objects.insert(r, Object::Array(arr));
            }
            _ => {
                self.inner
                    .get_dictionary_mut(pid)?
                    .set("Annots", Object::Array(arr));
            }
        }
        self.inner.get_dictionary_mut(pid)?.remove(b"Tabs");
        self.commit()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::state_name;

    #[test]
    fn state_names_are_safe() {
        assert_eq!(state_name("Sì!"), "S");
        assert_eq!(state_name(""), "Yes");
        assert_eq!(state_name("Off"), "Yes");
        assert_eq!(state_name("Choice 2"), "Choice2");
    }
}
