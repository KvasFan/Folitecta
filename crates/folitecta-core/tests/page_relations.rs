//! Pages that leave a document (deleted, or left out of an extract or a
//! split) take with them what belongs or leads to them: bookmarks, named
//! destinations, links, form fields, tags, the opening view; nothing of them
//! stays in the file, and what the rest needs stays. Checked on
//! `testdocs::relations_doc` (see there what points where).

use folitecta_core::testdocs::relations_doc;
use folitecta_core::{Document, LeftOut, LinkTarget};

fn open(bytes: Vec<u8>) -> Document {
    Document::from_bytes(bytes, None, None).unwrap()
}

/// The document as a file would give it back.
fn reopened(doc: &Document) -> Document {
    open(doc.bytes().to_vec())
}

/// The pages whose `Marker of page n` text some stream of the file holds.
fn markers(doc: &Document) -> Vec<usize> {
    let file = lopdf::Document::load_mem(doc.bytes().as_slice()).unwrap();
    let mut out: Vec<usize> = (1..=4)
        .filter(|n| {
            let marker = format!("Marker of page {n}");
            file.objects.values().any(|o| {
                o.as_stream().ok().is_some_and(|s| {
                    let data = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
                    data.windows(marker.len()).any(|w| w == marker.as_bytes())
                })
            })
        })
        .collect();
    out.sort_unstable();
    out
}

fn bookmarks(doc: &Document) -> Vec<(usize, String, Option<usize>)> {
    doc.outline_flat()
}

fn destinations(doc: &Document) -> Vec<(String, Option<usize>)> {
    doc.named_destinations()
        .into_iter()
        .map(|d| (d.name, d.page))
        .collect()
}

fn links(doc: &Document) -> Vec<(usize, LinkTarget)> {
    (0..doc.page_count())
        .flat_map(|p| doc.links(p).unwrap().into_iter().map(move |l| (p, l.target)))
        .collect()
}

/// Every field with the pages of its widgets.
fn fields(doc: &Document) -> Vec<(String, Vec<usize>)> {
    doc.form_fields()
        .into_iter()
        .map(|f| {
            let pages = f
                .widgets
                .iter()
                .filter_map(|w| {
                    (0..doc.page_count())
                        .find(|&p| doc.annotations(p).unwrap().iter().any(|a| a.id == Some(*w)))
                })
                .collect();
            (f.name, pages)
        })
        .collect()
}

fn page(n: usize) -> Option<usize> {
    Some(n)
}

#[test]
fn the_corpus_is_what_it_says() {
    let doc = open(relations_doc());
    assert_eq!(doc.page_count(), 4);
    assert_eq!(doc.page_labels(), ["i", "ii", "1", "2"]);
    assert_eq!(
        bookmarks(&doc),
        [
            (0, "Part A".into(), page(0)),
            (1, "Page 1".into(), page(0)),
            (1, "Page 2".into(), page(1)),
            (0, "Part B".into(), page(2)),
            (1, "Page 4".into(), page(3)),
        ]
    );
    assert_eq!(
        destinations(&doc),
        [
            ("first".into(), page(0)),
            ("fourth".into(), page(3)),
            ("second".into(), page(1)),
            ("third".into(), page(2)),
        ]
    );
    assert_eq!(
        links(&doc),
        [
            (0, LinkTarget::Page(2)),
            (1, LinkTarget::Page(3)),
            (2, LinkTarget::Page(1)),
            (3, LinkTarget::Uri("https://example.org/".into())),
        ]
    );
    assert_eq!(
        fields(&doc),
        [
            ("name".into(), vec![0]),
            ("shared".into(), vec![0, 2]),
            ("choice".into(), vec![1, 1]),
            ("notes".into(), vec![3]),
        ]
    );
    assert_eq!(markers(&doc), [1, 2, 3, 4]);
}

#[test]
fn a_deleted_page_takes_what_leads_to_it() {
    let mut doc = open(relations_doc());
    // Page 3: "Part B" and `third` lead there, the link of page 1 too; the
    // second widget of `shared` and the rectangle are on it.
    let takes = doc.deleting_pages_takes(&[2]).unwrap();
    assert_eq!(
        takes,
        LeftOut {
            bookmarks: 1,
            links: 1,
            destinations: 1,
            resources: takes.resources,
            ..LeftOut::default()
        }
    );
    // Asking changed nothing.
    assert_eq!(doc.page_count(), 4);
    assert!(!doc.can_undo());

    doc.delete_pages(&[2]).unwrap();
    for doc in [&doc, &reopened(&doc)] {
        assert_eq!(doc.page_count(), 3);
        // "Page 4" moves up in place of its parent.
        assert_eq!(
            bookmarks(doc),
            [
                (0, "Part A".into(), page(0)),
                (1, "Page 1".into(), page(0)),
                (1, "Page 2".into(), page(1)),
                (0, "Page 4".into(), page(2)),
            ]
        );
        assert_eq!(
            destinations(doc),
            [
                ("first".into(), page(0)),
                ("fourth".into(), page(2)),
                ("second".into(), page(1)),
            ]
        );
        assert_eq!(
            links(doc),
            [
                (1, LinkTarget::Page(2)),
                (2, LinkTarget::Uri("https://example.org/".into())),
            ]
        );
        // `shared` keeps its widget of page 1.
        assert_eq!(
            fields(doc),
            [
                ("name".into(), vec![0]),
                ("shared".into(), vec![0]),
                ("choice".into(), vec![1, 1]),
                ("notes".into(), vec![2]),
            ]
        );
        // The labels stay as they were shown.
        assert_eq!(doc.page_labels(), ["i", "ii", "2"]);
        assert_eq!(markers(doc), [1, 2, 4]);
    }
    // One undo step brings everything back.
    doc.undo().unwrap();
    assert_eq!(markers(&doc), [1, 2, 3, 4]);
    assert_eq!(bookmarks(&doc).len(), 5);
    assert_eq!(fields(&doc)[1], ("shared".into(), vec![0, 2]));
}

#[test]
fn deleting_the_pages_of_a_whole_field_and_of_the_opening_view() {
    let mut doc = open(relations_doc());
    // Page 2: the radio buttons, the comments, `second`, the opening view, the
    // GoTo bookmark "Page 2" and the link of page 3.
    let takes = doc.deleting_pages_takes(&[1]).unwrap();
    assert_eq!(
        (
            takes.bookmarks,
            takes.links,
            takes.destinations,
            takes.fields,
            takes.open_action
        ),
        (1, 1, 1, 1, true)
    );
    doc.delete_pages(&[1]).unwrap();
    let doc = reopened(&doc);
    assert_eq!(
        fields(&doc),
        [
            ("name".into(), vec![0]),
            ("shared".into(), vec![0, 1]),
            ("notes".into(), vec![2]),
        ]
    );
    assert!(doc.annotations(1).unwrap().iter().all(|a| a.subtype != "Text"));
    assert!(!doc.inner().catalog().unwrap().has(b"OpenAction"));
    assert_eq!(markers(&doc), [1, 3, 4]);
}

#[test]
fn tags_of_a_deleted_page_go_and_the_page_with_them() {
    let mut doc = open(relations_doc());
    doc.auto_tag().unwrap();
    let pages_of_tags = |doc: &Document| {
        let mut out = Vec::new();
        doc.structure().unwrap().walk(&mut |e, _| out.push(e.page));
        out
    };
    let before = pages_of_tags(&doc);
    assert!(before.contains(&Some(3)));
    let takes = doc.deleting_pages_takes(&[0]).unwrap();
    assert!(takes.tags > 0, "{takes:?}");
    doc.delete_pages(&[0]).unwrap();
    let doc = reopened(&doc);
    let after = pages_of_tags(&doc);
    // Every tag left is on a page of the document, or on none (the parts).
    assert!(after.iter().all(|p| p.is_none_or(|p| p < 3)), "{after:?}");
    assert_eq!(
        before.iter().filter(|p| p.is_some()).count() - takes.tags,
        after.iter().filter(|p| p.is_some()).count()
    );
    assert_eq!(markers(&doc), [2, 3, 4]);
}

#[test]
fn an_extract_keeps_only_what_its_pages_need() {
    let doc = open(relations_doc());
    let part = reopened(&doc.extract_pages(&[2, 3]).unwrap());
    assert_eq!(part.page_count(), 2);
    assert_eq!(
        bookmarks(&part),
        [(0, "Part B".into(), page(0)), (1, "Page 4".into(), page(1))]
    );
    assert_eq!(
        destinations(&part),
        [("fourth".into(), page(1)), ("third".into(), page(0))]
    );
    // The link of page 3 led to page 2.
    assert_eq!(
        links(&part),
        [(1, LinkTarget::Uri("https://example.org/".into()))]
    );
    assert_eq!(
        fields(&part),
        [("shared".into(), vec![0]), ("notes".into(), vec![1])]
    );
    assert_eq!(part.page_labels(), ["1", "2"]);
    assert_eq!(markers(&part), [3, 4]);
    // The document itself did not change.
    assert_eq!(markers(&doc), [1, 2, 3, 4]);
}

#[test]
fn a_split_gives_each_part_its_own() {
    let doc = open(relations_doc());
    let parts: Vec<Document> = doc.split_every(2).unwrap().iter().map(reopened).collect();
    assert_eq!(markers(&parts[0]), [1, 2]);
    assert_eq!(markers(&parts[1]), [3, 4]);
    assert_eq!(
        bookmarks(&parts[0]),
        [
            (0, "Part A".into(), page(0)),
            (1, "Page 1".into(), page(0)),
            (1, "Page 2".into(), page(1)),
        ]
    );
    assert_eq!(
        fields(&parts[0]).iter().map(|f| f.0.as_str()).collect::<Vec<_>>(),
        ["name", "shared", "choice"]
    );
}

#[test]
fn moving_pages_keeps_every_relation() {
    let mut doc = open(relations_doc());
    doc.reorder_pages(&[3, 2, 1, 0]).unwrap();
    let doc = reopened(&doc);
    assert_eq!(bookmarks(&doc)[0], (0, "Part A".into(), page(3)));
    assert_eq!(destinations(&doc)[0], ("first".into(), page(3)));
    assert_eq!(
        links(&doc)[0],
        (0, LinkTarget::Uri("https://example.org/".into()))
    );
    assert_eq!(fields(&doc)[1], ("shared".into(), vec![3, 1]));
    assert_eq!(markers(&doc), [1, 2, 3, 4]);
}

#[test]
fn asking_about_a_long_document_is_quick() {
    let doc = open(folitecta_core::testdocs::long_doc_linked(400));
    let start = std::time::Instant::now();
    let takes = doc.deleting_pages_takes(&[10, 11]).unwrap();
    let took = start.elapsed();
    // The link of page 11 goes with its page: only that of page 10 is counted.
    assert_eq!((takes.bookmarks, takes.destinations, takes.links), (2, 2, 1));
    assert!(took.as_secs_f64() < 2.0, "{took:?}");
}

/// An annotation: id, kind, and where its `/P`, `/Popup`, `/Parent` (of a
/// pop-up) and `/IRT` lead.
type Ties = (lopdf::ObjectId, String, Vec<(String, lopdf::ObjectId)>);

/// The annotations of a page with what they lead to.
fn annotation_ties(doc: &Document, page: usize) -> Vec<Ties> {
    doc.annotations(page)
        .unwrap()
        .into_iter()
        .map(|a| {
            let id = a.id.unwrap();
            let d = doc.inner().get_dictionary(id).unwrap();
            let ties = [&b"P"[..], b"Popup", b"IRT"]
                .into_iter()
                .chain((a.subtype == "Popup").then_some(&b"Parent"[..]))
                .filter_map(|k| {
                    let r = d.get(k).ok()?.as_reference().ok()?;
                    Some((String::from_utf8_lossy(k).to_string(), r))
                })
                .collect();
            (id, a.subtype, ties)
        })
        .collect()
}

#[test]
fn a_duplicated_page_has_its_own_comments() {
    let mut doc = open(relations_doc());
    doc.duplicate_pages(&[1]).unwrap();
    let doc = reopened(&doc);
    assert_eq!(doc.page_count(), 5);
    let copy = doc.page_ids()[2];
    let mine: Vec<lopdf::ObjectId> = annotation_ties(&doc, 2).iter().map(|a| a.0).collect();
    let theirs: Vec<lopdf::ObjectId> = annotation_ties(&doc, 1).iter().map(|a| a.0).collect();
    for (id, kind, ties) in annotation_ties(&doc, 2) {
        assert!(!theirs.contains(&id), "{kind} shared with the original");
        for (key, to) in ties {
            match key.as_str() {
                "P" => assert_eq!(to, copy, "{kind} /P"),
                _ => assert!(mine.contains(&to), "{kind} /{key} leads to the original page"),
            }
        }
    }
    let replies = doc.annotations(2).unwrap().iter().filter(|a| a.is_reply).count();
    assert_eq!(replies, 1);
    // Comment names stay unique.
    let names: Vec<String> = (0..5)
        .flat_map(|p| doc.annotations(p).unwrap())
        .filter_map(|a| {
            let d = doc.inner().get_dictionary(a.id?).ok()?;
            Some(String::from_utf8_lossy(d.get(b"NM").ok()?.as_str().ok()?).to_string())
        })
        .collect();
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(names.len(), unique.len(), "{names:?}");
    // The radio buttons of the copy belong to the same group.
    assert_eq!(fields(&doc)[2], ("choice".into(), vec![1, 1, 2, 2]));
}

#[test]
fn the_fields_of_a_duplicated_page_are_the_same_fields() {
    let mut doc = open(relations_doc());
    doc.duplicate_pages(&[0]).unwrap();
    for doc in [&doc, &reopened(&doc)] {
        assert_eq!(
            fields(doc),
            [
                ("name".into(), vec![0, 1]),
                // The widget of the copy joins after the others.
                ("shared".into(), vec![0, 3, 1]),
                ("choice".into(), vec![2, 2]),
                ("notes".into(), vec![4]),
            ]
        );
        let name = doc.form_fields().into_iter().find(|f| f.name == "name").unwrap();
        assert_eq!(name.value, "Ada");
        // The copy is not tagged in the place of the original.
        let page = doc.inner().get_dictionary(doc.page_ids()[1]).unwrap();
        assert!(!page.has(b"StructParents"));
    }
    // A value set once shows on both pages.
    let id = doc
        .form_fields()
        .into_iter()
        .find(|f| f.name == "name")
        .unwrap()
        .id;
    doc.set_field_value(id, "Grace").unwrap();
    let doc = reopened(&doc);
    assert_eq!(doc.form_fields()[0].value, "Grace");
    assert_eq!(doc.form_fields()[0].widgets.len(), 2);
}
