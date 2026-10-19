//! Copies prepared to be sent: the chosen pages leave nothing of the others,
//! comments are kept, flattened or removed, protection and signatures are
//! handled, and the open document never changes. Every copy is reopened.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use folitecta_core::digsig::{DigitalId, IdentityInfo, KeyKind, SignOptions};
use folitecta_core::{
    CopyComments, CopyOptions, CopySignatures, Document, Error, FieldProps, LeftOut, LinkTarget, NewField,
    SecurityPermissions, TextLayout,
};
use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter, Rc4CryptFilter};
use lopdf::{Dictionary, EncryptionState, EncryptionVersion, Object, Stream, dictionary};

/// `needle` in the raw bytes, in any stream (decoded) or in any string of the file.
fn contains_anywhere(bytes: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    let has = |b: &[u8]| b.windows(n.len()).any(|w| w == n);
    if has(bytes) {
        return true;
    }
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    fn strings(o: &Object, f: &dyn Fn(&[u8]) -> bool) -> bool {
        match o {
            Object::String(s, _) | Object::Name(s) => f(s),
            Object::Array(a) => a.iter().any(|i| strings(i, f)),
            Object::Dictionary(d) => d.iter().any(|(k, v)| f(k) || strings(v, f)),
            Object::Stream(s) => {
                s.dict.iter().any(|(k, v)| f(k) || strings(v, f))
                    || f(&s.decompressed_content().unwrap_or_else(|_| s.content.clone()))
            }
            _ => false,
        }
    }
    doc.objects.values().any(|o| strings(o, &has))
}

fn reopen(bytes: &[u8]) -> Document {
    Document::from_bytes(bytes.to_vec(), None, None).unwrap()
}

/// Four pages with marker words, bookmarks, links, named destinations, form
/// fields, comments, page labels and tags.
fn fixture() -> Document {
    let mut doc = Document::from_text(
        "MARK-ALPHA first\u{c}MARK-BRAVO second\u{c}MARK-CHARLIE third\u{c}MARK-DELTA fourth",
        TextLayout::default(),
    )
    .unwrap();
    assert_eq!(doc.page_count(), 4);
    for p in 0..4 {
        doc.add_bookmark(&format!("Chapter {}", p + 1), p, &[]).unwrap();
    }
    doc.add_page_link(0, [20.0, 20.0, 120.0, 40.0], 2).unwrap();
    doc.add_page_link(0, [20.0, 50.0, 120.0, 70.0], 1).unwrap();
    doc.add_named_destination("annex", 3, None).unwrap();
    doc.add_named_destination("intro", 0, None).unwrap();
    for (page, name) in [(0, "name"), (3, "salary")] {
        let props = FieldProps {
            name: name.into(),
            ..FieldProps::default()
        };
        doc.add_form_field(page, NewField::Text, [300.0, 100.0, 450.0, 120.0], &props)
            .unwrap();
    }
    doc.set_field_values(&[("salary".into(), "FIELD-SECRET".into())])
        .unwrap();
    doc.add_free_text(
        1,
        [50.0, 300.0, 300.0, 340.0],
        "SECRET-REMARK",
        "Reviewer",
        12.0,
        [0.0, 0.0, 0.0],
        true,
    )
    .unwrap();
    doc.add_note(0, 30.0, 500.0, "first note", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    doc.set_page_labels(&[(0, "r", "", 1), (2, "D", "", 1)]).unwrap();
    doc.auto_tag().unwrap();
    doc
}

fn some_pages(pages: &[usize], comments: CopyComments) -> CopyOptions {
    CopyOptions {
        pages: Some(pages.to_vec()),
        comments,
    }
}

/// Every object reference of the tags names an annotation that is on a page.
fn tags_point_at_annotations_on_pages(doc: &Document) -> bool {
    let inner = doc.inner();
    let mut on_pages = HashSet::new();
    for pid in doc.page_ids() {
        if let Ok(arr) = inner
            .get_dictionary(pid)
            .unwrap()
            .get(b"Annots")
            .and_then(|a| a.as_array())
        {
            on_pages.extend(arr.iter().filter_map(|a| a.as_reference().ok()));
        }
    }
    let objr: Vec<&Dictionary> = inner
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .flat_map(|d| {
            let mut out = Vec::new();
            if d.has_type(b"OBJR") {
                out.push(d);
            }
            if let Ok(k) = d.get(b"K").and_then(|k| k.as_array()) {
                out.extend(
                    k.iter()
                        .filter_map(|o| o.as_dict().ok())
                        .filter(|o| o.has_type(b"OBJR")),
                );
            }
            out
        })
        .collect();
    objr.iter().all(|o| {
        o.get(b"Obj")
            .and_then(|r| r.as_reference())
            .is_ok_and(|r| inner.objects.contains_key(&r) && on_pages.contains(&r))
    })
}

/// The keys of the pages and annotations are in the parent tree, and what it
/// names exists.
fn parent_tree_is_consistent(doc: &Document) -> bool {
    let inner = doc.inner();
    let Ok(root) = inner
        .catalog()
        .unwrap()
        .get(b"StructTreeRoot")
        .and_then(|r| r.as_reference())
    else {
        return true;
    };
    let tree = inner
        .get_dictionary(root)
        .unwrap()
        .get(b"ParentTree")
        .unwrap()
        .as_reference()
        .unwrap();
    let nums = inner
        .get_dictionary(tree)
        .unwrap()
        .get(b"Nums")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    let mut keys = HashSet::new();
    for pair in nums.chunks(2) {
        keys.insert(pair[0].as_i64().unwrap());
        let refs: Vec<Object> = match &pair[1] {
            Object::Array(a) => a.clone(),
            o => vec![o.clone()],
        };
        for r in refs {
            if let Object::Reference(r) = r {
                if !inner.objects.contains_key(&r) {
                    return false;
                }
            }
        }
    }
    for pid in doc.page_ids() {
        let page = inner.get_dictionary(pid).unwrap();
        if let Ok(k) = page.get(b"StructParents").and_then(|k| k.as_i64()) {
            if !keys.contains(&k) {
                return false;
            }
        }
        for a in page
            .get(b"Annots")
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default()
        {
            let Ok(d) = inner.get_dictionary(a.as_reference().unwrap()) else {
                return false;
            };
            if let Ok(k) = d.get(b"StructParent").and_then(|k| k.as_i64()) {
                if !keys.contains(&k) {
                    return false;
                }
            }
        }
    }
    true
}

#[test]
fn a_copy_of_some_pages_leaves_the_others_out() {
    let doc = fixture();
    let original_content: Vec<(usize, i64)> = doc
        .structure()
        .unwrap()
        .content()
        .into_iter()
        .filter(|(p, _)| *p < 2)
        .collect();
    let copy = doc
        .prepare_copy(&some_pages(&[0, 1], CopyComments::Keep))
        .unwrap();
    let back = reopen(&copy.bytes);
    assert_eq!(back.page_count(), 2);
    assert_eq!(copy.facts.pages, 2);
    for gone in [
        "MARK-CHARLIE",
        "MARK-DELTA",
        "FIELD-SECRET",
        "Chapter 3",
        "Chapter 4",
        "annex",
    ] {
        assert!(
            !contains_anywhere(&copy.bytes, gone),
            "{gone} is still in the copy"
        );
    }
    for kept in ["MARK-ALPHA", "MARK-BRAVO", "SECRET-REMARK"] {
        assert!(contains_anywhere(&copy.bytes, kept), "{kept} is missing");
    }
    assert_eq!(
        back.outline_flat(),
        vec![
            (0, "Chapter 1".to_string(), Some(0)),
            (0, "Chapter 2".to_string(), Some(1))
        ]
    );
    let links = back.links(0).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, LinkTarget::Page(1));
    let dests: Vec<String> = back.named_destinations().into_iter().map(|d| d.name).collect();
    assert_eq!(dests, ["intro"]);
    let fields: Vec<String> = back.form_fields().into_iter().map(|f| f.name).collect();
    assert_eq!(fields, ["name"]);
    assert_eq!(back.page_labels(), ["i", "ii"]);
    // The tags of the kept pages are the ones they had.
    let content = back.structure().unwrap().content();
    assert_eq!(content, original_content);
    assert!(tags_point_at_annotations_on_pages(&back));
    assert!(parent_tree_is_consistent(&back));
    let left = copy.facts.left_out;
    assert_eq!(
        (
            left.bookmarks,
            left.links,
            left.destinations,
            left.fields,
            left.threads
        ),
        (2, 1, 1, 1, false)
    );
    assert!(left.tags >= 2, "{left:?}");
    // The plain copy shows the same pages.
    assert_eq!(reopen(&copy.plain).page_count(), 2);
}

#[test]
fn page_labels_of_a_subset_stay_as_they_were_shown() {
    let doc = fixture();
    let copy = doc
        .prepare_copy(&some_pages(&[1, 3], CopyComments::Keep))
        .unwrap();
    assert_eq!(reopen(&copy.bytes).page_labels(), ["ii", "2"]);
    assert!(!contains_anywhere(&copy.bytes, "MARK-ALPHA"));
    assert!(!contains_anywhere(&copy.bytes, "MARK-CHARLIE"));
}

/// Four pages sharing their resources (on the page tree, or one dictionary
/// for every page), each drawing a form and a picture of its own; the form of
/// page 2 has no resources and uses those of the page.
fn shared_resources(on_tree: bool) -> Document {
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let mut xobjects = Dictionary::new();
    let shared = doc.new_object_id();
    for k in 0..4 {
        let mut form = dictionary! {
            "Type" => "XObject", "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 200.into(), 50.into()],
        };
        if k == 0 {
            // Its own resources are the shared dictionary, as many producers write it.
            form.set("Resources", Object::Reference(shared));
        } else if k != 1 {
            form.set(
                "Resources",
                dictionary! { "Font" => dictionary! { "F1" => font } },
            );
        }
        let text = format!("BT /F1 12 Tf 10 10 Td (FORM-MARK-{k}) Tj ET");
        let fid = doc.add_object(Stream::new(form, text.into_bytes()));
        xobjects.set(format!("Fm{k}"), fid);
        let image = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 10, "Height" => 1,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
            },
            format!("IMG-MARK-{k}").into_bytes(),
        );
        let iid = doc.add_object(image);
        xobjects.set(format!("Im{k}"), iid);
    }
    doc.objects.insert(
        shared,
        Object::Dictionary(dictionary! {
            "Font" => dictionary! { "F1" => font },
            "XObject" => xobjects,
            "ProcSet" => vec!["PDF".into(), "Text".into(), "ImageB".into()],
        }),
    );
    let mut kids = Vec::new();
    for k in 0..4 {
        let content = format!("q /Fm{k} Do Q q 10 0 0 10 0 0 cm /Im{k} Do Q");
        let cid = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let mut page = dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => cid,
            "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
        };
        if !on_tree {
            page.set("Resources", Object::Reference(shared));
        }
        kids.push(Object::Reference(doc.add_object(page)));
    }
    let mut tree = dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => 4 };
    if on_tree {
        tree.set("Resources", Object::Reference(shared));
    }
    doc.objects.insert(pages_id, Object::Dictionary(tree));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    Document::from_bytes(bytes, None, None).unwrap()
}

#[test]
fn pictures_and_drawings_only_the_other_pages_use_leave_the_copy() {
    for on_tree in [true, false] {
        let doc = shared_resources(on_tree);
        let copy = doc
            .prepare_copy(&some_pages(&[0, 1], CopyComments::Keep))
            .unwrap();
        for k in 0..4 {
            let want = k < 2;
            for marker in [format!("FORM-MARK-{k}"), format!("IMG-MARK-{k}")] {
                assert_eq!(
                    contains_anywhere(&copy.bytes, &marker),
                    want,
                    "{marker} (resources on the page tree: {on_tree})"
                );
            }
        }
        assert_eq!(copy.facts.left_out.resources, 4, "{on_tree}");
        // The kept pages still find what they draw.
        let back = reopen(&copy.bytes);
        let inner = back.inner();
        for (p, pid) in back.page_ids().into_iter().enumerate() {
            let res = inner.get_dictionary(pid).unwrap().get(b"Resources").unwrap();
            let res = match res {
                Object::Reference(r) => inner.get_dictionary(*r).unwrap(),
                o => o.as_dict().unwrap(),
            };
            let xo = res.get(b"XObject").unwrap().as_dict().unwrap();
            let mut names: Vec<String> = xo
                .iter()
                .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                .collect();
            names.sort();
            assert_eq!(names, [format!("Fm{p}"), format!("Im{p}")]);
            // The form of page 2 has no resources of its own: it draws with the
            // font of the page. That of page 1 has its own, cut to the font.
            assert_eq!(res.get(b"Font").is_ok(), p == 1);
            if p == 0 {
                let form = xo.get(b"Fm0").unwrap().as_reference().unwrap();
                let own = inner.get_object(form).unwrap().as_stream().unwrap();
                let own = own.dict.get(b"Resources").unwrap().as_dict().unwrap();
                assert!(own.get(b"Font").is_ok() && own.get(b"XObject").is_err());
            }
        }
        folitecta_core::Renderer::new(Arc::new(copy.bytes.clone()))
            .unwrap()
            .render_page(1, 0.5)
            .unwrap();
    }
}

#[test]
fn comments_are_kept_flattened_or_removed() {
    let mut doc = fixture();
    // An annotation that is not a comment stays whatever happens to the comments.
    let page = doc.page_id(0).unwrap();
    let mark = doc.inner_mut().add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Watermark",
        "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
    });
    let annots = doc
        .inner()
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .clone();
    let mut list = match annots {
        Object::Array(a) => a,
        Object::Reference(r) => doc.inner().get_object(r).unwrap().as_array().unwrap().clone(),
        _ => Vec::new(),
    };
    list.push(Object::Reference(mark));
    doc.inner_mut()
        .get_dictionary_mut(page)
        .unwrap()
        .set("Annots", list);
    doc.commit().unwrap();
    let watermarks = |d: &Document| {
        d.annotations(0)
            .unwrap()
            .iter()
            .filter(|a| a.subtype == "Watermark")
            .count()
    };

    let removed = doc
        .prepare_copy(&CopyOptions {
            pages: None,
            comments: CopyComments::Remove,
        })
        .unwrap();
    let back = reopen(&removed.bytes);
    assert!(back.comments().is_empty());
    assert_eq!(removed.facts.comments, 0);
    assert!(!contains_anywhere(&removed.bytes, "SECRET-REMARK"));
    assert!(!contains_anywhere(&removed.bytes, "first note"));
    assert_eq!(watermarks(&back), 1);
    assert!(tags_point_at_annotations_on_pages(&back));
    assert!(parent_tree_is_consistent(&back));
    // The tagged annotations are the ones still on the pages.
    let tagged: HashSet<(usize, usize)> = back.structure().unwrap().annotations().into_iter().collect();
    let links: HashSet<(usize, usize)> = (0..back.page_count())
        .flat_map(|p| back.links(p).unwrap().into_iter().map(move |l| (p, l.index)))
        .collect();
    assert!(links.is_subset(&tagged), "{links:?} {tagged:?}");

    let flat = doc
        .prepare_copy(&CopyOptions {
            pages: None,
            comments: CopyComments::Flatten,
        })
        .unwrap();
    let back = reopen(&flat.bytes);
    assert!(back.comments().is_empty());
    assert!(back.marks_on_page(1).iter().any(|m| m == "Flatten"));
    assert!(contains_anywhere(&flat.bytes, "SECRET-REMARK"));
    assert_eq!(watermarks(&back), 1);
    assert!(tags_point_at_annotations_on_pages(&back));
    assert!(parent_tree_is_consistent(&back));

    let kept = doc.prepare_copy(&CopyOptions::default()).unwrap();
    assert_eq!(reopen(&kept.bytes).comments().len(), 2);
    assert_eq!(kept.facts.comments, 2);
}

fn no_printing() -> SecurityPermissions {
    SecurityPermissions {
        print: false,
        ..SecurityPermissions::default()
    }
}

/// The `/P` value of an encrypted file (its encryption state when it opens
/// without a password).
fn permissions_of(bytes: &[u8]) -> i64 {
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    let dict = match &doc.encryption_state {
        Some(state) => state.encode().ok(),
        None => doc.get_encrypted().ok().cloned(),
    };
    dict.and_then(|d| d.get(b"P").ok().and_then(|p| p.as_i64().ok()))
        .unwrap_or(0)
}

/// `doc` written encrypted by one of the older or newer security handlers.
fn encrypted_with(doc: &Document, aes256: bool) -> Vec<u8> {
    let mut copy = lopdf::Document::load_mem(doc.bytes().as_slice()).unwrap();
    copy.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(b"0123456789abcdef".to_vec(), lopdf::StringFormat::Hexadecimal),
            Object::String(b"0123456789abcdef".to_vec(), lopdf::StringFormat::Hexadecimal),
        ]),
    );
    let permissions = lopdf::Permissions::all() - lopdf::Permissions::PRINTABLE;
    let state = if aes256 {
        let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
        filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));
        EncryptionState::try_from(EncryptionVersion::V5 {
            encrypt_metadata: true,
            crypt_filters: filters,
            file_encryption_key: &[7u8; 32],
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "owner-pw",
            user_password: "user-pw",
            permissions,
        })
        .unwrap()
    } else {
        let _rc4: Arc<dyn CryptFilter> = Arc::new(Rc4CryptFilter);
        EncryptionState::try_from(EncryptionVersion::V2 {
            document: &copy,
            owner_password: "owner-pw",
            user_password: "user-pw",
            key_length: 128,
            permissions,
        })
        .unwrap()
    };
    copy.encrypt(&state).unwrap();
    let mut out = Vec::new();
    copy.save_to(&mut out).unwrap();
    out
}

#[test]
fn a_protected_document_gives_protected_copies() {
    let dir = std::env::temp_dir().join(format!("folitecta-delivery-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut plain = Document::from_text(
        "MARK-ONE page\u{c}MARK-TWO page\u{c}MARK-THREE page",
        TextLayout::default(),
    )
    .unwrap();
    plain
        .add_note(1, 30.0, 500.0, "NOTE-TEXT", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    let aes128 = dir.join("aes128.pdf");
    plain
        .save_encrypted(&aes128, "user-pw", "owner-pw", no_printing())
        .unwrap();
    let files = [
        ("AES-128", std::fs::read(&aes128).unwrap()),
        ("RC4", encrypted_with(&plain, false)),
        ("AES-256", encrypted_with(&plain, true)),
    ];
    for (kind, file) in files {
        let original_p = permissions_of(&file);
        let doc = Document::from_bytes(file, None, Some("user-pw")).unwrap();
        assert!(doc.is_protected(), "{kind}");
        let copy = doc
            .prepare_copy(&some_pages(&[0, 2], CopyComments::Remove))
            .unwrap();
        assert!(
            matches!(
                Document::from_bytes(copy.bytes.clone(), None, None),
                Err(Error::PasswordRequired)
            ),
            "{kind}: the copy opens without a password"
        );
        for pw in ["user-pw", "owner-pw"] {
            let back = Document::from_bytes(copy.bytes.clone(), None, Some(pw)).unwrap();
            assert_eq!(back.page_count(), 2, "{kind} {pw}");
            assert!(back.comments().is_empty(), "{kind} {pw}");
        }
        let back = Document::from_bytes(copy.bytes.clone(), None, Some("user-pw")).unwrap();
        assert!(back.page_text(1).contains("MARK-THREE"), "{kind}");
        assert_eq!(permissions_of(&copy.bytes), original_p, "{kind}");
        assert!(copy.facts.protected);
        let shown = reopen(&copy.plain);
        assert_eq!(shown.page_count(), 2);
        // Nothing readable of the pages in the file itself.
        assert!(!copy.bytes.windows(8).any(|w| w == b"MARK-ONE"), "{kind}");
    }
    // Restrictions without an open password: the copy keeps its encryption too.
    let owner_only = dir.join("owner-only.pdf");
    plain
        .save_encrypted(&owner_only, "", "owner-pw", no_printing())
        .unwrap();
    let doc = Document::open(&owner_only).unwrap();
    assert!(doc.is_protected());
    let copy = doc.prepare_copy(&some_pages(&[1], CopyComments::Keep)).unwrap();
    assert!(copy.bytes.windows(8).any(|w| w == b"/Encrypt"));
    assert_eq!(
        permissions_of(&copy.bytes),
        permissions_of(&std::fs::read(&owner_only).unwrap())
    );
    assert!(copy.facts.protected);
    let _ = std::fs::remove_dir_all(&dir);
}

fn signer() -> DigitalId {
    DigitalId::create_self_signed(
        &IdentityInfo {
            name: "Reviewer".into(),
            organization: "Example".into(),
            unit: String::new(),
            email: String::new(),
            country: "IT".into(),
        },
        KeyKind::EcP256,
        3,
    )
    .unwrap()
}

#[test]
fn signatures_stay_only_on_the_unchanged_signed_file() {
    let mut blank = Document::new_blank(2, 300.0, 400.0).unwrap();
    blank
        .add_note(0, 30.0, 370.0, "draft", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    let signed = blank.sign(&signer(), &SignOptions::default()).unwrap();
    let doc = reopen(&signed);

    let same = doc.prepare_copy(&CopyOptions::default()).unwrap();
    assert_eq!(
        same.facts.signatures,
        CopySignatures::Kept {
            count: 1,
            intact: Some(1)
        }
    );
    assert_eq!(same.bytes, signed);
    assert!(reopen(&same.bytes).validate_signatures(&[])[0].valid);

    for opts in [
        some_pages(&[0], CopyComments::Keep),
        CopyOptions {
            pages: None,
            comments: CopyComments::Remove,
        },
    ] {
        let changed = doc.prepare_copy(&opts).unwrap();
        assert_eq!(changed.facts.signatures, CopySignatures::Removed(1));
        assert!(!reopen(&changed.bytes).has_signatures());
        assert!(!contains_anywhere(&changed.bytes, "/ByteRange"));
        assert!(!contains_anywhere(&changed.bytes, "ByteRange"));
    }

    // Changed after signing: an incremental update that keeps the signature.
    let mut dirty = reopen(&signed);
    dirty
        .add_note(1, 30.0, 370.0, "later", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    let update = dirty.prepare_copy(&CopyOptions::default()).unwrap();
    assert!(update.bytes.starts_with(&signed));
    assert_eq!(
        update.facts.signatures,
        CopySignatures::Kept {
            count: 1,
            intact: Some(1)
        }
    );
    assert_eq!(reopen(&update.bytes).comments().len(), 2);

    // Validation material of the signers goes with the signatures.
    let mut with_dss = reopen(&signed);
    let cert = with_dss
        .inner_mut()
        .add_object(Stream::new(dictionary! {}, b"CERT-DER-MARK".to_vec()));
    let catalog_id = with_dss
        .inner()
        .trailer
        .get(b"Root")
        .unwrap()
        .as_reference()
        .unwrap();
    with_dss
        .inner_mut()
        .get_dictionary_mut(catalog_id)
        .unwrap()
        .set("DSS", dictionary! { "Certs" => vec![Object::Reference(cert)] });
    with_dss.commit().unwrap();
    let cut = with_dss
        .prepare_copy(&some_pages(&[1], CopyComments::Keep))
        .unwrap();
    assert_eq!(cut.facts.signatures, CopySignatures::Removed(1));
    assert!(!contains_anywhere(&cut.bytes, "CERT-DER-MARK"));
    let back = reopen(&cut.bytes);
    let form = back
        .inner()
        .catalog()
        .unwrap()
        .get(b"AcroForm")
        .ok()
        .map(|f| match f {
            Object::Reference(r) => back.inner().get_dictionary(*r).unwrap().clone(),
            o => o.as_dict().unwrap().clone(),
        });
    assert!(form.is_none_or(|f| !f.has(b"SigFlags")));

    // Protected and signed: written whole and encrypted, so the signature goes.
    let mut prot = lopdf::Document::load_mem(&signed).unwrap();
    let state = {
        let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
        filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));
        EncryptionState::try_from(EncryptionVersion::V5 {
            encrypt_metadata: true,
            crypt_filters: filters,
            file_encryption_key: &[9u8; 32],
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "owner-pw",
            user_password: "user-pw",
            permissions: lopdf::Permissions::all(),
        })
        .unwrap()
    };
    prot.encrypt(&state).unwrap();
    let mut file = Vec::new();
    prot.save_to(&mut file).unwrap();
    let doc = Document::from_bytes(file.clone(), None, Some("user-pw")).unwrap();
    assert!(doc.has_signatures());
    let copy = doc.prepare_copy(&CopyOptions::default()).unwrap();
    assert_eq!(copy.facts.signatures, CopySignatures::Removed(1));
    assert!(copy.facts.protected);
    assert_eq!(permissions_of(&copy.bytes), permissions_of(&file));
    let back = Document::from_bytes(copy.bytes, None, Some("user-pw")).unwrap();
    assert!(!back.has_signatures());
}

#[test]
fn the_open_document_is_not_changed() {
    let doc = fixture();
    let bytes = doc.bytes();
    let (pages, comments, dirty, undo) = (
        doc.page_count(),
        doc.comments().len(),
        doc.is_dirty(),
        doc.can_undo(),
    );
    for opts in [
        some_pages(&[1], CopyComments::Flatten),
        some_pages(&[0, 3], CopyComments::Remove),
        CopyOptions::default(),
    ] {
        doc.prepare_copy(&opts).unwrap();
    }
    assert!(Arc::ptr_eq(&bytes, &doc.bytes()));
    assert_eq!(doc.page_count(), pages);
    assert_eq!(doc.comments().len(), comments);
    assert_eq!(doc.is_dirty(), dirty);
    assert_eq!(doc.can_undo(), undo);
}

#[test]
fn page_lists_are_checked() {
    let doc = fixture();
    let err = |p: &[usize]| doc.prepare_copy(&some_pages(p, CopyComments::Keep)).unwrap_err();
    assert!(matches!(err(&[]), Error::LastPage));
    assert!(matches!(err(&[9]), Error::InvalidPage(9)));
    assert!(matches!(err(&[1, 0]), Error::Invalid(_)));
    assert!(matches!(err(&[1, 1]), Error::Invalid(_)));
    let all = doc
        .prepare_copy(&some_pages(&[0, 1, 2, 3], CopyComments::Keep))
        .unwrap();
    assert_eq!(all.facts.left_out, LeftOut::default());
    assert_eq!(reopen(&all.bytes).bookmark_count(), 4);
}

#[test]
fn a_copy_without_changes_is_the_current_state() {
    let doc = fixture();
    let copy = doc.prepare_copy(&CopyOptions::default()).unwrap();
    let back = reopen(&copy.bytes);
    assert_eq!(back.page_count(), doc.page_count());
    assert_eq!(back.comments().len(), doc.comments().len());
    assert_eq!(back.form_fields().len(), doc.form_fields().len());
    assert_eq!(copy.facts.left_out, LeftOut::default());
    assert_eq!(copy.facts.signatures, CopySignatures::None);
    assert!(!copy.facts.protected);
}

#[test]
fn links_through_named_destinations_and_unusual_names_follow_their_pages() {
    let mut doc = fixture();
    doc.add_named_link(0, [200.0, 20.0, 260.0, 40.0], "annex")
        .unwrap();
    // A destination named in PDFDocEncoding (not UTF-8) on page 4.
    let target = doc.page_id(3).unwrap();
    let catalog_id = doc.inner().trailer.get(b"Root").unwrap().as_reference().unwrap();
    let mut dests = Dictionary::new();
    dests.set(
        b"caf\xe9".to_vec(),
        Object::Array(vec![Object::Reference(target), Object::Name(b"Fit".to_vec())]),
    );
    doc.inner_mut()
        .get_dictionary_mut(catalog_id)
        .unwrap()
        .set("Dests", dests);
    doc.commit().unwrap();
    assert_eq!(doc.named_destinations().len(), 3);
    let copy = doc
        .prepare_copy(&some_pages(&[0, 1], CopyComments::Keep))
        .unwrap();
    let back = reopen(&copy.bytes);
    let dests: Vec<String> = back.named_destinations().into_iter().map(|d| d.name).collect();
    assert_eq!(dests, ["intro"]);
    assert_eq!(copy.facts.left_out.destinations, 2);
    // Both links to page 3 and 4 went (one by page, one by name).
    assert_eq!(copy.facts.left_out.links, 2);
    assert_eq!(back.links(0).unwrap().len(), 1);
    assert!(tags_point_at_annotations_on_pages(&back));
}

#[test]
fn tags_of_the_other_pages_leave_the_id_tree_and_the_bookmarks() {
    let mut doc = fixture();
    let tree = doc.structure().unwrap();
    let mut on_page_4 = None;
    tree.walk(&mut |e, _| {
        if on_page_4.is_none() && e.page == Some(3) && !e.is_root() && !e.kids.is_empty() {
            on_page_4 = Some(e.id);
        }
    });
    let elem = on_page_4.expect("an element on page 4");
    let root = doc
        .inner()
        .catalog()
        .unwrap()
        .get(b"StructTreeRoot")
        .unwrap()
        .as_reference()
        .unwrap();
    let inner = doc.inner_mut();
    let e = inner.get_dictionary_mut(elem).unwrap();
    e.set("ID", Object::string_literal("elem-4"));
    e.set("Alt", Object::string_literal("ALT-SECRET"));
    let leaf = inner.add_object(dictionary! {
        "Names" => vec![Object::string_literal("elem-4"), Object::Reference(elem)],
    });
    inner
        .get_dictionary_mut(root)
        .unwrap()
        .set("IDTree", Object::Reference(leaf));
    // The first bookmark points at that element too.
    let first = inner
        .catalog()
        .unwrap()
        .get(b"Outlines")
        .unwrap()
        .as_reference()
        .unwrap();
    let first = inner
        .get_dictionary(first)
        .unwrap()
        .get(b"First")
        .unwrap()
        .as_reference()
        .unwrap();
    inner
        .get_dictionary_mut(first)
        .unwrap()
        .set("SE", Object::Reference(elem));
    doc.commit().unwrap();
    let copy = doc
        .prepare_copy(&some_pages(&[0, 1], CopyComments::Keep))
        .unwrap();
    assert!(!contains_anywhere(&copy.bytes, "ALT-SECRET"));
    assert!(!contains_anywhere(&copy.bytes, "elem-4"));
    let back = reopen(&copy.bytes);
    let titled: Vec<&Dictionary> = back
        .inner()
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.has(b"Title"))
        .collect();
    assert_eq!(titled.len(), 2);
    assert!(titled.iter().all(|d| !d.has(b"SE")));
}
