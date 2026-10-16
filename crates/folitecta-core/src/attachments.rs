//! Embedded file attachments.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{self, deref, dict_text, walk_name_tree};

#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub description: String,
    pub size: Option<u64>,
    pub modified: String,
    pub mime: String,
    /// The embedded file stream.
    pub stream: ObjectId,
    /// Page index when the attachment comes from a file-attachment annotation.
    pub page: Option<usize>,
    /// Key in the EmbeddedFiles name tree (None for annotation attachments).
    pub key: Option<Vec<u8>>,
}

/// Decode `#XX` escapes of a PDF name.
fn decode_name(name: &[u8]) -> String {
    let mut out = Vec::with_capacity(name.len());
    let mut i = 0;
    while i < name.len() {
        if name[i] == b'#' && i + 2 < name.len() {
            let hex = std::str::from_utf8(&name[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(b) = hex {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(name[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

impl Document {
    fn filespec_to_attachment(
        &self,
        spec: &Dictionary,
        key: Option<Vec<u8>>,
        page: Option<usize>,
    ) -> Option<Attachment> {
        let doc = &self.inner;
        let ef = deref(doc, spec.get(b"EF").ok()?).as_dict().ok()?;
        let stream_ref = ef
            .get(b"UF")
            .or_else(|_| ef.get(b"F"))
            .ok()?
            .as_reference()
            .ok()?;
        let stream = doc.get_object(stream_ref).ok()?.as_stream().ok()?;
        let params = stream
            .dict
            .get(b"Params")
            .ok()
            .map(|p| deref(doc, p))
            .and_then(|p| p.as_dict().ok());
        let name = dict_text(doc, spec, b"UF")
            .or_else(|| dict_text(doc, spec, b"F"))
            .or_else(|| key.as_ref().map(|k| String::from_utf8_lossy(k).to_string()))
            .unwrap_or_else(|| "attachment".into());
        Some(Attachment {
            name,
            description: dict_text(doc, spec, b"Desc").unwrap_or_default(),
            size: params
                .and_then(|p| p.get(b"Size").ok())
                .and_then(|s| s.as_i64().ok())
                .map(|s| s as u64)
                .or(Some(stream.content.len() as u64)),
            modified: params
                .and_then(|p| dict_text(doc, p, b"ModDate"))
                .map(|m| util::pdf_date_display(&m))
                .unwrap_or_default(),
            mime: stream
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|s| s.as_name().ok())
                .map(decode_name)
                .unwrap_or_default(),
            stream: stream_ref,
            page,
            key,
        })
    }

    fn embedded_files_tree(&self) -> Option<&Dictionary> {
        let catalog = self.inner.catalog().ok()?;
        let names = deref(&self.inner, catalog.get(b"Names").ok()?).as_dict().ok()?;
        deref(&self.inner, names.get(b"EmbeddedFiles").ok()?)
            .as_dict()
            .ok()
    }

    /// All attachments: document-level embedded files plus file attachment annotations.
    pub fn attachments(&self) -> Vec<Attachment> {
        let doc = &self.inner;
        let mut out = Vec::new();
        if let Some(tree) = self.embedded_files_tree() {
            let mut pairs = Vec::new();
            walk_name_tree(doc, tree, &mut pairs, 0);
            for (key, value) in pairs {
                if let Ok(spec) = deref(doc, &value).as_dict() {
                    if let Some(att) = self.filespec_to_attachment(spec, Some(key), None) {
                        out.push(att);
                    }
                }
            }
        }
        for (idx, pid) in self.page_ids().iter().enumerate() {
            let Ok(page) = doc.get_dictionary(*pid) else {
                continue;
            };
            let Ok(arr) = page
                .get(b"Annots")
                .map(|a| deref(doc, a))
                .and_then(|a| a.as_array())
            else {
                continue;
            };
            for a in arr {
                let Ok(d) = deref(doc, a).as_dict() else { continue };
                if d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) != Some(b"FileAttachment") {
                    continue;
                }
                if let Some(spec) = d
                    .get(b"FS")
                    .ok()
                    .map(|f| deref(doc, f))
                    .and_then(|f| f.as_dict().ok())
                {
                    if let Some(att) = self.filespec_to_attachment(spec, None, Some(idx)) {
                        out.push(att);
                    }
                }
            }
        }
        out
    }

    /// Decoded bytes of an attachment.
    pub fn attachment_data(&self, att: &Attachment) -> Result<Vec<u8>> {
        let stream = self.inner.get_object(att.stream)?.as_stream()?;
        Ok(stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()))
    }

    /// Write an attachment to disk.
    pub fn save_attachment(&self, att: &Attachment, path: impl AsRef<std::path::Path>) -> Result<()> {
        let data = self.attachment_data(att)?;
        std::fs::write(path.as_ref(), data).map_err(|e| Error::io(path.as_ref(), e))
    }

    fn embedded_pairs(&self) -> Vec<(Vec<u8>, Object)> {
        let mut pairs = Vec::new();
        if let Some(tree) = self.embedded_files_tree() {
            walk_name_tree(&self.inner, tree, &mut pairs, 0);
        }
        pairs
    }

    fn write_embedded_pairs(&mut self, mut pairs: Vec<(Vec<u8>, Object)>) -> Result<()> {
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let names: Vec<Object> = pairs
            .into_iter()
            .flat_map(|(k, v)| [Object::String(k, lopdf::StringFormat::Literal), v])
            .collect();
        let tree = dictionary! { "Names" => names };
        let catalog_names = self.inner.catalog()?.get(b"Names").ok().cloned();
        match catalog_names {
            Some(Object::Reference(r)) => {
                self.inner
                    .get_dictionary_mut(r)?
                    .set("EmbeddedFiles", Object::Dictionary(tree));
            }
            Some(Object::Dictionary(_)) => {
                let c = self.inner.catalog_mut()?;
                if let Ok(Object::Dictionary(n)) = c.get_mut(b"Names") {
                    n.set("EmbeddedFiles", Object::Dictionary(tree));
                }
            }
            _ => {
                let id = self.inner.add_object(dictionary! { "EmbeddedFiles" => tree });
                self.inner.catalog_mut()?.set("Names", Object::Reference(id));
            }
        }
        Ok(())
    }

    /// Attach a file to the document.
    pub fn add_attachment(
        &mut self,
        name: &str,
        data: Vec<u8>,
        description: &str,
        mime: Option<&str>,
    ) -> Result<()> {
        if name.trim().is_empty() {
            return Err(Error::Invalid("attachment name is empty".into()));
        }
        self.begin_edit();
        let size = data.len() as i64;
        let mut stream_dict = dictionary! {
            "Type" => "EmbeddedFile",
            "Params" => dictionary! { "Size" => size, "ModDate" => Object::string_literal(util::pdf_date_now()) },
        };
        if let Some(m) = mime {
            stream_dict.set("Subtype", Object::Name(m.replace('/', "#2F").into_bytes()));
        }
        let stream_id = self.inner.add_object(Stream::new(stream_dict, data));
        let mut spec = dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal(name.to_string()),
            "UF" => lopdf::text_string(name),
            "EF" => dictionary! { "F" => stream_id, "UF" => stream_id },
        };
        if !description.is_empty() {
            spec.set("Desc", lopdf::text_string(description));
        }
        let spec_id = self.inner.add_object(spec);
        let mut pairs = self.embedded_pairs();
        pairs.retain(|(k, _)| k != name.as_bytes());
        pairs.push((name.as_bytes().to_vec(), Object::Reference(spec_id)));
        self.write_embedded_pairs(pairs)?;
        self.commit()
    }

    /// Attach a file from disk (name = file name).
    pub fn add_attachment_from_path(
        &mut self,
        path: impl AsRef<std::path::Path>,
        description: &str,
    ) -> Result<()> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|e| Error::io(path, e))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        self.add_attachment(&name, data, description, None)
    }

    /// Remove a document-level attachment (annotation attachments are removed with their annotation).
    pub fn remove_attachment(&mut self, att: &Attachment) -> Result<()> {
        let key = att
            .key
            .clone()
            .ok_or_else(|| Error::Invalid("this attachment belongs to a page comment".into()))?;
        self.begin_edit();
        let mut pairs = self.embedded_pairs();
        pairs.retain(|(k, _)| *k != key);
        self.write_embedded_pairs(pairs)?;
        self.inner.prune_objects();
        self.commit()
    }
}
