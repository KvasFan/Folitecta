//! Editing the structure: the root, elements, their properties, artifacts, the role map and class map, and the parent tree.

use super::*;

impl Document {
    // ------------------------------------------------------------------
    // Editing helpers
    // ------------------------------------------------------------------

    pub(super) fn catalog_id(&self) -> Result<ObjectId> {
        Ok(self.inner.trailer.get(b"Root")?.as_reference()?)
    }

    /// Id of the structure root, made indirect when it was written in the catalog.
    pub(super) fn struct_root_id(&mut self) -> Option<ObjectId> {
        let cat = self.catalog_id().ok()?;
        let obj = self
            .inner
            .get_dictionary(cat)
            .ok()?
            .get(b"StructTreeRoot")
            .ok()?
            .clone();
        match obj {
            Object::Reference(r) => Some(r),
            Object::Dictionary(d) => {
                let id = self.inner.add_object(Object::Dictionary(d));
                self.inner
                    .get_dictionary_mut(cat)
                    .ok()?
                    .set("StructTreeRoot", Object::Reference(id));
                Some(id)
            }
            _ => None,
        }
    }

    pub(super) fn set_marked(&mut self, on: bool) -> Result<()> {
        let cat = self.catalog_id()?;
        let c = self.inner.get_dictionary_mut(cat)?;
        let mut mi = match c.get(b"MarkInfo") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        mi.set("Marked", Object::Boolean(on));
        c.set("MarkInfo", Object::Dictionary(mi));
        Ok(())
    }

    /// The structure root, created when missing (the document becomes tagged).
    pub(super) fn ensure_struct_root(&mut self) -> Result<ObjectId> {
        if let Some(id) = self.struct_root_id() {
            return Ok(id);
        }
        let pt = self
            .inner
            .add_object(dictionary! { "Nums" => Vec::<Object>::new() });
        let root = self.inner.add_object(dictionary! {
            "Type" => "StructTreeRoot",
            "K" => Vec::<Object>::new(),
            "ParentTree" => pt,
            "ParentTreeNextKey" => 0,
        });
        let cat = self.catalog_id()?;
        self.inner
            .get_dictionary_mut(cat)?
            .set("StructTreeRoot", Object::Reference(root));
        self.set_marked(true)?;
        Ok(root)
    }

    pub(super) fn kid_list(&self, parent: ObjectId) -> Vec<Object> {
        self.inner
            .get_dictionary(parent)
            .ok()
            .and_then(|d| d.get(b"K").ok())
            .map(|k| kid_objects(&self.inner, k))
            .unwrap_or_default()
    }

    pub(super) fn set_kid_list(&mut self, parent: ObjectId, kids: Vec<Object>) -> Result<()> {
        self.inner
            .get_dictionary_mut(parent)?
            .set("K", Object::Array(kids));
        Ok(())
    }

    pub(super) fn insert_kid(&mut self, parent: ObjectId, index: Option<usize>, kid: Object) -> Result<()> {
        let mut kids = self.kid_list(parent);
        let at = index.unwrap_or(kids.len()).min(kids.len());
        kids.insert(at, kid);
        self.set_kid_list(parent, kids)
    }

    pub(super) fn remove_kid(&mut self, parent: ObjectId, id: ObjectId) -> Result<Option<usize>> {
        let mut kids = self.kid_list(parent);
        let pos = kids.iter().position(|k| k.as_reference().ok() == Some(id));
        if let Some(p) = pos {
            kids.remove(p);
            self.set_kid_list(parent, kids)?;
        }
        Ok(pos)
    }

    pub(super) fn new_struct_elem(&mut self, parent: ObjectId, tag: &str, page: Option<usize>) -> ObjectId {
        let mut d = dictionary! {
            "Type" => "StructElem",
            "S" => Object::Name(tag.as_bytes().to_vec()),
            "P" => parent,
            "K" => Vec::<Object>::new(),
        };
        if let Some(pid) = page.and_then(|p| self.page_ids().get(p).copied()) {
            d.set("Pg", Object::Reference(pid));
        }
        self.inner.add_object(d)
    }

    pub(super) fn own_page(&self, id: ObjectId) -> Option<ObjectId> {
        self.inner
            .get_dictionary(id)
            .ok()?
            .get(b"Pg")
            .ok()?
            .as_reference()
            .ok()
    }

    /// A kid object for marked content of a page, inside the element `elem`.
    pub(super) fn content_kid(&self, elem: ObjectId, page: usize, mcid: i64) -> Object {
        let pid = self.page_ids().get(page).copied();
        if pid.is_some() && self.own_page(elem) == pid {
            Object::Integer(mcid)
        } else {
            let mut d = dictionary! { "Type" => "MCR", "MCID" => mcid };
            if let Some(pid) = pid {
                d.set("Pg", Object::Reference(pid));
            }
            Object::Dictionary(d)
        }
    }

    pub(super) fn annot_kid(&self, page: usize, index: usize) -> Option<Object> {
        let pid = self.page_ids().get(page).copied()?;
        let pd = self.inner.get_dictionary(pid).ok()?;
        let aid = crate::annots::annot_refs(&self.inner, pd).get(index)?.0?;
        Some(Object::Dictionary(dictionary! {
            "Type" => "OBJR",
            "Obj" => aid,
            "Pg" => pid,
        }))
    }

    /// Keep the kids of an element for which `keep` is true.
    pub(super) fn retain_kids(
        &mut self,
        tree: &StructElem,
        id: ObjectId,
        keep: &dyn Fn(&StructKid) -> bool,
    ) -> Result<()> {
        let Some(e) = tree.find(id) else { return Ok(()) };
        let raw = self.kid_list(id);
        if raw.len() != e.kids.len() {
            return Ok(());
        }
        let kept: Vec<Object> = raw
            .into_iter()
            .zip(e.kids.iter())
            .filter(|(_, k)| keep(k))
            .map(|(o, _)| o)
            .collect();
        if kept.len() != e.kids.len() {
            self.set_kid_list(id, kept)?;
        }
        Ok(())
    }

    /// Take marked content and annotations away from the elements that own them.
    pub(super) fn detach_content(
        &mut self,
        mcids: &HashSet<(usize, i64)>,
        annots: &HashSet<(usize, usize)>,
    ) -> Result<()> {
        let Some(tree) = self.structure() else {
            return Ok(());
        };
        let mut owners = Vec::new();
        tree.walk(&mut |e, _| {
            if e.kids.iter().any(|k| match k {
                StructKid::Content { page, mcid } => mcids.contains(&(*page, *mcid)),
                StructKid::Annot { page, index, .. } => annots.contains(&(*page, *index)),
                _ => false,
            }) {
                owners.push(e.id);
            }
        });
        for id in owners {
            self.retain_kids(&tree, id, &|k| match k {
                StructKid::Content { page, mcid } => !mcids.contains(&(*page, *mcid)),
                StructKid::Annot { page, index, .. } => !annots.contains(&(*page, *index)),
                _ => true,
            })?;
        }
        Ok(())
    }

    /// Rebuild the parent tree from the structure tree: the elements that own
    /// each marked content sequence and each tagged annotation.
    pub(crate) fn rebuild_parent_tree(&mut self) -> Result<()> {
        let Some(root_id) = self.struct_root_id() else {
            return Ok(());
        };
        let Some(tree) = self.structure() else {
            return Ok(());
        };
        let mut mcids: BTreeMap<usize, BTreeMap<i64, ObjectId>> = BTreeMap::new();
        let mut annots: HashMap<(usize, usize), ObjectId> = HashMap::new();
        tree.walk(&mut |e, _| {
            if e.is_root() {
                return;
            }
            for k in &e.kids {
                match k {
                    StructKid::Content { page, mcid } if (0..=MAX_MCID).contains(mcid) => {
                        mcids.entry(*page).or_default().insert(*mcid, e.id);
                    }
                    StructKid::Annot { page, index, .. } => {
                        annots.insert((*page, *index), e.id);
                    }
                    _ => {}
                }
            }
        });
        let page_ids = self.page_ids();
        let mut nums: Vec<Object> = Vec::new();
        let mut key: i64 = 0;
        for (p, pid) in page_ids.iter().enumerate() {
            let page = self.inner.get_dictionary_mut(*pid)?;
            match mcids.get(&p) {
                Some(m) if !m.is_empty() => {
                    let max = m.keys().last().copied().unwrap_or(0);
                    let arr: Vec<Object> = (0..=max)
                        .map(|i| m.get(&i).map(|id| Object::Reference(*id)).unwrap_or(Object::Null))
                        .collect();
                    page.set("StructParents", key);
                    nums.push(Object::Integer(key));
                    nums.push(Object::Array(arr));
                    key += 1;
                }
                _ => {
                    page.remove(b"StructParents");
                }
            }
        }
        for (p, pid) in page_ids.iter().enumerate() {
            let refs = match self.inner.get_dictionary(*pid) {
                Ok(pd) => crate::annots::annot_refs(&self.inner, pd),
                Err(_) => continue,
            };
            for (i, (aid, _)) in refs.into_iter().enumerate() {
                let Some(aid) = aid else { continue };
                let Ok(d) = self.inner.get_dictionary_mut(aid) else {
                    continue;
                };
                match annots.get(&(p, i)) {
                    Some(eid) => {
                        d.set("StructParent", key);
                        nums.push(Object::Integer(key));
                        nums.push(Object::Reference(*eid));
                        key += 1;
                    }
                    None => {
                        d.remove(b"StructParent");
                    }
                }
            }
        }
        let tree_dict = Object::Dictionary(dictionary! { "Nums" => nums });
        let old = self
            .inner
            .get_dictionary(root_id)?
            .get(b"ParentTree")
            .ok()
            .and_then(|o| o.as_reference().ok());
        let pt = match old {
            Some(id) => {
                self.inner.objects.insert(id, tree_dict);
                id
            }
            None => self.inner.add_object(tree_dict),
        };
        let root = self.inner.get_dictionary_mut(root_id)?;
        root.set("ParentTree", Object::Reference(pt));
        root.set("ParentTreeNextKey", key);
        Ok(())
    }

    pub(super) fn check_elem(&self, id: ObjectId) -> Result<StructElem> {
        self.structure()
            .and_then(|t| t.find(id).cloned())
            .ok_or_else(|| Error::Invalid("the tag is no longer in the structure".into()))
    }

    // ------------------------------------------------------------------
    // Editing
    // ------------------------------------------------------------------

    /// Mark the document as tagged or not.
    pub fn set_tagged(&mut self, on: bool) -> Result<()> {
        self.begin_edit();
        self.set_marked(on)?;
        self.commit()
    }

    /// Create an empty structure tree.
    pub fn create_struct_root(&mut self) -> Result<()> {
        if self.has_structure() {
            return Err(Error::Invalid("the document already has a structure root".into()));
        }
        self.begin_edit();
        self.ensure_struct_root()?;
        self.commit()
    }

    /// A new element inside `parent` (the root when None) at `index` among its
    /// kids (the end when None).
    pub fn add_struct_elem(
        &mut self,
        parent: Option<ObjectId>,
        index: Option<usize>,
        tag: &str,
        title: &str,
    ) -> Result<ObjectId> {
        let tag = tag.trim();
        if tag.is_empty() || tag.contains(char::is_whitespace) {
            return Err(Error::Invalid("a tag is one word".into()));
        }
        self.begin_edit();
        let parent = match parent {
            Some(p) => p,
            None => self.ensure_struct_root()?,
        };
        let id = self.new_struct_elem(parent, tag, None);
        if !title.trim().is_empty() {
            self.inner
                .get_dictionary_mut(id)?
                .set("T", lopdf::text_string(title.trim()));
        }
        self.insert_kid(parent, index, Object::Reference(id))?;
        self.commit()?;
        Ok(id)
    }

    /// Remove an element (its content stays on the page, untagged).
    pub fn delete_struct_elem(&mut self, id: ObjectId) -> Result<()> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let parent = tree
            .parent_of(id)
            .map(|p| p.id)
            .ok_or_else(|| Error::Invalid("the tag is no longer in the structure".into()))?;
        self.begin_edit();
        let parent = if parent == (0, 0) {
            self.struct_root_id().unwrap_or(parent)
        } else {
            parent
        };
        self.remove_kid(parent, id)?;
        self.rebuild_parent_tree()?;
        self.commit()
    }

    /// Move an element into `new_parent` (the root when None) at `index`
    /// among its kids as they are before the move.
    pub fn move_struct_elem(
        &mut self,
        id: ObjectId,
        new_parent: Option<ObjectId>,
        index: usize,
    ) -> Result<()> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let elem = tree
            .find(id)
            .ok_or_else(|| Error::Invalid("the tag is no longer in the structure".into()))?;
        if let Some(np) = new_parent {
            if elem.find(np).is_some() {
                return Err(Error::Invalid("a tag cannot go inside itself".into()));
            }
        }
        let old_parent = tree.parent_of(id).map(|p| p.id);
        let inherited_page = elem.page;
        self.begin_edit();
        // Content written as plain numbers takes its page from the element
        // or from one around it: keep it when the element moves.
        if self.own_page(id).is_none() {
            if let Some(pid) = inherited_page.and_then(|p| self.page_ids().get(p).copied()) {
                self.inner
                    .get_dictionary_mut(id)?
                    .set("Pg", Object::Reference(pid));
            }
        }
        let root = self
            .struct_root_id()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let old_parent = match old_parent {
            Some(p) if p != (0, 0) => p,
            _ => root,
        };
        let new_parent = new_parent.unwrap_or(root);
        let old_pos = self.remove_kid(old_parent, id)?;
        let mut at = index;
        if old_parent == new_parent {
            if let Some(op) = old_pos {
                if op < at {
                    at -= 1;
                }
            }
        }
        self.insert_kid(new_parent, Some(at), Object::Reference(id))?;
        self.inner
            .get_dictionary_mut(id)?
            .set("P", Object::Reference(new_parent));
        self.rebuild_parent_tree()?;
        self.commit()
    }

    /// Change the tag, title, alternate text, actual text and language of an element.
    pub fn set_struct_props(&mut self, id: ObjectId, props: &StructProps) -> Result<()> {
        self.check_elem(id)?;
        let tag = props.tag.trim();
        if tag.is_empty() || tag.contains(char::is_whitespace) {
            return Err(Error::Invalid("a tag is one word".into()));
        }
        self.begin_edit();
        let d = self.inner.get_dictionary_mut(id)?;
        d.set("S", Object::Name(tag.as_bytes().to_vec()));
        for (key, value) in [
            ("T", &props.title),
            ("Alt", &props.alt),
            ("ActualText", &props.actual_text),
            ("Lang", &props.lang),
        ] {
            if value.trim().is_empty() {
                d.remove(key.as_bytes());
            } else {
                d.set(key, lopdf::text_string(value.trim()));
            }
        }
        self.commit()
    }

    /// Change only the tag of an element.
    pub fn set_struct_tag(&mut self, id: ObjectId, tag: &str) -> Result<()> {
        let e = self.check_elem(id)?;
        self.set_struct_props(
            id,
            &StructProps {
                tag: tag.to_string(),
                title: e.title,
                alt: e.alt,
                actual_text: e.actual_text,
                lang: e.lang,
            },
        )
    }

    /// Remove the elements without content; returns how many went away.
    pub fn delete_empty_struct(&mut self) -> Result<usize> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let mut empty = Vec::new();
        fn collect(e: &StructElem, out: &mut Vec<(ObjectId, ObjectId)>) {
            for c in e.children() {
                if c.is_empty() {
                    out.push((e.id, c.id));
                } else {
                    collect(c, out);
                }
            }
        }
        collect(&tree, &mut empty);
        if empty.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let root = self.struct_root_id().unwrap_or((0, 0));
        for (parent, id) in &empty {
            let parent = if *parent == (0, 0) { root } else { *parent };
            self.remove_kid(parent, *id)?;
        }
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(empty.len())
    }

    /// Turn an element and its content into artifacts (decorative content).
    pub fn struct_to_artifact(&mut self, id: ObjectId) -> Result<()> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let elem = tree
            .find(id)
            .ok_or_else(|| Error::Invalid("the tag is no longer in the structure".into()))?;
        let parent = tree.parent_of(id).map(|p| p.id);
        let mut by_page: BTreeMap<usize, HashSet<i64>> = BTreeMap::new();
        for (p, m) in elem.content() {
            by_page.entry(p).or_default().insert(m);
        }
        self.begin_edit();
        let page_ids = self.page_ids();
        for (p, set) in by_page {
            let Some(&pid) = page_ids.get(p) else { continue };
            let ps = PageStreams::read(&self.inner, pid);
            if let Some(out) = rewrite_mcids(&ps.data, &|m| {
                if set.contains(&m) {
                    McChange::Artifact
                } else {
                    McChange::Keep
                }
            }) {
                write_pieces(&mut self.inner, pid, &ps, out)?;
            }
        }
        let root = self.struct_root_id().unwrap_or((0, 0));
        let parent = match parent {
            Some(p) if p != (0, 0) => p,
            _ => root,
        };
        self.remove_kid(parent, id)?;
        self.rebuild_parent_tree()?;
        self.commit()
    }

    /// Strip the structure: tags, parent tree and the markers of tagged
    /// content (artifacts stay). Does not commit.
    pub(super) fn clear_structure_inner(&mut self) -> Result<()> {
        for pid in self.page_ids() {
            let ps = PageStreams::read(&self.inner, pid);
            if let Some(out) = rewrite_mcids(&ps.data, &|_| McChange::Strip) {
                write_pieces(&mut self.inner, pid, &ps, out)?;
            }
            let refs = match self.inner.get_dictionary(pid) {
                Ok(pd) => crate::annots::annot_refs(&self.inner, pd),
                Err(_) => Vec::new(),
            };
            for (aid, _) in refs {
                if let Some(d) = aid.and_then(|a| self.inner.get_dictionary_mut(a).ok()) {
                    d.remove(b"StructParent");
                }
            }
            if let Ok(pd) = self.inner.get_dictionary_mut(pid) {
                pd.remove(b"StructParents");
            }
        }
        let cat = self.catalog_id()?;
        let c = self.inner.get_dictionary_mut(cat)?;
        c.remove(b"StructTreeRoot");
        c.remove(b"MarkInfo");
        self.inner.prune_objects();
        Ok(())
    }

    /// Remove the whole structure of the document.
    pub fn clear_structure(&mut self) -> Result<()> {
        self.begin_edit();
        self.clear_structure_inner()?;
        self.commit()
    }

    /// Replace the role map.
    pub fn set_role_map(&mut self, map: &[(String, String)]) -> Result<()> {
        self.begin_edit();
        let root = self.ensure_struct_root()?;
        let mut d = Dictionary::new();
        for (k, v) in map {
            let (k, v) = (k.trim(), v.trim());
            if !k.is_empty() && !v.is_empty() {
                d.set(k.as_bytes().to_vec(), Object::Name(v.as_bytes().to_vec()));
            }
        }
        let r = self.inner.get_dictionary_mut(root)?;
        if d.is_empty() {
            r.remove(b"RoleMap");
        } else {
            r.set("RoleMap", Object::Dictionary(d));
        }
        self.commit()
    }

    /// Replace the class map (attributes as `Key=Value` text, `;` between dictionaries).
    pub fn set_class_map(&mut self, map: &[(String, String)]) -> Result<()> {
        self.begin_edit();
        let root = self.ensure_struct_root()?;
        let mut d = Dictionary::new();
        for (k, v) in map {
            if !k.trim().is_empty() {
                d.set(k.trim().as_bytes().to_vec(), text_to_attrs(v));
            }
        }
        let r = self.inner.get_dictionary_mut(root)?;
        if d.is_empty() {
            r.remove(b"ClassMap");
        } else {
            r.set("ClassMap", Object::Dictionary(d));
        }
        self.commit()
    }

    /// Write the standard types of the role map on the elements; returns how
    /// many elements changed.
    pub fn apply_role_map(&mut self) -> Result<usize> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let mut changes = Vec::new();
        tree.walk(&mut |e, _| {
            if !e.is_root() && e.tag != e.role && is_standard(&e.role) {
                changes.push((e.id, e.role.clone()));
            }
        });
        if changes.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        for (id, role) in &changes {
            self.inner
                .get_dictionary_mut(*id)?
                .set("S", Object::Name(role.as_bytes().to_vec()));
        }
        self.commit()?;
        Ok(changes.len())
    }
}

/// One kid of a structure element, as the pruning of the tree sees it.
enum PruneKid {
    /// Marked content (an MCID or a marked-content reference) on a page.
    Content(Option<ObjectId>),
    /// An object reference: the page and the annotation.
    Object(Option<ObjectId>, Option<ObjectId>),
    /// A structure element.
    Elem(ObjectId),
    /// A reference to an object that is not in the file.
    Missing,
    Other,
}

/// What pruning the kids of an element found.
struct Pruned {
    kids: Vec<Object>,
    /// This list of kids changed.
    changed: bool,
    /// The kids of an element below changed.
    deep: bool,
    /// The kids held content before.
    had: bool,
    /// They still hold some.
    has: bool,
}

impl Document {
    fn prune_kid(&self, k: &Object, pg: Option<ObjectId>) -> PruneKid {
        let dict_kid = |d: &Dictionary| {
            let page = d.get(b"Pg").ok().and_then(|p| p.as_reference().ok()).or(pg);
            if d.has_type(b"OBJR") || d.has(b"Obj") {
                PruneKid::Object(page, d.get(b"Obj").ok().and_then(|o| o.as_reference().ok()))
            } else if d.has_type(b"MCR") || d.has(b"MCID") {
                PruneKid::Content(page)
            } else {
                PruneKid::Other
            }
        };
        match k {
            Object::Integer(_) => PruneKid::Content(pg),
            Object::Reference(r) => match self.inner.get_object(*r) {
                Ok(Object::Integer(_)) => PruneKid::Content(pg),
                Ok(Object::Dictionary(d)) => {
                    if d.has(b"S") && !d.has_type(b"MCR") && !d.has_type(b"OBJR") {
                        PruneKid::Elem(*r)
                    } else {
                        dict_kid(d)
                    }
                }
                Ok(_) => PruneKid::Other,
                Err(_) => PruneKid::Missing,
            },
            Object::Dictionary(d) => dict_kid(d),
            _ => PruneKid::Other,
        }
    }

    /// Every structure element of the subtree of `id` (`id` included).
    fn struct_subtree(&self, id: ObjectId, out: &mut Vec<ObjectId>, seen: &mut HashSet<ObjectId>) {
        let mut stack = vec![id];
        while let Some(e) = stack.pop() {
            if !seen.insert(e) || seen.len() > MAX_ELEMENTS {
                continue;
            }
            out.push(e);
            for k in self.kid_list(e) {
                if let PruneKid::Elem(c) = self.prune_kid(&k, None) {
                    stack.push(c);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn prune_kids(
        &mut self,
        kids: Vec<Object>,
        pg: Option<ObjectId>,
        gone_pages: &HashSet<ObjectId>,
        gone_annots: &HashSet<ObjectId>,
        removed: &mut Vec<ObjectId>,
        seen: &mut HashSet<ObjectId>,
        depth: usize,
    ) -> Pruned {
        let mut out = Pruned {
            kids: Vec::with_capacity(kids.len()),
            changed: false,
            deep: false,
            had: false,
            has: false,
        };
        let page_gone = |p: Option<ObjectId>| p.is_some_and(|p| gone_pages.contains(&p));
        for k in kids {
            match self.prune_kid(&k, pg) {
                PruneKid::Content(page) => {
                    out.had = true;
                    if page_gone(page) {
                        out.changed = true;
                        continue;
                    }
                    out.has = true;
                }
                PruneKid::Object(page, obj) => {
                    out.had = true;
                    let gone = match obj {
                        Some(o) => gone_annots.contains(&o) || !self.inner.objects.contains_key(&o),
                        None => true,
                    };
                    if gone || page_gone(page) {
                        out.changed = true;
                        continue;
                    }
                    out.has = true;
                }
                PruneKid::Elem(id) => {
                    if depth > 256 || !seen.insert(id) {
                        out.kids.push(k);
                        continue;
                    }
                    let own = self
                        .inner
                        .get_dictionary(id)
                        .ok()
                        .and_then(|d| d.get(b"Pg").ok())
                        .and_then(|p| p.as_reference().ok())
                        .or(pg);
                    let inner = self.kid_list(id);
                    let p = self.prune_kids(inner, own, gone_pages, gone_annots, removed, seen, depth + 1);
                    if p.changed {
                        let _ = self.set_kid_list(id, p.kids);
                    }
                    out.deep |= p.changed || p.deep;
                    out.had |= p.had;
                    if p.had && !p.has {
                        // Everything it held went with the pages or annotations.
                        let mut sub_seen = HashSet::new();
                        self.struct_subtree(id, removed, &mut sub_seen);
                        out.changed = true;
                        continue;
                    }
                    out.has |= p.has;
                }
                PruneKid::Missing => {
                    out.changed = true;
                    continue;
                }
                PruneKid::Other => {}
            }
            out.kids.push(k);
        }
        out
    }

    /// Take out of the structure tree what belonged to the `gone_pages` and
    /// to the `gone_annots` (annotations taken off the pages): their marked
    /// content, marked-content references and object references, then every
    /// element left without content (an element that never had any stays).
    /// The removed elements leave the `/IDTree` and the parent tree is made
    /// again. No undo step: for work copies. Returns the removed elements,
    /// so the references to them elsewhere (bookmarks) can be dropped.
    pub(crate) fn prune_structure(
        &mut self,
        gone_pages: &HashSet<ObjectId>,
        gone_annots: &HashSet<ObjectId>,
    ) -> Result<Vec<ObjectId>> {
        let Some(root) = self.struct_root_id() else {
            return Ok(Vec::new());
        };
        let mut removed = Vec::new();
        let mut seen = HashSet::new();
        let kids = self.kid_list(root);
        let p = self.prune_kids(kids, None, gone_pages, gone_annots, &mut removed, &mut seen, 0);
        if !p.changed && !p.deep {
            return Ok(removed);
        }
        if p.changed {
            self.set_kid_list(root, p.kids)?;
        }
        let gone: HashSet<ObjectId> = removed.iter().copied().collect();
        let tree = self
            .inner
            .get_dictionary(root)?
            .get(b"IDTree")
            .ok()
            .map(|t| deref(&self.inner, t).clone());
        if let Some(Object::Dictionary(tree)) = tree {
            let mut entries = Vec::new();
            crate::util::walk_name_tree(&self.inner, &tree, &mut entries, 0);
            let before = entries.len();
            entries.retain(|(_, v)| !matches!(v, Object::Reference(r) if gone.contains(r)));
            if entries.len() != before {
                if entries.is_empty() {
                    self.inner.get_dictionary_mut(root)?.remove(b"IDTree");
                } else {
                    entries.sort_by(|a, b| a.0.cmp(&b.0));
                    let names: Vec<Object> = entries
                        .into_iter()
                        .flat_map(|(k, v)| [Object::String(k, lopdf::StringFormat::Literal), v])
                        .collect();
                    let leaf = self.inner.add_object(dictionary! { "Names" => names });
                    self.inner
                        .get_dictionary_mut(root)?
                        .set("IDTree", Object::Reference(leaf));
                }
            }
        }
        self.rebuild_parent_tree()?;
        Ok(removed)
    }
}
