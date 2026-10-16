//! Document-level JavaScript: listing and removal.

use lopdf::{Dictionary, Object};

use crate::document::Document;
use crate::error::Result;
use crate::util::{deref, text_of, walk_name_tree};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInfo {
    pub name: String,
    pub source: String,
    pub code: String,
}

fn script_code(doc: &lopdf::Document, action: &Dictionary) -> Option<String> {
    let js = action.get(b"JS").ok()?;
    match deref(doc, js) {
        Object::Stream(s) => Some(
            String::from_utf8_lossy(&s.decompressed_content().unwrap_or_else(|_| s.content.clone()))
                .to_string(),
        ),
        other => text_of(doc, other),
    }
}

impl Document {
    /// Document-level scripts (name tree) and a JavaScript open action, if any.
    pub fn scripts(&self) -> Vec<ScriptInfo> {
        let doc = &self.inner;
        let mut out = Vec::new();
        let Ok(catalog) = doc.catalog() else { return out };
        if let Some(names) = catalog
            .get(b"Names")
            .ok()
            .map(|n| deref(doc, n))
            .and_then(|n| n.as_dict().ok())
        {
            if let Some(tree) = names
                .get(b"JavaScript")
                .ok()
                .map(|j| deref(doc, j))
                .and_then(|j| j.as_dict().ok())
            {
                let mut pairs = Vec::new();
                walk_name_tree(doc, tree, &mut pairs, 0);
                for (key, value) in pairs {
                    if let Ok(action) = deref(doc, &value).as_dict() {
                        out.push(ScriptInfo {
                            name: String::from_utf8_lossy(&key).to_string(),
                            source: "Document".into(),
                            code: script_code(doc, action).unwrap_or_default(),
                        });
                    }
                }
            }
        }
        if let Some(action) = catalog
            .get(b"OpenAction")
            .ok()
            .map(|a| deref(doc, a))
            .and_then(|a| a.as_dict().ok())
        {
            if action.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"JavaScript") {
                out.push(ScriptInfo {
                    name: "OpenAction".into(),
                    source: "Open action".into(),
                    code: script_code(doc, action).unwrap_or_default(),
                });
            }
        }
        out
    }

    /// Remove document-level scripts and a JavaScript open action. Returns the count removed.
    pub fn remove_scripts(&mut self) -> Result<usize> {
        let count = self.scripts().len();
        if count == 0 {
            return Ok(0);
        }
        self.begin_edit();
        let names_ref = self.inner.catalog()?.get(b"Names").ok().cloned();
        match names_ref {
            Some(Object::Reference(r)) => {
                if let Ok(d) = self.inner.get_dictionary_mut(r) {
                    d.remove(b"JavaScript");
                }
            }
            Some(Object::Dictionary(_)) => {
                if let Ok(Object::Dictionary(d)) = self.inner.catalog_mut()?.get_mut(b"Names") {
                    d.remove(b"JavaScript");
                }
            }
            _ => {}
        }
        let open_is_js = self
            .inner
            .catalog()?
            .get(b"OpenAction")
            .ok()
            .map(|a| deref(&self.inner, a))
            .and_then(|a| a.as_dict().ok())
            .map(|a| a.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"JavaScript"))
            .unwrap_or(false);
        if open_is_js {
            self.inner.catalog_mut()?.remove(b"OpenAction");
        }
        self.inner.prune_objects();
        self.commit()?;
        Ok(count)
    }
}
