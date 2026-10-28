//! Pages taken from another document (Insert Pages from a file, Combine
//! Files) bring their form fields, named destinations, bookmarks, layers and
//! tags; names already used get a suffix and the renames are reported.
//! Checked on `testdocs::relations_doc` (see there what points where).

use folitecta_core::combine::Source;
use folitecta_core::testdocs::relations_doc;
use folitecta_core::{Arrival, Document, LinkTarget};

fn open(bytes: Vec<u8>) -> Document {
    Document::from_bytes(bytes, None, None).unwrap()
}

fn reopened(doc: &Document) -> Document {
    open(doc.bytes().to_vec())
}

fn field_values(doc: &Document) -> Vec<(String, String, Vec<usize>)> {
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
            (f.name, f.value, pages)
        })
        .collect()
}

fn renamed(names: &[&str]) -> Vec<(String, String)> {
    names.iter().map(|n| (n.to_string(), format!("{n}_2"))).collect()
}

#[test]
fn a_document_inserted_into_itself_keeps_two_of_everything() {
    let mut doc = open(relations_doc());
    let other = open(relations_doc());
    let preview = doc.inserting_pages_brings(&other, None).unwrap();
    let arrival = doc.insert_pages_from(4, &other, None, None).unwrap();
    assert_eq!(preview, arrival);
    assert_eq!(
        arrival,
        Arrival {
            renamed_fields: renamed(&["name", "shared", "choice", "notes"]),
            renamed_destinations: renamed(&["first", "second", "third", "fourth"]),
            ..Arrival::default()
        }
    );
    for doc in [&doc, &reopened(&doc)] {
        assert_eq!(doc.page_count(), 8);
        let fields = field_values(doc);
        assert_eq!(fields.len(), 8);
        assert_eq!(fields[4], ("name_2".into(), "Ada".into(), vec![4]));
        assert_eq!(fields[5], ("shared_2".into(), "Both pages".into(), vec![4, 6]));
        assert_eq!(fields[6].0, "choice_2");
        assert_eq!(fields[6].1, "b");
        assert_eq!(fields[6].2, vec![5, 5]);
        // The links of the pages that came lead among them.
        let links: Vec<LinkTarget> = (4..8).map(|p| doc.links(p).unwrap().remove(0).target).collect();
        assert_eq!(
            links,
            [
                LinkTarget::Page(6),
                LinkTarget::Page(7),
                LinkTarget::Page(5),
                LinkTarget::Uri("https://example.org/".into())
            ]
        );
        let dests: Vec<(String, Option<usize>)> = doc
            .named_destinations()
            .into_iter()
            .map(|d| (d.name, d.page))
            .collect();
        assert!(dests.contains(&("third_2".into(), Some(6))), "{dests:?}");
        assert!(dests.contains(&("third".into(), Some(2))), "{dests:?}");
        // The bookmarks of both, those that came after ours.
        let tops: Vec<(String, Option<usize>)> =
            doc.outline().into_iter().map(|i| (i.title, i.page)).collect();
        assert_eq!(
            tops,
            [
                ("Part A".into(), Some(0)),
                ("Part B".into(), Some(2)),
                ("Part A".into(), Some(4)),
                ("Part B".into(), Some(6)),
            ]
        );
        assert_eq!(doc.outline()[3].children[0].page, Some(7));
    }
    // One undo step.
    doc.undo().unwrap();
    assert_eq!(doc.page_count(), 4);
    assert_eq!(doc.form_fields().len(), 4);
}

#[test]
fn some_pages_taken_leave_the_rest_behind() {
    let mut doc = Document::new_blank(2, 595.0, 842.0).unwrap();
    let other = open(relations_doc());
    // Pages 4 and 3 of the source, in that order, between the two pages.
    let arrival = doc.insert_pages_from(1, &other, Some(&[3, 2]), None).unwrap();
    assert!(arrival.renamed_fields.is_empty());
    // "Part A" with its children, `first` and `second`, the fields `name`
    // and `choice`, the link of page 3 (to page 2)…
    assert_eq!(arrival.left_out.fields, 2);
    assert_eq!(arrival.left_out.destinations, 2);
    assert!(arrival.left_out.bookmarks >= 3, "{:?}", arrival.left_out);
    assert!(arrival.left_out.open_action);
    let doc = reopened(&doc);
    assert_eq!(doc.page_count(), 4);
    assert!(doc.page_text(1).contains("Marker of page 4"));
    assert!(doc.page_text(2).contains("Marker of page 3"));
    assert_eq!(
        field_values(&doc),
        [
            ("shared".into(), "Both pages".into(), vec![2]),
            ("notes".into(), "Last page".into(), vec![1]),
        ]
    );
    assert_eq!(
        doc.outline_flat(),
        [(0, "Part B".into(), Some(2)), (1, "Page 4".into(), Some(1))]
    );
    // Nothing of the pages not taken came in.
    let file = lopdf::Document::load_mem(doc.bytes().as_slice()).unwrap();
    for n in [1, 2] {
        let marker = format!("Marker of page {n}");
        assert!(
            !file.objects.values().any(|o| o.as_stream().ok().is_some_and(|s| {
                let data = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
                data.windows(marker.len()).any(|w| w == marker.as_bytes())
            }))
        );
    }
}

#[test]
fn tags_come_into_a_tagged_document_and_are_counted_otherwise() {
    let mut tagged_source = open(relations_doc());
    tagged_source.auto_tag().unwrap();
    let count = |doc: &Document| {
        let mut n = 0;
        if let Some(s) = doc.structure() {
            s.walk(&mut |e, _| n += usize::from(!e.is_root() && e.page.is_some()));
        }
        n
    };
    let theirs = count(&tagged_source);
    assert!(theirs > 0);

    let mut tagged = open(relations_doc());
    tagged.auto_tag().unwrap();
    let ours = count(&tagged);
    let arrival = tagged.insert_pages_from(0, &tagged_source, None, None).unwrap();
    assert_eq!(arrival.tags_left, 0);
    let tagged = reopened(&tagged);
    assert_eq!(count(&tagged), ours + theirs);
    // Every page has its tags, and the parent tree knows them.
    let mut pages = std::collections::BTreeSet::new();
    tagged.structure().unwrap().walk(&mut |e, _| {
        if let Some(p) = e.page {
            pages.insert(p);
        }
    });
    assert_eq!(pages.into_iter().collect::<Vec<_>>(), (0..8).collect::<Vec<_>>());
    // The checks of tagged content find the same as on the source alone.
    let statuses = |doc: &Document| {
        doc.accessibility_check()
            .into_iter()
            .filter(|r| ["tagged_content", "tagged_annotations", "tagged_fields"].contains(&r.rule))
            .map(|r| (r.rule, r.status))
            .collect::<Vec<_>>()
    };
    assert_eq!(statuses(&tagged), statuses(&tagged_source));

    let mut plain = open(relations_doc());
    let arrival = plain.insert_pages_from(4, &tagged_source, None, None).unwrap();
    assert!(arrival.tags_left > 0);
    assert!(!plain.has_structure());
}

#[test]
fn layers_come_with_their_pages() {
    let mut other = open(relations_doc());
    other.add_layer("Plan", &[]).unwrap();
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    doc.insert_pages_from(1, &other, None, None).unwrap();
    let doc = reopened(&doc);
    assert_eq!(
        doc.layers().into_iter().map(|l| l.name).collect::<Vec<_>>(),
        ["Plan"]
    );
}

#[test]
fn combined_files_get_a_bookmark_each_and_their_renames() {
    let dir = std::env::temp_dir().join(format!("folitecta-incoming-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("first.pdf");
    let b = dir.join("second.pdf");
    std::fs::write(&a, relations_doc()).unwrap();
    std::fs::write(&b, relations_doc()).unwrap();
    let (doc, problems, arrivals) = Document::combine_with_arrivals(&[
        Source {
            path: a.clone(),
            pages: None,
        },
        Source {
            path: b.clone(),
            pages: Some(vec![0, 1]),
        },
    ])
    .unwrap();
    assert!(problems.is_empty());
    assert_eq!(doc.page_count(), 6);
    assert!(arrivals[0].1.is_empty());
    assert_eq!(
        arrivals[1].1.renamed_fields,
        renamed(&["name", "shared", "choice"])
    );
    let doc = reopened(&doc);
    let tops: Vec<(String, Option<usize>, usize)> = doc
        .outline()
        .into_iter()
        .map(|i| (i.title, i.page, i.children.len()))
        .collect();
    assert_eq!(
        tops,
        [("first".into(), Some(0), 2), ("second".into(), Some(4), 1)]
    );
    assert_eq!(doc.form_fields().len(), 7);
    let _ = std::fs::remove_dir_all(&dir);
}
