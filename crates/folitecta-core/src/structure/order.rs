//! The reading order of a page: items, moving them and joining neighbouring ones.

use super::*;

impl Document {
    // ------------------------------------------------------------------
    // Reading order
    // ------------------------------------------------------------------

    /// Elements of a page in reading order: blocks with their content, tables,
    /// figures and lists as a whole (cells one by one with `table_cells`).
    pub fn reading_order(&self, page: usize, table_cells: bool) -> Vec<OrderItem> {
        let Some(tree) = self.structure() else {
            return Vec::new();
        };
        let marked = self.marked_content(page);
        let rect_of = |m: i64| marked.iter().find(|i| i.mcid == Some(m));
        let mut out: Vec<OrderAcc> = Vec::new();
        fn has_page(e: &StructElem, page: usize) -> bool {
            e.content().iter().any(|c| c.0 == page) || e.annotations().iter().any(|a| a.0 == page)
        }
        fn visit(
            e: &StructElem,
            page: usize,
            table_cells: bool,
            current: Option<usize>,
            out: &mut Vec<OrderAcc>,
            rect_of: &dyn Fn(i64) -> Option<([f32; 4], String)>,
        ) {
            let mut current = current;
            if current.is_none() && !e.is_root() && has_page(e, page) {
                let role = e.role.as_str();
                let grouping = GROUPING.contains(&role) || (table_cells && role == "Table");
                let unit = matches!(role, "Figure" | "Formula" | "L") || (!table_cells && role == "Table");
                let direct = e.kids.iter().any(|k| {
                    matches!(k, StructKid::Content { page: p, .. } | StructKid::Annot { page: p, .. } if *p == page)
                });
                if unit || direct || (!grouping && !INLINE.contains(&role)) {
                    out.push((e.id, e.tag.clone(), e.role.clone(), None, Vec::new()));
                    current = Some(out.len() - 1);
                }
            }
            if let Some(c) = current {
                for k in &e.kids {
                    if let StructKid::Content { page: p, mcid } = k {
                        if *p == page {
                            if let Some((r, t)) = rect_of(*mcid) {
                                out[c].3 = geom::union_opt(out[c].3, r);
                                if !t.is_empty() {
                                    out[c].4.push(t);
                                }
                            }
                        }
                    }
                }
            }
            for k in &e.kids {
                if let StructKid::Elem(child) = k {
                    visit(child, page, table_cells, current, out, rect_of);
                }
            }
        }
        let lookup = |m: i64| rect_of(m).map(|i| (i.rect, i.text.clone()));
        visit(&tree, page, table_cells, None, &mut out, &lookup);
        // Annotation areas.
        let mut subtypes: HashMap<ObjectId, String> = HashMap::new();
        if let (Ok(annots), Ok(size)) = (self.annotations(page), self.page_size(page)) {
            for item in out.iter_mut() {
                if let Some(e) = tree.find(item.0) {
                    for (p, i) in e.annotations() {
                        if p == page {
                            if let Some(a) = annots.get(i) {
                                item.3 =
                                    geom::union_opt(item.3, crate::marks::pdf_rect_to_display(&size, a.rect));
                                subtypes.entry(item.0).or_insert_with(|| a.subtype.clone());
                            }
                        }
                    }
                }
            }
        }
        out.into_iter()
            .map(|(id, tag, role, rect, texts)| OrderItem {
                id,
                tag,
                role,
                rect: rect.unwrap_or_default(),
                text: excerpt(&texts.join(" "), 120),
                annotation: subtypes.remove(&id).unwrap_or_default(),
            })
            .collect()
    }

    /// Move the elements of a page's reading order: the item `from` goes to
    /// the place of the item `to`.
    pub fn move_reading_order(
        &mut self,
        page: usize,
        from: usize,
        to: usize,
        table_cells: bool,
    ) -> Result<()> {
        let items = self.reading_order(page, table_cells);
        let (Some(a), Some(b)) = (items.get(from), items.get(to)) else {
            return Err(Error::Invalid("no such item in the reading order".into()));
        };
        if from == to {
            return Ok(());
        }
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let target_parent = tree
            .parent_of(b.id)
            .ok_or_else(|| Error::Invalid("tag not found".into()))?;
        let pos = target_parent
            .kids
            .iter()
            .position(|k| matches!(k, StructKid::Elem(e) if e.id == b.id))
            .unwrap_or(0);
        let index = if from < to { pos + 1 } else { pos };
        let parent = (!target_parent.is_root()).then_some(target_parent.id);
        self.move_struct_elem(a.id, parent, index)
    }

    /// Join neighbouring items of the reading order with the same tag (lines
    /// of one paragraph); returns how many items went away.
    pub fn group_similar(&mut self, page: usize, table_cells: bool) -> Result<usize> {
        let items = self.reading_order(page, table_cells);
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let mut pairs: Vec<(ObjectId, ObjectId, ObjectId)> = Vec::new();
        let mut keep: Option<&OrderItem> = None;
        for it in &items {
            let joinable =
                matches!(it.role.as_str(), "P" | "H" | "Span") || heading_level(&it.role).is_some();
            match keep {
                Some(k)
                    if joinable
                        && k.tag == it.tag
                        && tree.parent_of(k.id).map(|p| p.id) == tree.parent_of(it.id).map(|p| p.id) =>
                {
                    let parent = tree.parent_of(it.id).map(|p| p.id).unwrap_or((0, 0));
                    pairs.push((k.id, it.id, parent));
                }
                _ => keep = Some(it),
            }
            if !joinable {
                keep = None;
            }
        }
        if pairs.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let root = self.struct_root_id().unwrap_or((0, 0));
        for (into, from, parent) in &pairs {
            let Some(src) = tree.find(*from) else { continue };
            let raw = self.kid_list(*from);
            let mut moved = Vec::new();
            for (o, k) in raw.into_iter().zip(src.kids.iter()) {
                match k {
                    StructKid::Content { page, mcid } => moved.push(self.content_kid(*into, *page, *mcid)),
                    StructKid::Elem(e) => {
                        self.inner
                            .get_dictionary_mut(e.id)?
                            .set("P", Object::Reference(*into));
                        moved.push(o);
                    }
                    _ => moved.push(o),
                }
            }
            let mut kids = self.kid_list(*into);
            kids.extend(moved);
            self.set_kid_list(*into, kids)?;
            let parent = if *parent == (0, 0) { root } else { *parent };
            self.remove_kid(parent, *from)?;
        }
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(pairs.len())
    }
}
