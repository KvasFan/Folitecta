//! Optional content groups (layers).

use lopdf::{Dictionary, Object, ObjectId};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, dict_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub id: ObjectId,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
}

fn refs_of(doc: &lopdf::Document, dict: &Dictionary, key: &[u8]) -> Vec<ObjectId> {
    dict.get(key)
        .map(|o| deref(doc, o))
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|arr| arr.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default()
}

impl Document {
    fn ocproperties(&self) -> Option<&Dictionary> {
        let catalog = self.inner.catalog().ok()?;
        deref(&self.inner, catalog.get(b"OCProperties").ok()?)
            .as_dict()
            .ok()
    }

    /// Layers of the document in declaration order.
    pub fn layers(&self) -> Vec<Layer> {
        let doc = &self.inner;
        let Some(oc) = self.ocproperties() else {
            return Vec::new();
        };
        let ocgs = refs_of(doc, oc, b"OCGs");
        let d = oc
            .get(b"D")
            .map(|d| deref(doc, d))
            .ok()
            .and_then(|d| d.as_dict().ok());
        let (on, off, locked, base_off) = match d {
            Some(d) => (
                refs_of(doc, d, b"ON"),
                refs_of(doc, d, b"OFF"),
                refs_of(doc, d, b"Locked"),
                d.get(b"BaseState").ok().and_then(|b| b.as_name().ok()) == Some(b"OFF"),
            ),
            None => (Vec::new(), Vec::new(), Vec::new(), false),
        };
        ocgs.into_iter()
            .filter_map(|id| {
                let dict = doc.get_dictionary(id).ok()?;
                let name = dict_text(doc, dict, b"Name").unwrap_or_else(|| format!("Layer {}", id.0));
                let visible = if base_off {
                    on.contains(&id)
                } else {
                    !off.contains(&id)
                };
                Some(Layer {
                    id,
                    name,
                    visible,
                    locked: locked.contains(&id),
                })
            })
            .collect()
    }

    fn with_default_config(&mut self, f: impl FnOnce(&mut Dictionary)) -> Result<()> {
        let oc_obj = self
            .inner
            .catalog()?
            .get(b"OCProperties")
            .ok()
            .cloned()
            .ok_or_else(|| Error::Invalid("the document has no layers".into()))?;
        let oc_id = match oc_obj {
            Object::Reference(r) => Some(r),
            _ => None,
        };
        // Work on an owned copy of the OCProperties dictionary, then write it back.
        let mut oc = match oc_id {
            Some(r) => self.inner.get_dictionary(r)?.clone(),
            None => oc_obj.as_dict()?.clone(),
        };
        let d_obj = oc.get(b"D").ok().cloned();
        match d_obj {
            Some(Object::Reference(dr)) => {
                let d = self.inner.get_dictionary_mut(dr)?;
                f(d);
            }
            Some(Object::Dictionary(mut d)) => {
                f(&mut d);
                oc.set("D", Object::Dictionary(d));
            }
            _ => {
                let mut d = Dictionary::new();
                f(&mut d);
                oc.set("D", Object::Dictionary(d));
            }
        }
        match oc_id {
            Some(r) => {
                *self.inner.get_object_mut(r)? = Object::Dictionary(oc);
            }
            None => {
                self.inner
                    .catalog_mut()?
                    .set("OCProperties", Object::Dictionary(oc));
            }
        }
        Ok(())
    }

    /// Change the default visibility of a layer.
    pub fn set_layer_visible(&mut self, id: ObjectId, visible: bool) -> Result<()> {
        if !self.layers().iter().any(|l| l.id == id) {
            return Err(Error::Invalid("layer not found".into()));
        }
        self.begin_edit();
        self.with_default_config(|d| {
            let base_off = d.get(b"BaseState").ok().and_then(|b| b.as_name().ok()) == Some(b"OFF");
            for key in ["ON", "OFF"] {
                let mut arr = d
                    .get(key.as_bytes())
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .cloned()
                    .unwrap_or_default();
                arr.retain(|o| o.as_reference().ok() != Some(id));
                d.set(key, Object::Array(arr));
            }
            let target = if visible {
                if base_off { Some("ON") } else { None }
            } else {
                Some("OFF")
            };
            if let Some(key) = target {
                if let Ok(Object::Array(arr)) = d.get_mut(key.as_bytes()) {
                    arr.push(Object::Reference(id));
                }
            }
        })?;
        self.commit()
    }

    /// Show or hide every layer.
    pub fn set_all_layers_visible(&mut self, visible: bool) -> Result<()> {
        let ids: Vec<ObjectId> = self.layers().iter().map(|l| l.id).collect();
        if ids.is_empty() {
            return Ok(());
        }
        self.begin_edit();
        self.with_default_config(|d| {
            d.remove(b"BaseState");
            d.set(
                "ON",
                Object::Array(if visible {
                    ids.iter().map(|i| Object::Reference(*i)).collect()
                } else {
                    Vec::new()
                }),
            );
            d.set(
                "OFF",
                Object::Array(if visible {
                    Vec::new()
                } else {
                    ids.iter().map(|i| Object::Reference(*i)).collect()
                }),
            );
        })?;
        self.commit()
    }
}
