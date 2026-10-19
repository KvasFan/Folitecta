//! FDF: comments written to and read from the PDF-syntax format.

use std::collections::{BTreeMap, HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId, dictionary};

use super::*;
use crate::util::deref;

impl Document {
    /// Export comments as FDF, with their appearances: every comment, or only
    /// the listed `(page, index)` pairs.
    pub fn export_comments_fdf(&self, only: Option<&[(usize, usize)]>) -> Result<Vec<u8>> {
        let items = self.exchange_comments(only);
        let src = &self.inner;
        let mut fdf = lopdf::Document::with_version("1.2");
        let mut map: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();
        let exported: HashSet<ObjectId> = items.iter().map(|(_, id, _)| *id).collect();
        let mut popups = HashMap::new();
        for (_, id, d) in &items {
            map.insert(*id, fdf.new_object_id());
            if let Ok(p) = d.get(b"Popup").and_then(|p| p.as_reference()) {
                if src.get_dictionary(p).is_ok() {
                    map.insert(p, fdf.new_object_id());
                    popups.insert(*id, p);
                }
            }
        }
        let queue: Vec<ObjectId> = items.iter().flat_map(|(_, _, d)| content_refs(d)).collect();
        copy_objects(src, &mut fdf, queue, &mut map);
        let mut annots = Vec::new();
        for (page, id, d) in &items {
            let mut a = d.clone();
            a.remove(b"P");
            a.remove(b"StructParent");
            a.set("Page", *page as i64);
            if let Ok(t) = a.get(b"IRT").and_then(|t| t.as_reference()) {
                if !exported.contains(&t) {
                    a.remove(b"IRT");
                    a.remove(b"RT");
                }
            }
            let mut obj = Object::Dictionary(a);
            remap_strict(&mut obj, &map);
            fdf.objects.insert(map[id], obj);
            annots.push(Object::Reference(map[id]));
            if let Some(p) = popups.get(id) {
                let mut pd = src.get_dictionary(*p)?.clone();
                pd.remove(b"P");
                pd.set("Page", *page as i64);
                pd.set("Parent", Object::Reference(*id));
                let mut pobj = Object::Dictionary(pd);
                remap_strict(&mut pobj, &map);
                fdf.objects.insert(map[p], pobj);
                annots.push(Object::Reference(map[p]));
            }
        }
        let mut fdf_dict = dictionary! { "Annots" => annots };
        if let Some(file) = self.path().and_then(|p| p.file_name()) {
            fdf_dict.set("F", Object::string_literal(file.to_string_lossy().to_string()));
        }
        if let Ok(ids @ Object::Array(_)) = src.trailer.get(b"ID") {
            fdf_dict.set("ID", ids.clone());
        }
        let root = fdf.add_object(dictionary! { "FDF" => fdf_dict });
        fdf.trailer.set("Root", root);
        let mut out = Vec::new();
        fdf.save_to(&mut out)?;
        // FDF shares the PDF file structure; only the header differs.
        if out.starts_with(b"%PDF-") {
            out[1] = b'F';
        }
        Ok(out)
    }

    /// Import the comments of an FDF file (one undo step).
    pub fn import_comments_fdf(&mut self, data: &[u8]) -> Result<usize> {
        self.fdf_import(data, false).map(|(n, _)| n)
    }

    pub(super) fn fdf_import(
        &mut self,
        data: &[u8],
        skip: bool,
    ) -> Result<(usize, Vec<super::UndrawableComment>)> {
        let pos = data
            .windows(5)
            .position(|w| w == b"%FDF-")
            .ok_or_else(|| Error::Invalid("the file is not an FDF file".into()))?;
        let mut bytes = data.to_vec();
        bytes[pos + 1] = b'P';
        let src = lopdf::Document::load_mem(&bytes)
            .map_err(|e| Error::Invalid(format!("cannot read the FDF file: {e}")))?;
        let annots: Vec<Object> = src
            .trailer
            .get(b"Root")
            .ok()
            .map(|r| deref(&src, r))
            .and_then(|r| r.as_dict().ok())
            .and_then(|r| sub_dict(&src, r, b"FDF"))
            .and_then(|f| f.get(b"Annots").ok())
            .map(|a| deref(&src, a))
            .and_then(|a| a.as_array().ok())
            .cloned()
            .unwrap_or_default();
        let pages = self.page_count();
        let entries: Vec<(usize, ObjectId, Dictionary)> = annots
            .iter()
            .enumerate()
            .filter_map(|(i, a)| {
                let (id, d) = match a {
                    Object::Reference(r) => (*r, src.get_dictionary(*r).ok()?.clone()),
                    Object::Dictionary(d) => ((u32::MAX - i as u32, 0), d.clone()),
                    _ => return None,
                };
                let page = d
                    .get(b"Page")
                    .ok()
                    .and_then(|p| p.as_i64().ok())
                    .unwrap_or(0)
                    .max(0) as usize;
                (page < pages).then_some((page, id, d))
            })
            .collect();
        self.import_from_source(&src, entries, skip)
    }
}
