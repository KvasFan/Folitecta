//! Tagging content: probes that measure untagged drawing, tags made from an area, annotations and automatic tagging of a document.

use super::*;

impl Document {
    // ------------------------------------------------------------------
    // Tagging content
    // ------------------------------------------------------------------

    /// Units of drawing outside tagged content, measured through probes.
    pub(super) fn probe_pages(&self, pages: &[usize]) -> Result<HashMap<usize, (PageStreams, Vec<Probed>)>> {
        let page_ids = self.page_ids();
        let mut copy = self.inner.clone();
        let mut found: HashMap<usize, (PageStreams, Vec<Unit>)> = HashMap::new();
        for &p in pages {
            let Some(&pid) = page_ids.get(p) else { continue };
            let ps = PageStreams::read(&self.inner, pid);
            let data = &ps.data;
            let ops = operations(data);
            let units = content_units(data, &ops);
            let wraps: Vec<(usize, usize, Wrap)> = units
                .iter()
                .enumerate()
                .map(|(k, u)| {
                    (
                        u.first,
                        u.last,
                        Wrap::Mcid("FolitectaProbe".into(), PROBE_BASE + k as i64),
                    )
                })
                .collect();
            let probe: Vec<u8> = write_wrapped(data, &ops, &wraps)
                .into_iter()
                .flat_map(|p| p.1)
                .collect();
            set_page_content(&mut copy, pid, probe)?;
            found.insert(p, (ps, units));
        }
        let bytes = Arc::new(crate::document::serialize(&mut copy)?);
        let measured = measure(bytes, pages)?;
        Ok(found
            .into_iter()
            .map(|(p, (ps, units))| {
                let items = measured.get(&p).cloned().unwrap_or_default();
                let probed = units
                    .into_iter()
                    .enumerate()
                    .map(|(k, unit)| Probed {
                        unit,
                        item: items
                            .iter()
                            .find(|i| i.mcid == Some(PROBE_BASE + k as i64))
                            .cloned(),
                    })
                    .collect();
                (p, (ps, probed))
            })
            .collect())
    }

    /// Runs of operations for units that share marked content: text units of
    /// one segment go together, any other unit alone.
    pub(super) fn runs(units: &[Unit]) -> Vec<(usize, usize)> {
        // Text units next to each other in one segment: (segment, unit number).
        type Run = (usize, usize, Option<(usize, usize)>);
        let mut out: Vec<Run> = Vec::new();
        for u in units {
            match out.last_mut() {
                Some(last) if u.kind == UnitKind::Text && last.2 == Some((u.segment, u.seq - 1)) => {
                    last.1 = u.last;
                    last.2 = Some((u.segment, u.seq));
                }
                _ => out.push((
                    u.first,
                    u.last,
                    (u.kind == UnitKind::Text).then_some((u.segment, u.seq)),
                )),
            }
        }
        out.into_iter().map(|(a, b, _)| (a, b)).collect()
    }

    /// Tag the content drawn inside an area of a page: content not tagged yet
    /// is marked, tagged content moves to the new element. The element goes
    /// into `parent` (the document element or the root when None).
    pub fn tag_area(
        &mut self,
        page: usize,
        rect: [f32; 4],
        tag: &str,
        parent: Option<ObjectId>,
        index: Option<usize>,
    ) -> Result<ObjectId> {
        let tag = tag.trim();
        if tag.is_empty() || tag.contains(char::is_whitespace) {
            return Err(Error::Invalid("a tag is one word".into()));
        }
        let pid = self.page_id(page)?;
        let probes = self.probe_pages(&[page])?;
        let (ps, probed) = probes.get(&page).cloned().unwrap_or_default();
        let inside = |r: [f32; 4]| {
            if !geom::intersects(r, rect) {
                return false;
            }
            let ix = (r[2].min(rect[2]) - r[0].max(rect[0])).max(0.0);
            let iy = (r[3].min(rect[3]) - r[1].max(rect[1])).max(0.0);
            let area = ((r[2] - r[0]) * (r[3] - r[1])).max(0.01);
            let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
            ix * iy / area > 0.3 || (cx >= rect[0] && cx <= rect[2] && cy >= rect[1] && cy <= rect[3])
        };
        let chosen: Vec<Unit> = probed
            .iter()
            .filter(|p| p.item.as_ref().is_some_and(|i| inside(i.rect)))
            .map(|p| p.unit)
            .collect();
        let existing: Vec<i64> = self
            .marked_content(page)
            .iter()
            .filter(|i| i.mcid.is_some() && inside(i.rect))
            .filter_map(|i| i.mcid)
            .collect();
        if chosen.is_empty() && existing.is_empty() {
            return Err(Error::Invalid("nothing is drawn in the selected area".into()));
        }
        self.begin_edit();
        let mut new_mcids = Vec::new();
        if !chosen.is_empty() {
            let ops = operations(&ps.data);
            let mut next = max_mcid(&ps.data).map(|m| m + 1).unwrap_or(0);
            let wraps: Vec<(usize, usize, Wrap)> = Self::runs(&chosen)
                .into_iter()
                .map(|(a, b)| {
                    let w = Wrap::Mcid(tag.to_string(), next);
                    new_mcids.push(next);
                    next += 1;
                    (a, b, w)
                })
                .collect();
            write_pieces(&mut self.inner, pid, &ps, write_wrapped(&ps.data, &ops, &wraps))?;
        }
        let moved: HashSet<(usize, i64)> = existing.iter().map(|m| (page, *m)).collect();
        self.detach_content(&moved, &HashSet::new())?;
        let root = self.ensure_struct_root()?;
        let parent = match parent {
            Some(p) if p != (0, 0) => p,
            _ => self.document_elem(root)?,
        };
        let id = self.new_struct_elem(parent, tag, Some(page));
        let mut kids: Vec<Object> = Vec::new();
        let mut all: Vec<i64> = existing;
        all.extend(new_mcids);
        all.sort_unstable();
        all.dedup();
        for m in all {
            kids.push(Object::Integer(m));
        }
        self.set_kid_list(id, kids)?;
        self.insert_kid(parent, index, Object::Reference(id))?;
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(id)
    }

    /// Tag an annotation (`Link`, `Form` or `Annot` element) inside `parent`.
    pub fn tag_annotation(
        &mut self,
        page: usize,
        index: usize,
        parent: Option<ObjectId>,
    ) -> Result<ObjectId> {
        let annot = self
            .annotations(page)?
            .into_iter()
            .nth(index)
            .ok_or_else(|| Error::Invalid("annotation not found".into()))?;
        let kid = self
            .annot_kid(page, index)
            .ok_or_else(|| Error::Invalid("the annotation cannot be tagged".into()))?;
        let tag = match annot.subtype.as_str() {
            "Link" => "Link",
            "Widget" => "Form",
            _ => "Annot",
        };
        self.begin_edit();
        let mut detach = HashSet::new();
        detach.insert((page, index));
        self.detach_content(&HashSet::new(), &detach)?;
        let root = self.ensure_struct_root()?;
        let parent = match parent {
            Some(p) if p != (0, 0) => p,
            _ => self.document_elem(root)?,
        };
        let id = self.new_struct_elem(parent, tag, Some(page));
        self.set_kid_list(id, vec![kid])?;
        self.insert_kid(parent, None, Object::Reference(id))?;
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(id)
    }

    /// The `Document` element under the root, created when the root has none.
    pub(super) fn document_elem(&mut self, root: ObjectId) -> Result<ObjectId> {
        let roles = self.roles();
        for k in self.kid_list(root) {
            if let Ok(r) = k.as_reference() {
                if let Ok(d) = self.inner.get_dictionary(r) {
                    if name_value(d, b"S").map(|s| role_of(&roles, &s)).as_deref() == Some("Document") {
                        return Ok(r);
                    }
                }
            }
        }
        let id = self.new_struct_elem(root, "Document", None);
        self.insert_kid(root, None, Object::Reference(id))?;
        Ok(id)
    }

    /// Annotations that have no element yet (form fields only with `fields_only`).
    pub(super) fn untagged_annotations(&self, fields_only: bool) -> Vec<(usize, usize, String)> {
        let tagged: HashSet<(usize, usize)> = self
            .structure()
            .map(|t| {
                let mut s = HashSet::new();
                t.walk(&mut |e, _| {
                    for k in &e.kids {
                        if let StructKid::Annot { page, index, .. } = k {
                            s.insert((*page, *index));
                        }
                    }
                });
                s
            })
            .unwrap_or_default();
        let mut todo = Vec::new();
        for p in 0..self.page_count() {
            for a in self.annotations(p).unwrap_or_default() {
                let taggable = match a.subtype.as_str() {
                    "Popup" | "PrinterMark" | "TrapNet" => false,
                    "Widget" => true,
                    _ => !fields_only && !a.hidden,
                };
                if taggable && a.id.is_some() && !tagged.contains(&(p, a.index)) {
                    todo.push((p, a.index, a.subtype.clone()));
                }
            }
        }
        todo
    }

    /// Tag annotations without an element; returns how many were tagged.
    /// Does not commit.
    pub(super) fn tag_annotations_inner(&mut self, todo: &[(usize, usize, String)]) -> Result<usize> {
        if todo.is_empty() {
            return Ok(0);
        }
        let root = self.ensure_struct_root()?;
        let doc_elem = self.document_elem(root)?;
        let mut pages_with_annots = HashSet::new();
        for (p, i, subtype) in todo {
            let Some(kid) = self.annot_kid(*p, *i) else {
                continue;
            };
            let tag = match subtype.as_str() {
                "Link" => "Link",
                "Widget" => "Form",
                _ => "Annot",
            };
            let id = self.new_struct_elem(doc_elem, tag, Some(*p));
            self.set_kid_list(id, vec![kid])?;
            self.insert_kid(doc_elem, None, Object::Reference(id))?;
            pages_with_annots.insert(*p);
        }
        let page_ids = self.page_ids();
        for p in pages_with_annots {
            if let Some(pd) = page_ids
                .get(p)
                .and_then(|pid| self.inner.get_dictionary_mut(*pid).ok())
            {
                pd.set("Tabs", Object::Name(b"S".to_vec()));
            }
        }
        self.rebuild_parent_tree()?;
        Ok(todo.len())
    }

    /// Tag the form fields that have no element yet; returns how many.
    pub fn tag_form_fields(&mut self) -> Result<usize> {
        self.tag_untagged(true)
    }

    /// Tag every annotation that has no element yet; returns how many.
    pub fn tag_annotations(&mut self) -> Result<usize> {
        self.tag_untagged(false)
    }

    pub(super) fn tag_untagged(&mut self, fields_only: bool) -> Result<usize> {
        let todo = self.untagged_annotations(fields_only);
        if todo.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let n = self.tag_annotations_inner(&todo)?;
        self.commit()?;
        Ok(n)
    }

    /// Tag the whole document from what its pages draw: paragraphs and
    /// headings (by text size), figures, artifacts for drawings and for the
    /// running headers and footers, annotations. Existing tags are replaced.
    pub fn auto_tag(&mut self) -> Result<AutoTagReport> {
        let pages: Vec<usize> = (0..self.page_count()).collect();
        self.begin_edit();
        if self.has_structure() {
            self.clear_structure_inner()?;
        }
        let probes = self.probe_pages(&pages)?;
        let sizes: Vec<crate::document::PageSize> = self.page_sizes();
        // Text that repeats at the top or bottom of the pages.
        let norm = |t: &str| {
            t.chars()
                .filter(|c| !c.is_ascii_digit())
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        };
        let in_margin = |p: usize, r: [f32; 4]| {
            let h = sizes.get(p).map(|s| s.height).unwrap_or(792.0);
            r[3] < h * 0.08 || r[1] > h * 0.92
        };
        let mut repeated: HashMap<String, HashSet<usize>> = HashMap::new();
        for (p, (_, probed)) in &probes {
            for pr in probed {
                if let Some(i) = &pr.item {
                    if i.has_text && in_margin(*p, i.rect) && i.text.chars().count() < 100 {
                        repeated.entry(norm(&i.text)).or_default().insert(*p);
                    }
                }
            }
        }
        let is_running = |p: usize, i: &MarkedItem| {
            if !i.has_text || !in_margin(p, i.rect) || i.text.chars().count() >= 100 {
                return false;
            }
            let n = norm(&i.text);
            n.is_empty() || (pages.len() > 1 && repeated.get(&n).is_some_and(|s| s.len() >= 2))
        };
        // Size of the body text: the size most characters have.
        let mut weights: HashMap<i32, usize> = HashMap::new();
        for (p, (_, probed)) in &probes {
            for pr in probed {
                if let Some(i) = &pr.item {
                    if i.has_text && !is_running(*p, i) {
                        *weights.entry((i.font_size * 2.0).round() as i32).or_default() +=
                            i.text.chars().count();
                    }
                }
            }
        }
        let body = weights
            .iter()
            .max_by_key(|(s, n)| (**n, -**s))
            .map(|(s, _)| *s as f32 / 2.0)
            .unwrap_or(10.0);
        // Paragraph and heading clusters, figures and artifacts per page.
        enum Block {
            Text {
                heading: bool,
                size: f32,
                units: Vec<Unit>,
                rect: [f32; 4],
                chars: usize,
            },
            Figure(Unit, [f32; 4]),
        }
        let mut blocks: BTreeMap<usize, Vec<Block>> = BTreeMap::new();
        let mut artifacts: BTreeMap<usize, Vec<Unit>> = BTreeMap::new();
        let mut heading_sizes: Vec<i32> = Vec::new();
        for &p in &pages {
            let Some((ps, probed)) = probes.get(&p) else {
                continue;
            };
            let list = blocks.entry(p).or_default();
            for pr in probed {
                // Units that draw nothing need no marking.
                let Some(item) = &pr.item else { continue };
                // Watermarks, running headers and page numbers added by Folitecta.
                if ps.is_pagination(pr.unit.offset) {
                    artifacts.entry(p).or_default().push(pr.unit);
                    continue;
                }
                let text_like = item.has_text && !item.text.trim().is_empty();
                if text_like && !is_running(p, item) {
                    let size = item.font_size;
                    let heading = size >= body * 1.2;
                    if let Some(Block::Text {
                        heading: h,
                        size: s,
                        units,
                        rect,
                        chars,
                    }) = list.last_mut()
                    {
                        let same_class = *h == heading && (size - *s).abs() <= 0.12 * s.max(size);
                        let last = *rect;
                        let line = ((item.rect[1] + item.rect[3]) / 2.0 > last[1])
                            && ((item.rect[1] + item.rect[3]) / 2.0 < last[3]);
                        let below =
                            item.rect[1] >= last[3] - 0.5 * size && item.rect[1] - last[3] <= 0.9 * size;
                        let overlap = item.rect[0] < last[2] + size && item.rect[2] > last[0] - size;
                        if same_class && (line || (below && overlap)) {
                            units.push(pr.unit);
                            *rect = geom::union_opt(Some(*rect), item.rect).unwrap_or(*rect);
                            *chars += item.text.chars().count();
                            continue;
                        }
                    }
                    list.push(Block::Text {
                        heading,
                        size,
                        units: vec![pr.unit],
                        rect: item.rect,
                        chars: item.text.chars().count(),
                    });
                } else if item.has_image && !text_like {
                    list.push(Block::Figure(pr.unit, item.rect));
                } else {
                    artifacts.entry(p).or_default().push(pr.unit);
                }
            }
            for b in list.iter_mut() {
                if let Block::Text {
                    heading, size, chars, ..
                } = b
                {
                    if *heading && *chars > 200 {
                        *heading = false;
                    }
                    if *heading {
                        heading_sizes.push((*size * 2.0).round() as i32);
                    }
                }
            }
        }
        heading_sizes.sort_unstable_by(|a, b| b.cmp(a));
        heading_sizes.dedup();
        let level_of = |size: f32| {
            let s = (size * 2.0).round() as i32;
            heading_sizes
                .iter()
                .position(|h| *h == s)
                .map(|i| (i + 1).min(6))
                .unwrap_or(1)
        };
        // Write the marked content and build the elements.
        let mut report = AutoTagReport::default();
        let root = self.ensure_struct_root()?;
        let doc_elem = self.document_elem(root)?;
        let page_ids = self.page_ids();
        for &p in &pages {
            let (Some((ps, _)), Some(&pid)) = (probes.get(&p), page_ids.get(p)) else {
                continue;
            };
            let ops = operations(&ps.data);
            let mut next = 0i64;
            let mut wraps: Vec<(usize, usize, Wrap)> = Vec::new();
            let mut elems: Vec<(String, Vec<i64>, [f32; 4])> = Vec::new();
            for b in blocks.get(&p).map(|v| v.as_slice()).unwrap_or(&[]) {
                match b {
                    Block::Text {
                        heading,
                        size,
                        units,
                        rect,
                        ..
                    } => {
                        let tag = if *heading {
                            format!("H{}", level_of(*size))
                        } else {
                            "P".to_string()
                        };
                        let mut ids = Vec::new();
                        for (a, z) in Self::runs(units) {
                            wraps.push((a, z, Wrap::Mcid(tag.clone(), next)));
                            ids.push(next);
                            next += 1;
                        }
                        if *heading {
                            report.headings += 1;
                        } else {
                            report.paragraphs += 1;
                        }
                        elems.push((tag, ids, *rect));
                    }
                    Block::Figure(u, rect) => {
                        wraps.push((u.first, u.last, Wrap::Mcid("Figure".into(), next)));
                        elems.push(("Figure".into(), vec![next], *rect));
                        next += 1;
                        report.figures += 1;
                    }
                }
            }
            for u in artifacts.get(&p).map(|v| v.as_slice()).unwrap_or(&[]) {
                wraps.push((u.first, u.last, Wrap::Artifact));
                report.artifacts += 1;
            }
            wraps.sort_by_key(|w| w.0);
            if !wraps.is_empty() {
                write_pieces(&mut self.inner, pid, ps, write_wrapped(&ps.data, &ops, &wraps))?;
            }
            let mut created: Vec<(ObjectId, [f32; 4], String)> = Vec::new();
            for (tag, ids, rect) in elems {
                let id = self.new_struct_elem(doc_elem, &tag, Some(p));
                self.set_kid_list(id, ids.into_iter().map(Object::Integer).collect())?;
                self.insert_kid(doc_elem, None, Object::Reference(id))?;
                created.push((id, rect, tag));
            }
            // Links inside the paragraph they cover, other annotations after the page.
            let size = self.page_size(p)?;
            let mut has_annots = false;
            for a in self.annotations(p).unwrap_or_default() {
                if matches!(a.subtype.as_str(), "Popup" | "PrinterMark" | "TrapNet") || a.id.is_none() {
                    continue;
                }
                if a.hidden && a.subtype != "Widget" {
                    continue;
                }
                let Some(kid) = self.annot_kid(p, a.index) else {
                    continue;
                };
                let tag = match a.subtype.as_str() {
                    "Link" => "Link",
                    "Widget" => "Form",
                    _ => "Annot",
                };
                let r = crate::marks::pdf_rect_to_display(&size, a.rect);
                let host = (tag == "Link")
                    .then(|| {
                        created
                            .iter()
                            .filter(|(_, cr, t)| t != "Figure" && geom::intersects(*cr, r))
                            .max_by(|x, y| {
                                let ov = |c: [f32; 4]| {
                                    (c[2].min(r[2]) - c[0].max(r[0])).max(0.0)
                                        * (c[3].min(r[3]) - c[1].max(r[1])).max(0.0)
                                };
                                ov(x.1).total_cmp(&ov(y.1))
                            })
                            .map(|c| c.0)
                    })
                    .flatten();
                let parent = host.unwrap_or(doc_elem);
                let id = self.new_struct_elem(parent, tag, Some(p));
                self.set_kid_list(id, vec![kid])?;
                self.insert_kid(parent, None, Object::Reference(id))?;
                report.annotations += 1;
                has_annots = true;
            }
            if has_annots {
                self.inner
                    .get_dictionary_mut(pid)?
                    .set("Tabs", Object::Name(b"S".to_vec()));
            }
        }
        self.set_marked(true)?;
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(report)
    }

    /// Elements of a role (for example every `Figure`), in document order.
    pub fn struct_elems_of(&self, role: &str) -> Vec<StructElem> {
        let mut out = Vec::new();
        if let Some(t) = self.structure() {
            t.walk(&mut |e, _| {
                if e.role == role {
                    out.push(e.clone());
                }
            });
        }
        out
    }
}
