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
        });
    }

    /// Set the value of a text, choice or check box field.
    pub fn set_field_value(&mut self, id: ObjectId, value: &str) -> Result<()> {
        let field = self
            .form_fields()
            .into_iter()
            .find(|f| f.id == id)
            .ok_or_else(|| Error::Invalid("field not found".into()))?;
        self.begin_edit();
        let new_value = match field.kind {
            FieldKind::CheckBox | FieldKind::RadioButton => {
                let on = if value.is_empty() || value == "Off" {
                    "Off"
                } else {
                    value
                };
                Object::Name(on.as_bytes().to_vec())
            }
            _ => lopdf::text_string(value),
        };
        {
            let d = self.inner.get_dictionary_mut(id)?;
            d.set("V", new_value.clone());
            if matches!(field.kind, FieldKind::CheckBox | FieldKind::RadioButton) {
                d.set("AS", new_value.clone());
            }
            // Drop stale appearance streams of text fields; viewers regenerate them.
            if matches!(
                field.kind,
                FieldKind::Text | FieldKind::ComboBox | FieldKind::ListBox
            ) {
                d.remove(b"AP");
            }
        }
        // Widget kids share the value through inheritance, but check box kids need /AS.
        let kids: Vec<ObjectId> = self
            .inner
            .get_dictionary(id)?
            .get(b"Kids")
            .ok()
            .and_then(|k| k.as_array().ok())
            .map(|arr| arr.iter().filter_map(|k| k.as_reference().ok()).collect())
            .unwrap_or_default();
        for kid in kids {
            if let Ok(d) = self.inner.get_dictionary_mut(kid) {
                if matches!(field.kind, FieldKind::CheckBox | FieldKind::RadioButton) {
                    d.set("AS", new_value.clone());
                } else {
                    d.remove(b"AP");
                }
            }
        }
        self.set_need_appearances()?;
        self.commit()
    }

    fn set_need_appearances(&mut self) -> Result<()> {
        let catalog = self.inner.catalog()?;
        let form_ref = catalog.get(b"AcroForm").ok().cloned();
        match form_ref {
            Some(Object::Reference(r)) => {
                self.inner.get_dictionary_mut(r)?.set("NeedAppearances", true);
            }
            Some(Object::Dictionary(_)) => {
                let c = self.inner.catalog_mut()?;
                if let Ok(f) = c.get_mut(b"AcroForm") {
                    if let Ok(d) = f.as_dict_mut() {
                        d.set("NeedAppearances", true);
                    }
                }
            }
            _ => {}
        }
        Ok(())
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
            if let Ok(d) = self.inner.get_dictionary_mut(f.id) {
                match d.get(b"DV").ok().cloned() {
                    Some(dv) => {
                        d.set("V", dv.clone());
                        if matches!(f.kind, FieldKind::CheckBox | FieldKind::RadioButton) {
                            d.set("AS", dv);
                        }
                    }
                    None => {
                        d.remove(b"V");
                        if matches!(f.kind, FieldKind::CheckBox | FieldKind::RadioButton) {
                            d.set("AS", Object::Name(b"Off".to_vec()));
                        }
                    }
                }
                d.remove(b"AP");
                count += 1;
            }
        }
        self.set_need_appearances()?;
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
