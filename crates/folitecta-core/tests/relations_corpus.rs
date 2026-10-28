//! The relations corpus (FOL-L052): `testdocs::relations_doc`, plain and
//! tagged, taken through every page operation, saved and reopened; after
//! each one every relation still leads somewhere real:
//! - bookmarks, named destinations and links to pages lead to pages of the
//!   document;
//! - every form field has a widget on a page, every widget is listed by the
//!   page its `/P` names, and every widget on a page belongs to a field;
//! - pop-ups and replies are on the page of their comment;
//! - tags are on pages of the document, and the parent tree gives back the
//!   element of every marked content of a page;
//! - no object of the file leads to a page that is not in the page tree.

use std::collections::{HashMap, HashSet};

use folitecta_core::combine::Source;
use folitecta_core::structure::StructKid;
use folitecta_core::testdocs::relations_doc;
use folitecta_core::{Document, LinkTarget};
use lopdf::{Object, ObjectId};

fn open(bytes: Vec<u8>) -> Document {
    Document::from_bytes(bytes, None, None).unwrap()
}

fn corpus(tagged: bool) -> Document {
    let mut doc = open(relations_doc());
    if tagged {
        doc.auto_tag().unwrap();
    }
    doc
}

/// Every object reachable from the trailer.
fn reachable(file: &lopdf::Document) -> HashSet<ObjectId> {
    fn refs(o: &Object, out: &mut Vec<ObjectId>) {
        match o {
            Object::Reference(r) => out.push(*r),
            Object::Array(a) => a.iter().for_each(|o| refs(o, out)),
            Object::Dictionary(d) => d.iter().for_each(|(_, o)| refs(o, out)),
            Object::Stream(s) => s.dict.iter().for_each(|(_, o)| refs(o, out)),
            _ => {}
        }
    }
    let mut seen = HashSet::new();
    let mut stack = Vec::new();
    file.trailer.iter().for_each(|(_, o)| refs(o, &mut stack));
    while let Some(id) = stack.pop() {
        if seen.insert(id) {
            if let Ok(o) = file.get_object(id) {
                refs(o, &mut stack);
            }
        }
    }
    seen
}

/// Every relation of `doc` (as saved and reopened) leads somewhere real.
fn check(doc: &Document, what: &str) {
    let doc = open(doc.bytes().to_vec());
    let file = doc.inner();
    let n = doc.page_count();
    let pages = doc.page_ids();
    let page_set: HashSet<ObjectId> = pages.iter().copied().collect();

    fn walk(items: &[folitecta_core::OutlineItem], what: &str) {
        for i in items {
            assert!(i.valid, "{what}: bookmark {:?} leads nowhere", i.title);
            walk(&i.children, what);
        }
    }
    walk(&doc.outline(), what);
    for d in doc.named_destinations() {
        assert!(d.page.is_some(), "{what}: destination {:?} leads nowhere", d.name);
    }
    for p in 0..n {
        for l in doc.links(p).unwrap() {
            assert!(
                matches!(l.target, LinkTarget::Page(t) if t < n) || matches!(l.target, LinkTarget::Uri(_)),
                "{what}: link on page {p} leads to {:?}",
                l.target
            );
        }
    }

    // Annotations: where each one is, and what its entries lead to.
    let mut on_page: HashMap<ObjectId, usize> = HashMap::new();
    for p in 0..n {
        for a in doc.annotations(p).unwrap() {
            on_page.insert(a.id.unwrap(), p);
        }
    }
    for (id, p) in &on_page {
        let d = file.get_dictionary(*id).unwrap();
        if let Ok(target) = d.get(b"P").and_then(|o| o.as_reference()) {
            assert_eq!(target, pages[*p], "{what}: /P of {id:?}");
        }
        for key in [&b"Popup"[..], b"IRT"] {
            if let Ok(r) = d.get(key).and_then(|o| o.as_reference()) {
                assert_eq!(
                    on_page.get(&r),
                    Some(p),
                    "{what}: /{} of {id:?}",
                    String::from_utf8_lossy(key)
                );
            }
        }
    }
    let fields = doc.form_fields();
    let mut widgets_of_fields = HashSet::new();
    for f in &fields {
        assert!(
            f.widgets.iter().any(|w| on_page.contains_key(w)),
            "{what}: field {:?} has no widget on a page",
            f.name
        );
        widgets_of_fields.extend(f.widgets.iter().copied());
    }
    for (id, p) in &on_page {
        let d = file.get_dictionary(*id).unwrap();
        if d.get(b"Subtype").and_then(|s| s.as_name()).ok() == Some(b"Widget".as_slice()) {
            assert!(
                widgets_of_fields.contains(id),
                "{what}: widget {id:?} on page {p} in no field"
            );
        }
    }
    let mut names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "{what}: two fields of one name");

    // Tags.
    if let Some(tree) = doc.structure() {
        let mut owners: HashMap<(usize, i64), ObjectId> = HashMap::new();
        tree.walk(&mut |e, _| {
            if let Some(p) = e.page {
                assert!(p < n, "{what}: tag {:?} on page {p}", e.tag);
            }
            for k in &e.kids {
                if let StructKid::Content { page, mcid } = k {
                    assert!(*page < n, "{what}: content of {:?} on page {page}", e.tag);
                    owners.insert((*page, *mcid), e.id);
                }
            }
        });
        let root = file
            .catalog()
            .unwrap()
            .get(b"StructTreeRoot")
            .unwrap()
            .as_reference()
            .unwrap();
        let nums = file
            .get_dictionary(root)
            .unwrap()
            .get(b"ParentTree")
            .ok()
            .and_then(|t| t.as_reference().ok())
            .and_then(|t| file.get_dictionary(t).ok())
            .and_then(|t| t.get(b"Nums").ok())
            .and_then(|a| a.as_array().ok())
            .cloned()
            .unwrap_or_default();
        let by_key: HashMap<i64, Object> = nums
            .chunks(2)
            .filter_map(|c| Some((c.first()?.as_i64().ok()?, c.get(1)?.clone())))
            .collect();
        for ((page, mcid), elem) in owners {
            let key = file
                .get_dictionary(pages[page])
                .unwrap()
                .get(b"StructParents")
                .unwrap_or_else(|_| panic!("{what}: page {page} has tags but no /StructParents"))
                .as_i64()
                .unwrap();
            let arr = match &by_key[&key] {
                Object::Array(a) => a.clone(),
                Object::Reference(r) => file.get_object(*r).unwrap().as_array().unwrap().clone(),
                o => panic!("{what}: parent tree entry {o:?}"),
            };
            assert_eq!(
                arr.get(mcid as usize).and_then(|o| o.as_reference().ok()),
                Some(elem),
                "{what}: parent tree, page {page}, marked content {mcid}"
            );
        }
    }

    // Nothing leads to a page that is not in the tree.
    for id in reachable(file) {
        let Ok(d) = file.get_dictionary(id) else { continue };
        if d.get(b"Type").and_then(|t| t.as_name()).ok() == Some(b"Page".as_slice()) {
            assert!(
                page_set.contains(&id),
                "{what}: the file keeps page object {id:?} out of the tree"
            );
        }
    }
}

fn source(doc: &Document, name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, doc.bytes().as_slice()).unwrap();
    path
}

type Operation = (&'static str, fn(&mut Document));

fn last(d: &Document) -> usize {
    d.page_count() - 1
}

/// Delete those of `pages` the document has, keeping one page at least.
fn delete(d: &mut Document, pages: &[usize]) {
    let n = d.page_count();
    let pages: Vec<usize> = pages.iter().copied().filter(|&p| p < n).take(n - 1).collect();
    if !pages.is_empty() {
        d.delete_pages(&pages).unwrap();
    }
}

const OPERATIONS: &[Operation] = &[
    ("rotate", |d| d.rotate_pages(&[0, last(d)], 90).unwrap()),
    ("move", |d| d.move_pages(&[0], last(d)).unwrap()),
    ("swap", |d| d.swap_pages(0, last(d)).unwrap()),
    ("reverse", |d| d.reverse_pages().unwrap()),
    ("reorder", |d| {
        // Every other page first.
        let n = d.page_count();
        let order: Vec<usize> = (0..n).step_by(2).chain((1..n).step_by(2)).collect();
        d.reorder_pages(&order).unwrap()
    }),
    ("duplicate", |d| {
        d.duplicate_pages(&(0..d.page_count()).collect::<Vec<_>>())
            .unwrap()
    }),
    ("delete first", |d| delete(d, &[0])),
    ("delete middle", |d| delete(d, &[1, 2])),
    ("delete last", |d| delete(d, &[last(d)])),
    ("insert blank", |d| {
        d.insert_blank_pages(1, 2, 595.0, 842.0).unwrap()
    }),
    ("extract", |d| *d = d.extract_pages(&[last(d), 0]).unwrap()),
    ("split", |d| {
        let n = d.page_count();
        *d = d.split_every(n.div_ceil(2)).unwrap().pop().unwrap()
    }),
    ("insert itself", |d| {
        let other = open(d.bytes().to_vec());
        d.insert_pages_from(1, &other, None, None).unwrap();
    }),
    ("insert some pages", |d| {
        let other = open(d.bytes().to_vec());
        d.insert_pages_from(1, &other, Some(&[last(d), 0]), None).unwrap();
    }),
    ("insert the plain corpus", |d| {
        d.insert_pages_from(0, &corpus(false), None, None).unwrap();
    }),
    ("combine", |d| {
        let a = source(d, "a.pdf");
        let b = source(&corpus(true), "b.pdf");
        let (doc, problems, _) = Document::combine_with_arrivals(&[
            Source { path: a, pages: None },
            Source {
                path: b,
                pages: Some(vec![2, 1]),
            },
        ])
        .unwrap();
        assert!(problems.is_empty());
        *d = doc;
    }),
];

#[test]
fn every_page_operation_keeps_the_relations() {
    for tagged in [false, true] {
        let what = if tagged { "tagged" } else { "plain" };
        check(&corpus(tagged), what);
        for (name, op) in OPERATIONS {
            let mut doc = corpus(tagged);
            op(&mut doc);
            check(&doc, &format!("{what}, {name}"));
            // And again on what came out.
            op(&mut doc);
            check(&doc, &format!("{what}, {name} twice"));
        }
    }
}

#[test]
fn operations_in_a_row_keep_the_relations() {
    let mut doc = corpus(true);
    for (name, op) in OPERATIONS {
        op(&mut doc);
        check(&doc, &format!("after {name}"));
    }
    // Undo goes back through all of them on the last document.
    while doc.can_undo() {
        doc.undo().unwrap();
        check(&doc, "undo");
    }
}

#[test]
#[should_panic(expected = "leads nowhere")]
fn the_check_finds_what_a_page_left_behind() {
    // Page 3 taken out of the page tree by hand, nothing else changed: the
    // bookmark "Part B" and the destination `third` still lead to it.
    let mut doc = corpus(false);
    let root = doc
        .inner()
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    let gone = doc.page_ids()[2];
    let pages = doc.inner_mut().get_dictionary_mut(root).unwrap();
    let kids: Vec<Object> = pages
        .get(b"Kids")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter(|k| k.as_reference().ok() != Some(gone))
        .cloned()
        .collect();
    pages.set("Kids", kids);
    pages.set("Count", 3);
    doc.commit().unwrap();
    check(&doc, "a page taken out by hand");
}
