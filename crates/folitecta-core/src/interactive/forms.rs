//! Interactive form fields and signature fields.

use std::collections::HashMap;

use lopdf::{Dictionary, Object, ObjectId};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{self, deref, dict_text, rect_of, text_of};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    CheckBox,
    RadioButton,
    ComboBox,
    ListBox,
    PushButton,
    Signature,
    Unknown(String),
}

impl FieldKind {
    pub fn label(&self) -> &str {
        match self {
            FieldKind::Text => "Text",
            FieldKind::CheckBox => "Check box",
            FieldKind::RadioButton => "Radio button",
            FieldKind::ComboBox => "Combo box",
            FieldKind::ListBox => "List box",
            FieldKind::PushButton => "Push button",
            FieldKind::Signature => "Signature",
            FieldKind::Unknown(_) => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub id: ObjectId,
    pub name: String,
    pub kind: FieldKind,
    pub value: String,
    pub page: Option<usize>,
    pub rect: Option<[f32; 4]>,
    pub required: bool,
    pub read_only: bool,
    /// Export values / options for choice fields and buttons.
    pub options: Vec<String>,
    /// A text field drawn as a barcode (`/PMD`).
    pub barcode: bool,
    /// The widget annotations of the field.
    pub widgets: Vec<ObjectId>,
}

impl FormField {
    /// Last part of the name (`c` for `a.b.c`).
    pub fn short_name(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SignatureInfo {
    pub field: String,
    pub signed: bool,
    pub signer: String,
    pub date: String,
    pub reason: String,
    pub location: String,
    pub filter: String,
    pub page: Option<usize>,
}

struct Inherited {
    ft: Option<String>,
    ff: i64,
    v: Option<Object>,
    opt: Vec<String>,
}

impl Document {
    fn acroform(&self) -> Option<&Dictionary> {
        let catalog = self.inner.catalog().ok()?;
        deref(&self.inner, catalog.get(b"AcroForm").ok()?).as_dict().ok()
    }

    pub fn has_form(&self) -> bool {
        self.acroform().is_some()
    }

    /// Map from annotation object id to page index (for widgets).
    fn annot_page_map(&self) -> HashMap<ObjectId, usize> {
        let mut map = HashMap::new();
        for (idx, pid) in self.page_ids().iter().enumerate() {
            if let Ok(page) = self.inner.get_dictionary(*pid) {
                if let Ok(arr) = page
                    .get(b"Annots")
                    .map(|a| deref(&self.inner, a))
                    .and_then(|a| a.as_array())
                {
                    for a in arr {
                        if let Ok(r) = a.as_reference() {
                            map.insert(r, idx);
                        }
                    }
                }
            }
        }
        map
    }

    /// All terminal form fields.
    pub fn form_fields(&self) -> Vec<FormField> {
        self.kept("form_fields", || self.read_form_fields())
            .as_ref()
            .clone()
    }

    fn read_form_fields(&self) -> Vec<FormField> {
        let Some(form) = self.acroform() else {
            return Vec::new();
        };
        let Ok(fields) = form
            .get(b"Fields")
            .map(|f| deref(&self.inner, f))
            .and_then(|f| f.as_array())
        else {
            return Vec::new();
        };
        let pages = self.annot_page_map();
        let mut out = Vec::new();
        for f in fields {
            if let Ok(id) = f.as_reference() {
                self.walk_field(
                    id,
                    "",
                    &Inherited {
                        ft: None,
                        ff: 0,
                        v: None,
                        opt: Vec::new(),
                    },
                    &pages,
                    &mut out,
                    0,
                );
            }
        }
        out
    }

    fn walk_field(
        &self,
        id: ObjectId,
        prefix: &str,
        inherited: &Inherited,
        pages: &HashMap<ObjectId, usize>,
        out: &mut Vec<FormField>,
        depth: usize,
    ) {
        if depth > 32 {
            return;
        }
        let Ok(dict) = self.inner.get_dictionary(id) else {
            return;
        };
        let doc = &self.inner;
        let partial = dict_text(doc, dict, b"T");
        let name = match (&partial, prefix.is_empty()) {
            (Some(p), true) => p.clone(),
            (Some(p), false) => format!("{prefix}.{p}"),
            (None, _) => prefix.to_string(),
        };
        let ft = dict
            .get(b"FT")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_string())
            .or_else(|| inherited.ft.clone());
        let ff = dict
            .get(b"Ff")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(inherited.ff);
        let v = dict.get(b"V").ok().cloned().or_else(|| inherited.v.clone());
        let opt: Vec<String> = dict
            .get(b"Opt")
            .map(|o| deref(doc, o))
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|arr| {
                arr.iter()
                    .filter_map(|o| match deref(doc, o) {
                        Object::Array(pair) => pair.last().and_then(|x| text_of(doc, x)),
                        other => text_of(doc, other),
                    })
                    .collect()
            })
            .unwrap_or_else(|| inherited.opt.clone());
        let kids: Vec<ObjectId> = dict
            .get(b"Kids")
            .map(|k| deref(doc, k))
            .ok()
            .and_then(|k| k.as_array().ok())
            .map(|arr| arr.iter().filter_map(|k| k.as_reference().ok()).collect())
            .unwrap_or_default();
        let kid_is_field = |kid: ObjectId| {
            self.inner
                .get_dictionary(kid)
                .map(|d| d.has(b"T"))
                .unwrap_or(false)
        };
        let has_field_kids = kids.iter().any(|k| kid_is_field(*k));
        if has_field_kids {
            let inh = Inherited { ft, ff, v, opt };
            for kid in kids {
                self.walk_field(kid, &name, &inh, pages, out, depth + 1);
            }
            return;
        }
        // Terminal field (possibly with widget-only kids).
        let Some(ft) = ft else { return };
        let kind = match ft.as_str() {
            "Tx" => FieldKind::Text,
            "Btn" if ff & (1 << 16) != 0 => FieldKind::PushButton,
            "Btn" if ff & (1 << 15) != 0 => FieldKind::RadioButton,
            "Btn" => FieldKind::CheckBox,
            "Ch" if ff & (1 << 17) != 0 => FieldKind::ComboBox,
            "Ch" => FieldKind::ListBox,
            "Sig" => FieldKind::Signature,
            other => FieldKind::Unknown(other.to_string()),
        };
        let value = match v.as_ref().map(|v| deref(doc, v)) {
            Some(Object::Array(arr)) => arr
                .iter()
                .filter_map(|o| text_of(doc, o))
                .collect::<Vec<_>>()
                .join(", "),
            Some(Object::Dictionary(d)) if kind == FieldKind::Signature => {
                dict_text(doc, d, b"Name").unwrap_or_else(|| "(signed)".into())
            }
            Some(other) => text_of(doc, other).unwrap_or_default(),
            None => String::new(),
        };
        let widget_id = if kids.is_empty() { id } else { kids[0] };
        let page = pages.get(&widget_id).copied().or_else(|| pages.get(&id).copied());
        let rect = self
            .inner
            .get_dictionary(widget_id)
            .ok()
            .and_then(|d| d.get(b"Rect").ok())
            .and_then(|r| rect_of(doc, r));
        out.push(FormField {
            id,
            name,
            kind,
            value,
            page,
            rect,
            required: ff & 2 != 0,
            read_only: ff & 1 != 0,
            options: opt,
            barcode: dict.has(b"PMD"),
            widgets: if kids.is_empty() { vec![id] } else { kids },
        });
    }

    /// Set the value of a text, choice, check box or radio button field and
    /// redraw its widgets. For check boxes any value other than "Off" (or
    /// empty) checks the box; radio groups select the button whose state is `value`.
    pub fn set_field_value(&mut self, id: ObjectId, value: &str) -> Result<()> {
        let field = self
            .form_fields()
            .into_iter()
            .find(|f| f.id == id)
            .ok_or_else(|| Error::Invalid("field not found".into()))?;
        if field.read_only {
            return Err(Error::Invalid(format!(
                "the field \"{}\" is read-only",
                field.name
            )));
        }
        // A validation script may refuse the value, saying why.
        if field.kind == FieldKind::Text || field.kind == FieldKind::ComboBox {
            if let Some(run) = self.run_field_script(field.id, b"V", value) {
                if run.ok && !run.event_rc {
                    let why = run
                        .out
                        .iter()
                        .find(|(k, _)| k == "alert")
                        .map(|(_, t)| t.clone())
                        .unwrap_or_else(|| format!("the value of \"{}\" was not accepted", field.name));
                    return Err(Error::Invalid(why));
                }
            }
        }
        self.begin_edit();
        self.write_field_value(&field, value)?;
        self.recalculate()?;
        self.commit()
    }

    pub(crate) fn write_field_value(&mut self, field: &FormField, value: &str) -> Result<()> {
        let id = field.id;
        match field.kind {
            FieldKind::CheckBox | FieldKind::RadioButton => {
                let off = value.is_empty() || value == "Off";
                let widgets = self.field_widgets(id);
                let states: Vec<String> = widgets.iter().map(|w| self.widget_on_state(*w)).collect();
                let v = if off {
                    "Off".to_string()
                } else if field.kind == FieldKind::CheckBox {
                    states.first().cloned().unwrap_or_else(|| value.to_string())
                } else {
                    value.to_string()
                };
                self.inner
                    .get_dictionary_mut(id)?
                    .set("V", Object::Name(v.as_bytes().to_vec()));
                for (w, on) in widgets.into_iter().zip(states) {
                    let state = if !off && on == v {
                        v.clone()
                    } else {
                        "Off".to_string()
                    };
                    let has_ap = self.inner.get_dictionary(w)?.has(b"AP");
                    self.inner
                        .get_dictionary_mut(w)?
                        .set("AS", Object::Name(state.into_bytes()));
                    if !has_ap {
                        self.refresh_widget(w)?;
                    }
                }
            }
            FieldKind::ListBox if value.contains('\n') => {
                let items: Vec<Object> = value.lines().map(lopdf::text_string).collect();
                self.inner.get_dictionary_mut(id)?.set("V", Object::Array(items));
                self.refresh_field(id)?;
            }
            _ => {
                self.inner
                    .get_dictionary_mut(id)?
                    .set("V", lopdf::text_string(value));
                self.refresh_field(id)?;
            }
        }
        Ok(())
    }

    /// Set several field values at once by full name (one undo step); unknown
    /// names are skipped. Returns the number of fields changed.
    pub fn set_field_values(&mut self, values: &[(String, String)]) -> Result<usize> {
        let fields = self.form_fields();
        let mut todo = Vec::new();
        for (name, value) in values {
            if let Some(f) = fields.iter().find(|f| {
                &f.name == name
                    && !f.read_only
                    && f.kind != FieldKind::Signature
                    && f.kind != FieldKind::PushButton
            }) {
                todo.push((f.clone(), value.clone()));
            }
        }
        if todo.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        for (f, v) in &todo {
            self.write_field_value(f, v)?;
        }
        self.recalculate()?;
        self.commit()?;
        Ok(todo.len())
    }

    /// Name of the "on" appearance state of a check box or radio widget.
    pub(crate) fn widget_on_state(&self, widget: ObjectId) -> String {
        let doc = &self.inner;
        let Ok(d) = doc.get_dictionary(widget) else {
            return "Yes".into();
        };
        if let Ok(n) = d.get(b"_On").and_then(|n| n.as_name()) {
            return String::from_utf8_lossy(n).to_string();
        }
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
            .unwrap_or_else(|| "Yes".into())
    }

    /// Reset all fields to their default values.
    pub fn reset_form(&mut self) -> Result<usize> {
        let fields = self.form_fields();
        if fields.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let mut count = 0;
        for f in &fields {
            if f.kind == FieldKind::Signature || f.kind == FieldKind::PushButton {
                continue;
            }
            let dv = self
                .inner
                .get_dictionary(f.id)
                .ok()
                .and_then(|d| d.get(b"DV").ok())
                .map(|o| deref(&self.inner, o).clone());
            let value = match dv {
                Some(Object::Name(n)) => String::from_utf8_lossy(&n).to_string(),
                Some(other) => text_of(&self.inner, &other).unwrap_or_default(),
                None => String::new(),
            };
            match self.write_field_value(f, &value) {
                Ok(()) => count += 1,
                Err(_) => continue,
            }
            if self
                .inner
                .get_dictionary(f.id)
                .map(|d| !d.has(b"DV"))
                .unwrap_or(false)
                && !matches!(f.kind, FieldKind::CheckBox | FieldKind::RadioButton)
            {
                if let Ok(d) = self.inner.get_dictionary_mut(f.id) {
                    d.remove(b"V");
                }
                self.refresh_field(f.id)?;
            }
        }
        self.recalculate()?;
        self.commit()?;
        Ok(count)
    }

    /// Signature fields with their signing information.
    pub fn signatures(&self) -> Vec<SignatureInfo> {
        let doc = &self.inner;
        self.form_fields()
            .into_iter()
            .filter(|f| f.kind == FieldKind::Signature)
            .map(|f| {
                let v = doc
                    .get_dictionary(f.id)
                    .ok()
                    .and_then(|d| d.get(b"V").ok())
                    .map(|v| deref(doc, v))
                    .and_then(|v| v.as_dict().ok());
                match v {
                    Some(sig) => SignatureInfo {
                        field: f.name,
                        signed: sig.has(b"Contents"),
                        signer: dict_text(doc, sig, b"Name").unwrap_or_default(),
                        date: dict_text(doc, sig, b"M")
                            .map(|m| util::pdf_date_display(&m))
                            .unwrap_or_default(),
                        reason: dict_text(doc, sig, b"Reason").unwrap_or_default(),
                        location: dict_text(doc, sig, b"Location").unwrap_or_default(),
                        filter: sig
                            .get(b"SubFilter")
                            .ok()
                            .and_then(|s| s.as_name().ok())
                            .map(|n| String::from_utf8_lossy(n).to_string())
                            .unwrap_or_default(),
                        page: f.page,
                    },
                    None => SignatureInfo {
                        field: f.name,
                        page: f.page,
                        ..Default::default()
                    },
                }
            })
            .collect()
    }

    /// Export field values as `name = value` lines (simple FDF-like text).
    pub fn form_values(&self) -> Vec<(String, String)> {
        self.form_fields()
            .into_iter()
            .map(|f| (f.name, f.value))
            .collect()
    }
}
