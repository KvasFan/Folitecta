//! Pages taken from another document bring what belongs to them: their form
//! fields join the form, their named destinations, bookmarks and layers come
//! along, and so do their tags when this document is tagged. A field or
//! destination name already used here gets a suffix, and the renames are
//! listed; what the source tied to its pages not taken stays behind and is
//! counted.

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

use lopdf::{Dictionary, Object, ObjectId};

use crate::delivery::LeftOut;
use crate::document::Document;
use crate::error::{Error, Result};
use crate::outline::OutlineItem;
use crate::util::{deref, text_of};

/// What pages taken from another document brought along, renamed or left.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Arrival {
    /// Form fields renamed because this document has a field of that name
    /// (old name, new name).
    pub renamed_fields: Vec<(String, String)>,
    /// Named destinations renamed the same way; the links and bookmarks
    /// that use them follow.
    pub renamed_destinations: Vec<(String, String)>,
    /// What belonged to, or led to, the pages of the source not taken.
    pub left_out: LeftOut,
    /// The elements of the source's tags not brought: this document has no
    /// tags.
    pub tags_left: usize,
}

impl Arrival {
    /// Nothing renamed or left behind.
    pub fn is_empty(&self) -> bool {
        self.renamed_fields.is_empty()
            && self.renamed_destinations.is_empty()
            && self.left_out.is_empty()
            && self.tags_left == 0
    }
}

/// `name` with the first suffix (`_2`, `_3`, …) that `taken` does not hold.
fn free_name(name: &str, taken: &HashSet<String>) -> String {
    (2..)
        .map(|i| format!("{name}_{i}"))
        .find(|n| !taken.contains(n))
        .unwrap_or_else(|| name.to_string())
}

impl Document {
    /// The names of the fields at the top of the form.
    fn top_field_names(&self) -> Vec<(ObjectId, String)> {
        let doc = &self.inner;
        doc.catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .and_then(|f| deref(doc, f).as_dict().ok())
            .and_then(|f| f.get(b"Fields").ok())
            .and_then(|f| deref(doc, f).as_array().ok())
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|f| {
                        let id = f.as_reference().ok()?;
                        let name = doc
                            .get_dictionary(id)
                            .ok()?
                            .get(b"T")
                            .ok()
                            .and_then(|t| text_of(doc, t))
                            .unwrap_or_default();
                        Some((id, name))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Insert pages of `other` before index `at` (`pages`: zero-based pages
    /// of `other` in the order wanted, every page when `None`), with what
    /// belongs to them (see the module). With `title` the bookmarks of the
    /// source go under a new bookmark of that title, leading to the first
    /// page inserted. One undo step. Pages of a protected document are
    /// refused by a document that is not protected.
    pub fn insert_pages_from(
        &mut self,
        at: usize,
        other: &Document,
        pages: Option<&[usize]>,
        title: Option<&str>,
    ) -> Result<Arrival> {
        self.check_import_from(other)?;
        if at > self.page_count() {
            return Err(Error::InvalidPage(at));
        }
        let mut src = other.derived()?;
        src.changing();
        let mut arrival = Arrival::default();
        if let Some(sel) = pages {
            let n = src.page_count();
            if let Some(&bad) = sel.iter().find(|&&p| p >= n) {
                return Err(Error::InvalidPage(bad));
            }
            if sel.is_empty() {
                return Ok(arrival);
            }
            let mut kept = sel.to_vec();
            kept.sort_unstable();
            kept.dedup();
            if kept.len() != sel.len() {
                return Err(Error::Invalid("pages must be listed once".into()));
            }
            // What the source ties to its other pages stays there.
            arrival.left_out = src.keep_only_pages(&kept, &AtomicBool::new(false))?;
            let ids = src.page_ids();
            let order: Vec<ObjectId> = sel
                .iter()
                .map(|p| ids[kept.binary_search(p).unwrap_or(0)])
                .collect();
            src.write_page_tree(&order)?;
        }
        self.begin_edit();
        if let Err(e) = self.take_pages(at, &mut src, title, &mut arrival) {
            self.abandon_edit();
            return Err(e);
        }
        self.commit()?;
        Ok(arrival)
    }

    /// What [`insert_pages_from`](Self::insert_pages_from) would rename and
    /// leave behind, nothing changed.
    pub fn inserting_pages_brings(&self, other: &Document, pages: Option<&[usize]>) -> Result<Arrival> {
        let mut copy = self.derived()?;
        let at = copy.page_count();
        copy.insert_pages_from(at, other, pages, None)
    }

    /// Move the pages of `src` (a work copy, every page taken) with their
    /// fields, destinations, bookmarks, layers and tags before `at`.
    fn take_pages(
        &mut self,
        at: usize,
        src: &mut Document,
        title: Option<&str>,
        arrival: &mut Arrival,
    ) -> Result<()> {
        // Names already used here get a suffix in the source first, so its
        // links and bookmarks follow the new names.
        let mut dest_names: HashSet<String> = self
            .named_dest_map()
            .into_iter()
            .map(|(k, _)| String::from_utf8_lossy(&k).to_string())
            .collect();
        for (k, _) in src.named_dest_map() {
            let name = String::from_utf8_lossy(&k).to_string();
            if dest_names.contains(&name) {
                let new = free_name(&name, &dest_names);
                src.rename_named_destination_in(&k, new.as_bytes())?;
                arrival.renamed_destinations.push((name, new.clone()));
                dest_names.insert(new);
            } else {
                dest_names.insert(name);
            }
        }
        let mut field_names: HashSet<String> = self.top_field_names().into_iter().map(|(_, n)| n).collect();
        for (id, name) in &src.top_field_names() {
            if name.is_empty() {
                continue;
            }
            if field_names.contains(name) {
                let new = free_name(name, &field_names);
                src.inner
                    .get_dictionary_mut(*id)?
                    .set("T", lopdf::text_string(&new));
                arrival.renamed_fields.push((name.clone(), new.clone()));
                field_names.insert(new);
            } else {
                field_names.insert(name.clone());
            }
        }

        let src_tag_count = src.structure().map_or(0, |t| {
            let mut n = 0;
            t.walk(&mut |e, _| n += usize::from(!e.is_root()));
            n
        });

        // The objects of the source, numbered after ours.
        src.inner.renumber_objects_with(self.inner.max_id + 1);
        src.changing();
        let src_fields = src.top_field_names();
        let src_pages = src.page_ids();
        let src_outline = src.outline();
        let src_tree = src.names_tree_entries();
        let src_catalog = src.inner.catalog()?.clone();
        let src_dests_dict = src_catalog
            .get(b"Dests")
            .ok()
            .and_then(|d| deref(&src.inner, d).as_dict().ok())
            .cloned()
            .unwrap_or_default();
        let src_form = src_catalog
            .get(b"AcroForm")
            .ok()
            .and_then(|f| deref(&src.inner, f).as_dict().ok())
            .cloned();
        let src_layers = src_catalog
            .get(b"OCProperties")
            .ok()
            .and_then(|o| deref(&src.inner, o).as_dict().ok())
            .cloned();
        let src_tags = src_catalog
            .get(b"StructTreeRoot")
            .ok()
            .and_then(|o| deref(&src.inner, o).as_dict().ok())
            .cloned();
        let our_outline = self.outline();
        let max_id = src.inner.max_id;
        for (id, obj) in std::mem::take(&mut src.inner.objects) {
            self.inner.objects.insert(id, obj);
        }
        self.inner.max_id = self.inner.max_id.max(max_id);

        // The pages.
        self.inherit_attributes(&src_pages);
        let mut ids = self.page_ids();
        let tail = ids.split_off(at);
        ids.extend(src_pages.iter().copied());
        ids.extend(tail);
        self.write_page_tree(&ids)?;

        // Form fields, with the fonts their appearances name.
        if let Some(form) = &src_form {
            let fields: Vec<Object> = src_fields.iter().map(|(id, _)| Object::Reference(*id)).collect();
            if !fields.is_empty() {
                let ours = self.ensure_acroform()?;
                let their_fonts = form
                    .get(b"DR")
                    .ok()
                    .and_then(|d| deref(&self.inner, d).as_dict().ok())
                    .and_then(|d| d.get(b"Font").ok())
                    .and_then(|f| deref(&self.inner, f).as_dict().ok())
                    .cloned()
                    .unwrap_or_default();
                let f = self.inner.get_dictionary_mut(ours)?;
                let mut list = f
                    .get(b"Fields")
                    .ok()
                    .and_then(|l| l.as_array().ok())
                    .cloned()
                    .unwrap_or_default();
                list.extend(fields);
                f.set("Fields", list);
                if let Ok(Object::Dictionary(dr)) = f.get_mut(b"DR") {
                    if let Ok(Object::Dictionary(fonts)) = dr.get_mut(b"Font") {
                        for (k, v) in their_fonts.iter() {
                            if !fonts.has(k) {
                                fonts.set(k.clone(), v.clone());
                            }
                        }
                    }
                }
            }
        }

        // Named destinations, where the source kept them: those named by
        // strings in the name tree, those named by names in `/Dests`.
        if !src_tree.is_empty() {
            let mut entries = self.names_tree_entries();
            entries.extend(src_tree);
            self.write_names_tree(entries)?;
        }
        if !src_dests_dict.is_empty() {
            if self.dests_dict_mut().is_none() {
                self.inner.catalog_mut()?.set("Dests", Dictionary::new());
            }
            if let Some(d) = self.dests_dict_mut() {
                for (k, v) in src_dests_dict.iter() {
                    d.set(k.clone(), v.clone());
                }
            }
        }

        // Bookmarks: those of the source where its pages went.
        if !src_outline.is_empty() || title.is_some() {
            let first_new = at;
            let mut incoming: Vec<OutlineItem> = src_outline;
            if let Some(title) = title {
                let mut part = self.new_bookmark_item(title, first_new, None)?;
                part.children = std::mem::take(&mut incoming);
                incoming = vec![part];
            }
            let mut items = our_outline;
            // Before the first bookmark of ours that leads after the place.
            let place = items
                .iter()
                .position(|i| i.page.is_some_and(|p| p >= at))
                .unwrap_or(items.len());
            items.splice(place..place, incoming);
            self.write_outline(&items)?;
        }

        // Layers: the groups of the source join ours, shown as they were.
        if let Some(theirs) = src_layers {
            self.adopt_layers(&theirs)?;
        }

        // Tags, when this document has them.
        if let Some(theirs) = src_tags {
            if self.has_structure() {
                self.adopt_structure(&theirs)?;
                self.rebuild_parent_tree()?;
            } else {
                arrival.tags_left = src_tag_count;
            }
        }
        self.inner.prune_objects();
        Ok(())
    }

    /// Add the optional content groups of `theirs` (another document's
    /// `/OCProperties`, its objects already here) to ours, with their order
    /// and the groups hidden at first.
    fn adopt_layers(&mut self, theirs: &Dictionary) -> Result<()> {
        let arr = |d: &Dictionary, key: &[u8]| -> Vec<Object> {
            d.get(key)
                .ok()
                .and_then(|a| deref(&self.inner, a).as_array().ok())
                .cloned()
                .unwrap_or_default()
        };
        let their_d = theirs
            .get(b"D")
            .ok()
            .and_then(|d| deref(&self.inner, d).as_dict().ok())
            .cloned()
            .unwrap_or_default();
        let (ocgs, order, off) = (
            arr(theirs, b"OCGs"),
            arr(&their_d, b"Order"),
            arr(&their_d, b"OFF"),
        );
        if ocgs.is_empty() {
            return Ok(());
        }
        let mut ours = match self.inner.catalog()?.get(b"OCProperties").ok().cloned() {
            Some(o) => deref(&self.inner, &o).as_dict().cloned().unwrap_or_default(),
            None => Dictionary::new(),
        };
        let mut our_d = ours
            .get(b"D")
            .ok()
            .and_then(|d| deref(&self.inner, d).as_dict().ok())
            .cloned()
            .unwrap_or_default();
        let mut list = arr(&ours, b"OCGs");
        list.extend(ocgs.iter().cloned());
        ours.set("OCGs", list);
        let mut our_order = arr(&our_d, b"Order");
        our_order.extend(if order.is_empty() { ocgs } else { order });
        our_d.set("Order", our_order);
        if !off.is_empty() {
            let mut our_off = arr(&our_d, b"OFF");
            our_off.extend(off);
            our_d.set("OFF", our_off);
        }
        ours.set("D", our_d);
        match self.inner.catalog()?.get(b"OCProperties").ok().cloned() {
            Some(Object::Reference(id)) => {
                self.inner.objects.insert(id, Object::Dictionary(ours));
            }
            _ => self
                .inner
                .catalog_mut()?
                .set("OCProperties", Object::Dictionary(ours)),
        }
        Ok(())
    }
}
