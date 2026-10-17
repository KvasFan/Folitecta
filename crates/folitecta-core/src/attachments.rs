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
    /// Stored (compressed) size when the file is compressed.
    pub compressed: Option<u64>,
    /// How the file relates to the document (`/AFRelationship`), e.g. `Source`.
    pub relationship: String,
    pub created: String,
    /// The file-attachment annotation holding it.
    pub annotation: Option<ObjectId>,
}

/// Relationships of associated files.
pub const RELATIONSHIPS: &[&str] = &[
    "Unspecified",
    "Source",
    "Data",
    "Alternative",
    "Supplement",
    "EncryptedPayload",
    "FormData",
    "Schema",
];

/// File types that can run code when opened.
pub fn is_executable_name(name: &str) -> bool {
    const RISKY: &[&str] = &[
        "exe", "com", "bat", "cmd", "scr", "pif", "msi", "msp", "ps1", "psm1", "vbs", "vbe", "js", "jse",
        "wsf", "wsh", "hta", "jar", "cpl", "lnk", "reg", "sh", "bash", "command", "app", "dmg", "pkg", "run",
        "appimage", "deb", "rpm", "dll", "so", "dylib", "py", "pl", "rb", "html", "htm", "svg", "url",
        "desktop", "iso", "docm", "xlsm", "pptm",
    ];
    std::path::Path::new(name)
        .extension()
        .map(|e| RISKY.contains(&e.to_string_lossy().to_lowercase().as_str()))
        .unwrap_or(true)
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
        annotation: Option<ObjectId>,
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
            compressed: stream.dict.has(b"Filter").then_some(stream.content.len() as u64),
            relationship: spec
                .get(b"AFRelationship")
                .ok()
                .and_then(|r| r.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).to_string())
                .unwrap_or_default(),
            created: params
                .and_then(|p| dict_text(doc, p, b"CreationDate"))
                .map(|m| util::pdf_date_display(&m))
                .unwrap_or_default(),
            annotation,
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
                    if let Some(att) = self.filespec_to_attachment(spec, Some(key), None, None) {
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
                    let annot = a.as_reference().ok();
                    if let Some(att) = self.filespec_to_attachment(spec, None, Some(idx), annot) {
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

    pub(crate) fn embedded_pairs(&self) -> Vec<(Vec<u8>, Object)> {
        let mut pairs = Vec::new();
        if let Some(tree) = self.embedded_files_tree() {
            walk_name_tree(&self.inner, tree, &mut pairs, 0);
        }
        pairs
    }

    pub(crate) fn write_embedded_pairs(&mut self, mut pairs: Vec<(Vec<u8>, Object)>) -> Result<()> {
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
        self.insert_attachment(name.as_bytes().to_vec(), name, data, description, mime)?;
        self.commit()
    }

    /// Embed a file under the name-tree `key` (replacing a file with the same
    /// key), without an undo step. Returns its file specification.
    pub(crate) fn insert_attachment(
        &mut self,
        key: Vec<u8>,
        name: &str,
        data: Vec<u8>,
        description: &str,
        mime: Option<&str>,
    ) -> Result<ObjectId> {
        let size = data.len() as i64;
        let mut stream_dict = dictionary! {
            "Type" => "EmbeddedFile",
            "Params" => dictionary! {
                "Size" => size,
                "CreationDate" => Object::string_literal(util::pdf_date_now()),
                "ModDate" => Object::string_literal(util::pdf_date_now()),
            },
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
        pairs.retain(|(k, _)| *k != key);
        pairs.push((key, Object::Reference(spec_id)));
        self.write_embedded_pairs(pairs)?;
        Ok(spec_id)
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

    /// The file specification object of an attachment, when it is an indirect object.
    fn filespec_ref(&self, att: &Attachment) -> Option<ObjectId> {
        match &att.key {
            Some(key) => self
                .embedded_pairs()
                .into_iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_reference().ok()),
            None => self
                .inner
                .get_dictionary(att.annotation?)
                .ok()?
                .get(b"FS")
                .ok()?
                .as_reference()
                .ok(),
        }
    }

    /// Run `f` on the file specification of an attachment.
    fn edit_filespec(&mut self, att: &Attachment, f: impl FnOnce(&mut Dictionary)) -> Result<()> {
        let missing = || Error::Invalid("attachment not found".into());
        if let Some(r) = self.filespec_ref(att) {
            f(self.inner.get_dictionary_mut(r)?);
            return Ok(());
        }
        if let Some(key) = &att.key {
            let mut pairs = self.embedded_pairs();
            let pos = pairs.iter().position(|(k, _)| k == key).ok_or_else(missing)?;
            let Object::Dictionary(mut d) = pairs[pos].1.clone() else {
                return Err(missing());
            };
            f(&mut d);
            pairs[pos].1 = Object::Dictionary(d);
            return self.write_embedded_pairs(pairs);
        }
        let annot = att.annotation.ok_or_else(missing)?;
        match self.inner.get_dictionary_mut(annot)?.get_mut(b"FS") {
            Ok(Object::Dictionary(d)) => {
                f(d);
                Ok(())
            }
            _ => Err(missing()),
        }
    }

    /// Change the name, description and relationship of an attachment (one undo step).
    pub fn set_attachment_info(
        &mut self,
        att: &Attachment,
        name: &str,
        description: &str,
        relationship: &str,
    ) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("attachment name is empty".into()));
        }
        if !relationship.is_empty() && !RELATIONSHIPS.contains(&relationship) {
            return Err(Error::Invalid(format!("unknown relationship {relationship}")));
        }
        let renamed = att.key.as_deref().filter(|k| *k != name.as_bytes());
        if renamed.is_some() && self.embedded_pairs().iter().any(|(k, _)| k == name.as_bytes()) {
            return Err(Error::Invalid(format!("an attachment named {name} exists")));
        }
        self.begin_edit();
        self.edit_filespec(att, |d| {
            d.set("F", Object::string_literal(name));
            d.set("UF", lopdf::text_string(name));
            if description.trim().is_empty() {
                d.remove(b"Desc");
            } else {
                d.set("Desc", lopdf::text_string(description.trim()));
            }
            if relationship.is_empty() || relationship == "Unspecified" {
                d.remove(b"AFRelationship");
            } else {
                d.set("AFRelationship", Object::Name(relationship.as_bytes().to_vec()));
            }
        })?;
        if let Some(old) = renamed {
            let old = old.to_vec();
            let mut pairs = self.embedded_pairs();
            for p in pairs.iter_mut().filter(|p| p.0 == old) {
                p.0 = name.as_bytes().to_vec();
            }
            self.write_embedded_pairs(pairs)?;
        }
        // Files with a relationship are associated with the document.
        if !relationship.is_empty() && relationship != "Unspecified" && att.key.is_some() {
            if let Some(r) = self.filespec_ref(att).or_else(|| {
                self.embedded_pairs()
                    .into_iter()
                    .find(|(k, _)| k == name.as_bytes())
                    .and_then(|(_, v)| v.as_reference().ok())
            }) {
                let cat = self.inner.catalog_mut()?;
                let mut af = match cat.get(b"AF") {
                    Ok(Object::Array(a)) => a.clone(),
                    _ => Vec::new(),
                };
                if !af.iter().any(|o| o.as_reference().ok() == Some(r)) {
                    af.push(Object::Reference(r));
                }
                cat.set("AF", Object::Array(af));
            }
        }
        self.commit()
    }

    /// Replace the contents of an attachment, keeping its name and details.
    pub fn replace_attachment_data(&mut self, att: &Attachment, data: Vec<u8>) -> Result<()> {
        let old = self.inner.get_object(att.stream)?.as_stream()?.dict.clone();
        let doc = &self.inner;
        let created = old
            .get(b"Params")
            .ok()
            .and_then(|p| deref(doc, p).as_dict().ok())
            .and_then(|p| p.get(b"CreationDate").ok().cloned());
        self.begin_edit();
        let mut params = dictionary! {
            "Size" => data.len() as i64,
            "ModDate" => Object::string_literal(util::pdf_date_now()),
        };
        if let Some(c) = created {
            params.set("CreationDate", c);
        }
        let mut dict = dictionary! { "Type" => "EmbeddedFile", "Params" => params };
        if let Ok(s) = old.get(b"Subtype") {
            dict.set("Subtype", s.clone());
        }
        let mut stream = Stream::new(dict, data);
        let _ = stream.compress();
        // Same object: every reference to the file stays valid.
        self.inner.objects.insert(att.stream, Object::Stream(stream));
        self.commit()
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
