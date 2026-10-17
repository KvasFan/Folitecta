//! PDF portfolios: a cover document holding files in folders, with the
//! columns (schema), sort order, initial file and layout shown to readers.

use std::collections::{BTreeMap, HashMap};

use lopdf::{Dictionary, Object, ObjectId, dictionary};

use crate::attachments::Attachment;
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{decode_pdf_text, deref, dict_text};

/// A column of the portfolio's file list.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaField {
    /// Key of the field in the schema and in the files' item data.
    pub key: String,
    /// Column title.
    pub name: String,
    /// `F` file name, `Desc` description, `ModDate`, `CreationDate`, `Size`,
    /// `CompressedSize`, or custom data: `S` text, `D` date, `N` number.
    pub kind: String,
    pub visible: bool,
    pub editable: bool,
}

/// Kinds of portfolio columns.
pub const FIELD_KINDS: &[&str] = &[
    "F",
    "Desc",
    "ModDate",
    "CreationDate",
    "Size",
    "CompressedSize",
    "S",
    "D",
    "N",
];

#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioFolder {
    pub id: i64,
    pub name: String,
    /// `None` for the root folder.
    pub parent: Option<i64>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioItem {
    /// Key in the embedded files (with the folder prefix).
    pub key: Vec<u8>,
    pub name: String,
    /// Folder holding the file (`None`: the root).
    pub folder: Option<i64>,
    pub attachment: Attachment,
    /// Values of the custom columns.
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Portfolio {
    /// `D` details, `T` tiles, `H` cover only (files hidden).
    pub view: String,
    pub schema: Vec<SchemaField>,
    /// File shown first.
    pub initial: Option<Vec<u8>>,
    /// Sort column and ascending order.
    pub sort: Option<(String, bool)>,
    pub root: Option<i64>,
    pub folders: Vec<PortfolioFolder>,
    pub items: Vec<PortfolioItem>,
}

impl Portfolio {
    /// Folders inside `parent` (`None`: inside the root).
    pub fn subfolders(&self, parent: Option<i64>) -> Vec<&PortfolioFolder> {
        let parent = parent.or(self.root);
        let mut out: Vec<&PortfolioFolder> = self
            .folders
            .iter()
            .filter(|f| f.parent.is_some() && f.parent == parent)
            .collect();
        out.sort_by_key(|f| f.name.to_lowercase());
        out
    }

    /// Files inside a folder (`None`: the root), in the portfolio's sort order.
    pub fn files_in(&self, folder: Option<i64>) -> Vec<&PortfolioItem> {
        let folder = folder.filter(|f| Some(*f) != self.root);
        let mut out: Vec<&PortfolioItem> = self.items.iter().filter(|i| i.folder == folder).collect();
        self.sort_items(&mut out);
        out
    }

    /// Value shown in a column for a file.
    pub fn value(&self, item: &PortfolioItem, field: &SchemaField) -> String {
        let a = &item.attachment;
        match field.kind.as_str() {
            "F" => item.name.clone(),
            "Desc" => a.description.clone(),
            "ModDate" => a.modified.clone(),
            "CreationDate" => a.created.clone(),
            "Size" => a.size.map(|s| s.to_string()).unwrap_or_default(),
            "CompressedSize" => a.compressed.map(|s| s.to_string()).unwrap_or_default(),
            _ => item.fields.get(&field.key).cloned().unwrap_or_default(),
        }
    }

    /// Order files as the portfolio sorts them (by name when it does not say).
    pub fn sort_items(&self, items: &mut Vec<&PortfolioItem>) {
        let Some((key, asc)) = &self.sort else {
            items.sort_by_key(|i| i.name.to_lowercase());
            return;
        };
        let Some(field) = self.schema.iter().find(|f| &f.key == key) else {
            return;
        };
        let numeric = matches!(field.kind.as_str(), "Size" | "CompressedSize" | "N");
        items.sort_by(|a, b| {
            let (va, vb) = (self.value(a, field), self.value(b, field));
            let o = if numeric {
                let n = |v: &str| v.trim().parse::<f64>().unwrap_or(0.0);
                n(&va).partial_cmp(&n(&vb)).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                va.to_lowercase().cmp(&vb.to_lowercase())
            };
            if *asc { o } else { o.reverse() }
        });
    }
}

/// Folder id and file name of a name-tree key (`<3>report.pdf`).
fn split_key(key: &[u8]) -> (Option<i64>, String) {
    let s = decode_pdf_text(key);
    if let Some(rest) = s.strip_prefix('<') {
        if let Some((id, name)) = rest.split_once('>') {
            if let Ok(id) = id.parse::<i64>() {
                return (Some(id), name.to_string());
            }
        }
    }
    (None, s)
}

fn folder_key(folder: Option<i64>, name: &str) -> Vec<u8> {
    match folder {
        Some(id) => format!("<{id}>{name}").into_bytes(),
        None => name.as_bytes().to_vec(),
    }
}

fn default_schema() -> Dictionary {
    let field = |name: &str, kind: &str, order: i64| {
        Object::Dictionary(dictionary! {
            "Type" => "CollectionField",
            "Subtype" => Object::Name(kind.as_bytes().to_vec()),
            "N" => lopdf::text_string(name),
            "O" => order,
            "V" => true,
        })
    };
    dictionary! {
        "Type" => "CollectionSchema",
        "FileName" => field("Name", "F", 0),
        "Description" => field("Description", "Desc", 1),
        "Modified" => field("Modified", "ModDate", 2),
        "Size" => field("Size", "Size", 3),
    }
}

impl Document {
    fn collection(&self) -> Option<&Dictionary> {
        let cat = self.inner.catalog().ok()?;
        deref(&self.inner, cat.get(b"Collection").ok()?).as_dict().ok()
    }

    pub fn is_portfolio(&self) -> bool {
        self.collection().is_some()
    }

    /// Folders with their objects (root first).
    fn portfolio_folders(&self) -> Vec<(ObjectId, PortfolioFolder)> {
        let doc = &self.inner;
        let mut out = Vec::new();
        let Some(root) = self
            .collection()
            .and_then(|c| c.get(b"Folders").ok())
            .and_then(|f| f.as_reference().ok())
        else {
            return out;
        };
        let mut stack = vec![(root, None)];
        let mut seen = std::collections::HashSet::new();
        while let Some((id, parent)) = stack.pop() {
            if !seen.insert(id) || seen.len() > 10_000 {
                continue;
            }
            let Ok(d) = doc.get_dictionary(id) else { continue };
            let fid = d.get(b"ID").ok().and_then(|v| v.as_i64().ok()).unwrap_or(0);
            out.push((
                id,
                PortfolioFolder {
                    id: fid,
                    name: dict_text(doc, d, b"Name").unwrap_or_default(),
                    parent,
                    description: dict_text(doc, d, b"Desc").unwrap_or_default(),
                },
            ));
            // Children: the first child, then its siblings.
            let mut child = d.get(b"Child").ok().and_then(|c| c.as_reference().ok());
            let mut guard = 0;
            while let Some(c) = child {
                guard += 1;
                if guard > 10_000 {
                    break;
                }
                stack.push((c, Some(fid)));
                child = doc
                    .get_dictionary(c)
                    .ok()
                    .and_then(|cd| cd.get(b"Next").ok())
                    .and_then(|n| n.as_reference().ok());
            }
        }
        out
    }

    /// The portfolio of the document, when it is one.
    pub fn portfolio(&self) -> Option<Portfolio> {
        let doc = &self.inner;
        let coll = self.collection()?;
        let view = coll
            .get(b"View")
            .ok()
            .and_then(|v| v.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_string())
            .unwrap_or_else(|| "D".into());
        let mut schema: Vec<(i64, SchemaField)> = coll
            .get(b"Schema")
            .ok()
            .and_then(|s| deref(doc, s).as_dict().ok())
            .map(|s| {
                s.iter()
                    .filter_map(|(k, v)| {
                        let d = deref(doc, v).as_dict().ok()?;
                        let kind = d.get(b"Subtype").ok()?.as_name().ok()?;
                        Some((
                            d.get(b"O").ok().and_then(|o| o.as_i64().ok()).unwrap_or(i64::MAX),
                            SchemaField {
                                key: String::from_utf8_lossy(k).to_string(),
                                name: dict_text(doc, d, b"N")
                                    .unwrap_or_else(|| String::from_utf8_lossy(k).to_string()),
                                kind: String::from_utf8_lossy(kind).to_string(),
                                visible: d.get(b"V").ok().and_then(|v| v.as_bool().ok()).unwrap_or(true),
                                editable: d.get(b"E").ok().and_then(|v| v.as_bool().ok()).unwrap_or(false),
                            },
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        schema.sort_by_key(|(o, f)| (*o, f.key.clone()));
        let initial = coll.get(b"D").ok().and_then(|d| match d {
            Object::String(s, _) => Some(s.clone()),
            _ => None,
        });
        let sort = coll
            .get(b"Sort")
            .ok()
            .and_then(|s| deref(doc, s).as_dict().ok())
            .and_then(|s| {
                let key = match s.get(b"S").ok()? {
                    Object::Name(n) => String::from_utf8_lossy(n).to_string(),
                    Object::Array(a) => String::from_utf8_lossy(a.first()?.as_name().ok()?).to_string(),
                    _ => return None,
                };
                let asc = match s.get(b"A") {
                    Ok(Object::Boolean(b)) => *b,
                    Ok(Object::Array(a)) => a.first().and_then(|b| b.as_bool().ok()).unwrap_or(true),
                    _ => true,
                };
                Some((key, asc))
            });
        let folders = self.portfolio_folders();
        let root = folders.first().map(|(_, f)| f.id);
        // Item data of the files, by name-tree key.
        let mut data: HashMap<Vec<u8>, BTreeMap<String, String>> = HashMap::new();
        for (key, value) in self.embedded_pairs() {
            let Ok(spec) = deref(doc, &value).as_dict() else {
                continue;
            };
            let Some(ci) = spec.get(b"CI").ok().and_then(|c| deref(doc, c).as_dict().ok()) else {
                continue;
            };
            let mut fields = BTreeMap::new();
            for (k, v) in ci.iter() {
                let v = deref(doc, v);
                let text = match v {
                    Object::Dictionary(sub) => sub.get(b"D").ok().map(|x| deref(doc, x).clone()),
                    other => Some(other.clone()),
                };
                let s = match text {
                    Some(Object::String(s, _)) => {
                        let t = decode_pdf_text(&s);
                        if t.starts_with("D:") {
                            crate::util::pdf_date_display(&t)
                        } else {
                            t
                        }
                    }
                    Some(Object::Integer(i)) => i.to_string(),
                    Some(Object::Real(r)) => r.to_string(),
                    _ => String::new(),
                };
                fields.insert(String::from_utf8_lossy(k).to_string(), s);
            }
            data.insert(key, fields);
        }
        let items = self
            .attachments()
            .into_iter()
            .filter_map(|a| {
                let key = a.key.clone()?;
                let (folder, name) = split_key(&key);
                let folder = folder.filter(|f| Some(*f) != root);
                Some(PortfolioItem {
                    fields: data.remove(&key).unwrap_or_default(),
                    key,
                    name: if a.name.is_empty() { name } else { a.name.clone() },
                    folder,
                    attachment: a,
                })
            })
            .collect();
        Some(Portfolio {
            view,
            schema: schema.into_iter().map(|(_, f)| f).collect(),
            initial,
            sort,
            root,
            folders: folders.into_iter().map(|(_, f)| f).collect(),
            items,
        })
    }

    /// Edit the collection dictionary (created when missing).
    fn edit_collection(&mut self, f: impl FnOnce(&mut Dictionary)) -> Result<()> {
        let existing = self.inner.catalog()?.get(b"Collection").ok().cloned();
        match existing {
            Some(Object::Reference(r)) => f(self.inner.get_dictionary_mut(r)?),
            Some(Object::Dictionary(mut d)) => {
                f(&mut d);
                self.inner.catalog_mut()?.set("Collection", Object::Dictionary(d));
            }
            _ => {
                let mut d = dictionary! { "Type" => "Collection", "View" => "D" };
                f(&mut d);
                let id = self.inner.add_object(d);
                self.inner.catalog_mut()?.set("Collection", Object::Reference(id));
            }
        }
        Ok(())
    }

    fn check_portfolio(&self) -> Result<()> {
        if self.is_portfolio() {
            Ok(())
        } else {
            Err(Error::Invalid("the document is not a portfolio".into()))
        }
    }

    /// Turn the document into a portfolio: its attachments become the files,
    /// its pages the cover (one undo step).
    pub fn make_portfolio(&mut self) -> Result<()> {
        if self.is_portfolio() {
            return Ok(());
        }
        self.begin_edit();
        let root = self.inner.add_object(dictionary! {
            "Type" => "Folder",
            "ID" => 0,
            "Name" => lopdf::text_string(""),
        });
        self.edit_collection(|c| {
            c.set("Schema", Object::Dictionary(default_schema()));
            c.set("Folders", Object::Reference(root));
            c.set(
                "Sort",
                Object::Dictionary(dictionary! { "S" => "FileName", "A" => true }),
            );
        })?;
        self.inner
            .catalog_mut()?
            .set("PageMode", Object::Name(b"UseAttachments".to_vec()));
        self.commit()
    }

    /// Stop being a portfolio (the files stay attached).
    pub fn remove_portfolio(&mut self) -> Result<()> {
        self.check_portfolio()?;
        self.begin_edit();
        self.inner.catalog_mut()?.remove(b"Collection");
        self.commit()
    }

    /// A new portfolio with the given files and a cover page.
    pub fn new_portfolio(files: Vec<(String, Vec<u8>)>, cover_title: &str) -> Result<Document> {
        let text = format!(
            "{cover_title}\n\n{}",
            files
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut doc = Document::from_text(&text, crate::textdoc::TextLayout::default())?;
        doc.make_portfolio()?;
        doc.begin_edit();
        for (name, data) in files {
            let mime = mime_of(&name);
            doc.insert_attachment(name.as_bytes().to_vec(), &name, data, "", mime)?;
        }
        doc.commit()?;
        doc.clear_history();
        Ok(doc)
    }

    /// Add a file to a folder of the portfolio (`None`: the root).
    pub fn portfolio_add_file(&mut self, name: &str, data: Vec<u8>, folder: Option<i64>) -> Result<()> {
        self.check_portfolio()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the file has no name".into()));
        }
        let p = self.portfolio().expect("checked above");
        let folder = folder.filter(|f| Some(*f) != p.root);
        if let Some(f) = folder {
            if !p.folders.iter().any(|x| x.id == f) {
                return Err(Error::Invalid("folder not found".into()));
            }
        }
        self.begin_edit();
        self.insert_attachment(folder_key(folder, name), name, data, "", mime_of(name))?;
        self.commit()
    }

    /// Rewrite the links between folder objects after a change of the list.
    fn write_folders(&mut self, folders: &[(ObjectId, PortfolioFolder)]) -> Result<()> {
        let obj_of: HashMap<i64, ObjectId> = folders.iter().map(|(o, f)| (f.id, *o)).collect();
        for (obj, f) in folders {
            let mut kids: Vec<&(ObjectId, PortfolioFolder)> =
                folders.iter().filter(|(_, x)| x.parent == Some(f.id)).collect();
            kids.sort_by_key(|(_, x)| x.name.to_lowercase());
            let d = self.inner.get_dictionary_mut(*obj)?;
            d.set("Type", Object::Name(b"Folder".to_vec()));
            d.set("ID", f.id);
            d.set("Name", lopdf::text_string(&f.name));
            if f.description.is_empty() {
                d.remove(b"Desc");
            } else {
                d.set("Desc", lopdf::text_string(&f.description));
            }
            match f.parent.and_then(|p| obj_of.get(&p)) {
                Some(p) => d.set("Parent", Object::Reference(*p)),
                None => {
                    d.remove(b"Parent");
                }
            }
            match kids.first() {
                Some((k, _)) => d.set("Child", Object::Reference(*k)),
                None => {
                    d.remove(b"Child");
                }
            }
            for (i, (k, _)) in kids.iter().enumerate() {
                let next = kids.get(i + 1).map(|(n, _)| *n);
                let kd = self.inner.get_dictionary_mut(*k)?;
                match next {
                    Some(n) => kd.set("Next", Object::Reference(n)),
                    None => {
                        kd.remove(b"Next");
                    }
                }
            }
        }
        Ok(())
    }

    /// Create a folder inside `parent` (`None`: the root). Returns its id.
    pub fn portfolio_create_folder(&mut self, name: &str, parent: Option<i64>) -> Result<i64> {
        self.check_portfolio()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the folder has no name".into()));
        }
        self.begin_edit();
        let mut folders = self.portfolio_folders();
        if folders.is_empty() {
            let root = self
                .inner
                .add_object(dictionary! { "Type" => "Folder", "ID" => 0, "Name" => lopdf::text_string("") });
            self.edit_collection(|c| c.set("Folders", Object::Reference(root)))?;
            folders = self.portfolio_folders();
        }
        let root = folders[0].1.id;
        let parent = parent.unwrap_or(root);
        if !folders.iter().any(|(_, f)| f.id == parent) {
            return Err(Error::Invalid("folder not found".into()));
        }
        let id = folders.iter().map(|(_, f)| f.id).max().unwrap_or(0) + 1;
        let obj = self
            .inner
            .add_object(dictionary! { "Type" => "Folder", "ID" => id });
        folders.push((
            obj,
            PortfolioFolder {
                id,
                name: name.to_string(),
                parent: Some(parent),
                description: String::new(),
            },
        ));
        self.write_folders(&folders)?;
        self.commit()?;
        Ok(id)
    }

    /// Rename a folder and set its description.
    pub fn portfolio_edit_folder(&mut self, id: i64, name: &str, description: &str) -> Result<()> {
        self.check_portfolio()?;
        let mut folders = self.portfolio_folders();
        let f = folders
            .iter_mut()
            .find(|(_, f)| f.id == id)
            .ok_or_else(|| Error::Invalid("folder not found".into()))?;
        if name.trim().is_empty() {
            return Err(Error::Invalid("the folder has no name".into()));
        }
        f.1.name = name.trim().to_string();
        f.1.description = description.trim().to_string();
        self.begin_edit();
        self.write_folders(&folders)?;
        self.commit()
    }

    /// Delete files and folders (with everything inside them).
    pub fn portfolio_delete(&mut self, keys: &[Vec<u8>], folders: &[i64]) -> Result<usize> {
        self.check_portfolio()?;
        let mut all = self.portfolio_folders();
        let root = all.first().map(|(_, f)| f.id);
        // The folders and everything below them.
        let mut gone: Vec<i64> = folders.iter().copied().filter(|f| Some(*f) != root).collect();
        let mut i = 0;
        while i < gone.len() {
            let id = gone[i];
            gone.extend(
                all.iter()
                    .filter(|(_, f)| f.parent == Some(id))
                    .map(|(_, f)| f.id),
            );
            i += 1;
        }
        let pairs = self.embedded_pairs();
        let before = pairs.len();
        let kept: Vec<(Vec<u8>, Object)> = pairs
            .into_iter()
            .filter(|(k, _)| {
                let (folder, _) = split_key(k);
                !keys.contains(k) && !folder.is_some_and(|f| gone.contains(&f))
            })
            .collect();
        let removed = before - kept.len() + gone.len();
        if removed == 0 {
            return Ok(0);
        }
        self.begin_edit();
        self.write_embedded_pairs(kept)?;
        let gone_objs: Vec<ObjectId> = all
            .iter()
            .filter(|(_, f)| gone.contains(&f.id))
            .map(|(o, _)| *o)
            .collect();
        all.retain(|(_, f)| !gone.contains(&f.id));
        self.write_folders(&all)?;
        for o in gone_objs {
            self.inner.objects.remove(&o);
        }
        self.inner.prune_objects();
        self.commit()?;
        Ok(removed)
    }

    /// Show the files as details (`D`), tiles (`T`) or only the cover (`H`).
    pub fn set_portfolio_view(&mut self, view: &str) -> Result<()> {
        self.check_portfolio()?;
        if !["D", "T", "H"].contains(&view) {
            return Err(Error::Invalid(format!("unknown portfolio view {view}")));
        }
        self.begin_edit();
        self.edit_collection(|c| c.set("View", Object::Name(view.as_bytes().to_vec())))?;
        self.commit()
    }

    /// The file shown first (`None`: the cover).
    pub fn set_portfolio_initial(&mut self, key: Option<&[u8]>) -> Result<()> {
        self.check_portfolio()?;
        self.begin_edit();
        self.edit_collection(|c| match key {
            Some(k) => c.set("D", Object::String(k.to_vec(), lopdf::StringFormat::Literal)),
            None => {
                c.remove(b"D");
            }
        })?;
        self.commit()
    }

    /// Sort the files by a column.
    pub fn set_portfolio_sort(&mut self, key: &str, ascending: bool) -> Result<()> {
        self.check_portfolio()?;
        self.begin_edit();
        self.edit_collection(|c| {
            c.set(
                "Sort",
                Object::Dictionary(
                    dictionary! { "S" => Object::Name(key.as_bytes().to_vec()), "A" => ascending },
                ),
            )
        })?;
        self.commit()
    }

    /// Replace the columns (their order is the order given).
    pub fn set_portfolio_schema(&mut self, fields: &[SchemaField]) -> Result<()> {
        self.check_portfolio()?;
        let mut seen = std::collections::HashSet::new();
        for f in fields {
            if f.key.trim().is_empty() || !seen.insert(f.key.clone()) {
                return Err(Error::Invalid(format!(
                    "column key {} is empty or repeated",
                    f.key
                )));
            }
            if !FIELD_KINDS.contains(&f.kind.as_str()) {
                return Err(Error::Invalid(format!("unknown column kind {}", f.kind)));
            }
        }
        let mut schema = dictionary! { "Type" => "CollectionSchema" };
        for (i, f) in fields.iter().enumerate() {
            schema.set(
                f.key.as_bytes().to_vec(),
                Object::Dictionary(dictionary! {
                    "Type" => "CollectionField",
                    "Subtype" => Object::Name(f.kind.as_bytes().to_vec()),
                    "N" => lopdf::text_string(&f.name),
                    "O" => i as i64,
                    "V" => f.visible,
                    "E" => f.editable,
                }),
            );
        }
        self.begin_edit();
        self.edit_collection(|c| c.set("Schema", Object::Dictionary(schema)))?;
        self.commit()
    }

    /// Set the value of a custom column for a file.
    pub fn set_portfolio_item_value(&mut self, key: &[u8], field: &str, value: &str) -> Result<()> {
        let p = self
            .portfolio()
            .ok_or_else(|| Error::Invalid("the document is not a portfolio".into()))?;
        let col = p
            .schema
            .iter()
            .find(|f| f.key == field)
            .ok_or_else(|| Error::Invalid(format!("no column {field}")))?;
        let value_obj = match col.kind.as_str() {
            "N" => match value.trim().parse::<f64>() {
                Ok(v) if v.fract() == 0.0 => Object::Integer(v as i64),
                Ok(v) => Object::Real(v as f32),
                Err(_) if value.trim().is_empty() => Object::Null,
                Err(_) => return Err(Error::Invalid(format!("{value} is not a number"))),
            },
            "D" => {
                let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() < 8 && !value.trim().is_empty() {
                    return Err(Error::Invalid(format!("{value} is not a date (YYYY-MM-DD)")));
                }
                Object::string_literal(format!("D:{digits}"))
            }
            "S" => lopdf::text_string(value),
            _ => return Err(Error::Invalid(format!("column {field} is not editable data"))),
        };
        let spec_ref = self
            .embedded_pairs()
            .into_iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.as_reference().ok())
            .ok_or_else(|| Error::Invalid("file not found".into()))?;
        self.begin_edit();
        let ci_obj = self.inner.get_dictionary(spec_ref)?.get(b"CI").ok().cloned();
        let ci_id = match ci_obj {
            Some(Object::Reference(r)) => r,
            other => {
                let d = match other {
                    Some(Object::Dictionary(d)) => d,
                    _ => dictionary! { "Type" => "CollectionItem" },
                };
                let id = self.inner.add_object(d);
                self.inner
                    .get_dictionary_mut(spec_ref)?
                    .set("CI", Object::Reference(id));
                id
            }
        };
        let ci = self.inner.get_dictionary_mut(ci_id)?;
        if matches!(value_obj, Object::Null) || value.trim().is_empty() {
            ci.remove(field.as_bytes());
        } else {
            ci.set(field.as_bytes().to_vec(), value_obj);
        }
        self.commit()
    }
}

/// Media type of a file, from its extension.
pub fn mime_of(name: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(name)
        .extension()?
        .to_string_lossy()
        .to_lowercase();
    Some(match ext.as_str() {
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "xml" => "text/xml",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "zip" => "application/zip",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => return None,
    })
}
