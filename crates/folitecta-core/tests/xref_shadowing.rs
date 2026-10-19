//! Later revisions that change a signed page through the cross-reference
//! sections themselves (written byte by byte, as an attacker would): a
//! second entry pointing back at an old object header, a free entry, a
//! hybrid section whose cross-reference stream moves the page, an object
//! stream holding the page twice, another generation of the page content.
//! Readers that follow the sections draw the changed page; Folitecta must
//! report the change (and the certification problem), never "validation
//! material only". The same on a certified and on a protected document.

use std::sync::Arc;

use folitecta_core::Document;
use folitecta_core::digsig::{
    CertProfile, Certificate, Check, DigitalId, ExtendedUsage, IdentityInfo, KeyKind, RevocationPool,
    SignOptions, SignatureStatus, cert_to_der, timestamp,
};
use folitecta_core::render::{RenderOpts, Renderer};
use lopdf::{Object, ObjectId, Stream, dictionary};

const DAY: i64 = 86_400;
const SIGN: i64 = 1_792_324_800;
const NOW: i64 = SIGN + 2 * DAY;
const ORIGINAL: &[u8] = b"BT /F1 24 Tf 20 200 Td (ORIGINAL) Tj ET";
const HACKED: &[u8] = b"1 0 0 rg 0 0 300 150 re f BT /F1 24 Tf 20 200 Td (HACKED) Tj ET";
const USER: &str = "user-pw";

fn who(name: &str) -> IdentityInfo {
    IdentityInfo {
        name: name.into(),
        ..IdentityInfo::default()
    }
}

struct Fixture {
    signed: Vec<u8>,
    trusted: Vec<Certificate>,
    password: Option<&'static str>,
    der: Vec<u8>,
}

/// A page saying ORIGINAL, signed with a time stamp (certified with P=1 when
/// `certify`), protected with a password when `protect`.
fn fixture(certify: u8, protect: bool) -> Fixture {
    let ca = DigitalId::create_with(
        &who("Shadow Root CA"),
        KeyKind::EcP256,
        &CertProfile {
            not_before: SIGN - 400 * DAY,
            not_after: SIGN + 400 * DAY,
            ca: true,
            ..CertProfile::default()
        },
    )
    .unwrap();
    let mut s = ca
        .issue(
            &who("Shadow Signer"),
            KeyKind::EcP256,
            &CertProfile {
                not_before: SIGN - DAY,
                not_after: SIGN + 100 * DAY,
                ..CertProfile::default()
            },
        )
        .unwrap();
    s.chain.clear();
    let t = DigitalId::create_with(
        &who("Shadow TSA"),
        KeyKind::EcP256,
        &CertProfile {
            not_before: SIGN - 400 * DAY,
            not_after: SIGN + 400 * DAY,
            usages: vec![ExtendedUsage::TimeStamping],
            ..CertProfile::default()
        },
    )
    .unwrap();
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    let page = doc.page_id(0).unwrap();
    let content = doc
        .inner()
        .get_dictionary(page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.inner_mut().objects.insert(
        content,
        Object::Stream(Stream::new(dictionary! {}, ORIGINAL.to_vec())),
    );
    doc.inner_mut().get_dictionary_mut(page).unwrap().set(
        "Resources",
        dictionary! { "Font" => dictionary! { "F1" => dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" } } },
    );
    // An object nothing reaches (a number in use the document does not show).
    doc.inner_mut().add_object(dictionary! { "Unused" => true });
    doc.commit().unwrap();
    if protect {
        let dir = std::env::temp_dir().join(format!("folitecta-shadow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("protected-{certify}.pdf"));
        doc.save_encrypted(&path, USER, "owner-pw", Default::default(), Default::default())
            .unwrap();
        doc = Document::open_with_password(&path, USER).unwrap();
    }
    let opts = SignOptions {
        time: SIGN as u64,
        certify,
        ..SignOptions::default()
    };
    let mut send = |req: &[u8]| timestamp::respond(&t, req, SIGN as u64, 1);
    let signed = doc.sign_to_file(&s, &opts, Some(&mut send)).unwrap().bytes;
    Fixture {
        signed,
        trusted: vec![ca.certificate.clone(), t.certificate.clone()],
        password: protect.then_some(USER),
        der: cert_to_der(&ca.certificate),
    }
}

fn load(f: &Fixture, bytes: &[u8]) -> lopdf::Document {
    match f.password {
        Some(pw) => {
            lopdf::Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(pw)).unwrap()
        }
        None => lopdf::Document::load_mem(bytes).unwrap(),
    }
}

fn last_startxref(data: &[u8]) -> usize {
    let at = data.windows(9).rposition(|w| w == b"startxref").unwrap();
    let s: String = data[at + 9..]
        .iter()
        .skip_while(|c| c.is_ascii_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .map(|&c| c as char)
        .collect();
    s.parse().unwrap()
}

/// An object written in PDF syntax (strings in hexadecimal).
fn ser(o: &Object) -> String {
    match o {
        Object::Null => "null".into(),
        Object::Boolean(b) => b.to_string(),
        Object::Integer(i) => i.to_string(),
        Object::Real(r) => format!("{r}"),
        Object::Name(n) => format!("/{}", String::from_utf8_lossy(n)),
        Object::String(s, _) => format!("<{}>", s.iter().map(|b| format!("{b:02X}")).collect::<String>()),
        Object::Array(a) => format!("[{}]", a.iter().map(ser).collect::<Vec<_>>().join(" ")),
        Object::Dictionary(d) => format!(
            "<<{}>>",
            d.iter()
                .map(|(k, v)| format!("/{} {}", String::from_utf8_lossy(k), ser(v)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Object::Reference((n, g)) => format!("{n} {g} R"),
        Object::Stream(_) => unreachable!("streams are written by `stream_body`"),
    }
}

/// The body of a stream object of `id`, encrypted with the document key of
/// a protected fixture.
fn stream_body(enc: Option<&lopdf::EncryptionState>, id: ObjectId, dict: &str, content: &[u8]) -> Vec<u8> {
    let data = match enc {
        Some(state) => {
            let mut o = Object::Stream(Stream::new(dictionary! {}, content.to_vec()));
            lopdf::encryption::encrypt_object(state, id, &mut o).unwrap();
            o.as_stream().unwrap().content.clone()
        }
        None => content.to_vec(),
    };
    let mut b = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    b.extend_from_slice(&data);
    b.extend_from_slice(b"\nendstream");
    b
}

/// An update with a cross-reference table: `objs` written in order (number,
/// generation, body), entries for those in `listed` and the lines `extra`.
struct Update {
    objs: Vec<(u32, u16, Vec<u8>)>,
    extra: Vec<(u32, String)>,
    trailer_extra: String,
    size: u32,
    listed: Vec<u32>,
}

fn append(f: &Fixture, base: &[u8], u: &Update) -> (Vec<u8>, Vec<(u32, usize)>) {
    let prev = load(f, base);
    let root = prev.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let info = prev.trailer.get(b"Info").ok().and_then(|i| i.as_reference().ok());
    let encrypt = lopdf::Document::load_mem(base)
        .ok()
        .and_then(|d| d.trailer.get(b"Encrypt").ok().cloned());
    let prev_x = last_startxref(base);
    let mut out = base.to_vec();
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    let mut offs = Vec::new();
    for (n, g, body) in &u.objs {
        offs.push((*n, out.len()));
        out.extend_from_slice(format!("{n} {g} obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let mut entries: Vec<(u32, String)> = u.extra.clone();
    for (n, g, _) in &u.objs {
        if u.listed.contains(n) {
            let off = offs.iter().find(|(m, _)| m == n).unwrap().1;
            entries.push((*n, format!("{off:010} {g:05} n \n")));
        }
    }
    entries.sort_by_key(|e| e.0);
    let xref = out.len();
    out.extend_from_slice(b"xref\n");
    for (n, line) in &entries {
        out.extend_from_slice(format!("{n} 1\n{line}").as_bytes());
    }
    let info_s = info.map(|(n, g)| format!(" /Info {n} {g} R")).unwrap_or_default();
    let enc_s = encrypt
        .map(|e| format!(" /Encrypt {}", ser(&e)))
        .unwrap_or_default();
    let id_s = prev
        .trailer
        .get(b"ID")
        .ok()
        .map(|i| format!(" /ID {}", ser(i)))
        .unwrap_or_default();
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root {} {} R{info_s}{enc_s}{id_s} /Prev {prev_x}{} >>\nstartxref\n{xref}\n%%EOF\n",
            u.size, root.0, root.1, u.trailer_extra
        )
        .as_bytes(),
    );
    (out, offs)
}

fn first_signature(f: &Fixture, bytes: &[u8]) -> SignatureStatus {
    let doc = Document::from_bytes(bytes.to_vec(), None, f.password).unwrap();
    doc.validate_signatures_with(&f.trusted, &RevocationPool::default(), NOW)
        .into_iter()
        .find(|s| !s.document_time_stamp)
        .unwrap()
}

fn assert_changed(name: &str, f: &Fixture, bytes: &[u8]) {
    assert!(
        bytes.starts_with(&f.signed),
        "{name}: an update of the signed file"
    );
    let s = first_signature(f, bytes);
    assert!(s.valid, "{name}: {:?}", s.problems);
    assert!(
        !s.later_revisions_material,
        "{name}: taken for validation material"
    );
    assert_eq!(s.checks.integrity, Check::Warning, "{name}");
    let expected = if s.certify == Some(1) {
        "certification does not allow"
    } else {
        "(in a later revision)"
    };
    assert!(
        s.problems.iter().any(|p| p.contains(expected)),
        "{name}: {:?}",
        s.problems
    );
}

/// The page shows the change (the renderer follows the sections).
fn drawn_changed(bytes: &[u8]) -> bool {
    let r = Renderer::new(Arc::new(bytes.to_vec())).unwrap();
    let img = r.render_rgba(0, 1.0, RenderOpts::default()).unwrap();
    img.get_pixel(150, 250).0[1] < 100
}

fn page_and_content(prev: &lopdf::Document) -> (ObjectId, ObjectId) {
    let page = *prev.get_pages().values().next().unwrap();
    let content = prev
        .get_dictionary(page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_reference()
        .unwrap();
    (page, content)
}

fn run(certify: u8, protect: bool) {
    let f = fixture(certify, protect);
    let tag = format!("certify {certify}, protected {protect}");
    let prev = load(&f, &f.signed);
    let enc = prev.encryption_state.clone();
    let enc = enc.as_ref();
    let raw = lopdf::Document::load_mem(&f.signed).unwrap();
    let (page, c) = page_and_content(&prev);
    let base = f.signed.windows(5).position(|w| w == b"%PDF-").unwrap();
    let old_off = match raw.reference_table.get(c.0) {
        Some(lopdf::xref::XrefEntry::Normal { offset, .. }) => *offset as usize,
        other => panic!("content entry {other:?}"),
    };
    let size = raw.max_id.max(prev.max_id) + 10;

    // Control: a hand-written update that only adds a store is material.
    let dss = size + 3;
    let cert = size + 4;
    let mut cat = prev.get_dictionary(page_root(&prev)).unwrap().clone();
    cat.set("DSS", Object::Reference((dss, 0)));
    let (control, _) = append(
        &f,
        &f.signed,
        &Update {
            objs: vec![
                (cert, 0, stream_body(enc, (cert, 0), "", &f.der)),
                (dss, 0, format!("<< /Certs [{cert} 0 R] >>").into_bytes()),
                (page_root(&prev).0, 0, ser(&Object::Dictionary(cat)).into_bytes()),
            ],
            extra: vec![],
            trailer_extra: String::new(),
            size: cert + 1,
            listed: vec![cert, dss, page_root(&prev).0],
        },
    );
    let s = first_signature(&f, &control);
    assert!(s.later_revisions_material, "{tag}: control {:?}", s.problems);
    assert_eq!(s.checks.integrity, Check::Passed, "{tag}");

    // A1: the content redefined; a second, unused entry points back at the
    // old header of the content.
    let (a1, _) = append(
        &f,
        &f.signed,
        &Update {
            objs: vec![(c.0, c.1, stream_body(enc, c, "", HACKED))],
            extra: vec![(size - 1, format!("{:010} 00000 n \n", old_off - base))],
            trailer_extra: String::new(),
            size,
            listed: vec![c.0],
        },
    );
    assert_changed(&format!("A1 shadow entry, {tag}"), &f, &a1);
    if !protect {
        assert!(drawn_changed(&a1), "the attack changes the page");
    }

    // A2: the content freed.
    let (a2, _) = append(
        &f,
        &f.signed,
        &Update {
            objs: vec![],
            extra: vec![(c.0, format!("{:010} {:05} f \n", 0, c.1 + 1))],
            trailer_extra: String::new(),
            size,
            listed: vec![],
        },
    );
    assert_changed(&format!("A2 free entry, {tag}"), &f, &a2);

    // A3: a hybrid update whose cross-reference stream moves the page into
    // an object stream, with a new content stream.
    let (os, xs, nc) = (size, size + 1, size + 2);
    let mut pd = prev.get_dictionary(page).unwrap().clone();
    pd.set("Contents", Object::Reference((nc, 0)));
    let header = format!("{} 0 ", page.0);
    let osdata = format!("{header}{}", ser(&Object::Dictionary(pd)));
    let osbody = stream_body(
        enc,
        (os, 0),
        &format!("/Type /ObjStm /N 1 /First {}", header.len()),
        osdata.as_bytes(),
    );
    let mut xdata = vec![2u8];
    xdata.extend_from_slice(&os.to_be_bytes());
    xdata.extend_from_slice(&0u16.to_be_bytes());
    let mut xbody = format!(
        "<< /Type /XRef /Size {} /W [1 4 2] /Index [{} 1] /Length {} >>\nstream\n",
        nc + 1,
        page.0,
        xdata.len()
    )
    .into_bytes();
    xbody.extend_from_slice(&xdata);
    xbody.extend_from_slice(b"\nendstream");
    let make = |xs_off: usize| {
        append(
            &f,
            &f.signed,
            &Update {
                objs: vec![
                    (os, 0, osbody.clone()),
                    (nc, 0, stream_body(enc, (nc, 0), "", HACKED)),
                    (xs, 0, xbody.clone()),
                ],
                extra: vec![],
                trailer_extra: format!(" /XRefStm {xs_off}"),
                size: nc + 1,
                listed: vec![os, nc, xs],
            },
        )
    };
    let (_, offs) = make(0);
    let xs_off = offs.iter().find(|(n, _)| *n == xs).unwrap().1 - base;
    let (a3, _) = make(xs_off);
    assert_changed(&format!("A3 hybrid section, {tag}"), &f, &a3);
    if !protect {
        assert!(drawn_changed(&a3), "the attack changes the page");
    }

    // A4: a cross-reference stream update whose object stream holds the page
    // twice: at index 0 (named by the entry) changed, at index 1 the old copy.
    let old = prev.get_dictionary(page).unwrap().clone();
    let mut pd = old.clone();
    pd.set("Contents", Object::Reference((nc, 0)));
    let (newd, oldd) = (ser(&Object::Dictionary(pd)), ser(&Object::Dictionary(old)));
    let header = format!("{} 0 {} {} ", page.0, page.0, newd.len() + 1);
    let osdata = format!("{header}{newd} {oldd}");
    let root = prev.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let info = prev.trailer.get(b"Info").unwrap().as_reference().unwrap();
    let mut out = f.signed.clone();
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    let off_os = out.len() - base;
    out.extend_from_slice(format!("{os} 0 obj\n").as_bytes());
    out.extend_from_slice(&stream_body(
        enc,
        (os, 0),
        &format!("/Type /ObjStm /N 2 /First {}", header.len()),
        osdata.as_bytes(),
    ));
    out.extend_from_slice(b"\nendobj\n");
    let off_nc = out.len() - base;
    out.extend_from_slice(format!("{nc} 0 obj\n").as_bytes());
    out.extend_from_slice(&stream_body(enc, (nc, 0), "", HACKED));
    out.extend_from_slice(b"\nendobj\n");
    let off_xs = out.len() - base;
    let mut xd = Vec::new();
    let mut entry = |t: u8, a: u32, b: u16| {
        xd.push(t);
        xd.extend_from_slice(&a.to_be_bytes());
        xd.extend_from_slice(&b.to_be_bytes());
    };
    entry(2, os, 0);
    entry(1, off_os as u32, 0);
    entry(1, off_xs as u32, 0);
    entry(1, off_nc as u32, 0);
    let encrypt = raw
        .trailer
        .get(b"Encrypt")
        .ok()
        .map(|e| format!(" /Encrypt {}", ser(e)))
        .unwrap_or_default();
    let id = raw
        .trailer
        .get(b"ID")
        .ok()
        .map(|i| format!(" /ID {}", ser(i)))
        .unwrap_or_default();
    out.extend_from_slice(
        format!(
            "{xs} 0 obj\n<< /Type /XRef /Size {} /W [1 4 2] /Index [{} 1 {} 3] /Root {} 0 R /Info {} 0 R{encrypt}{id} /Prev {} /Length {} >>\nstream\n",
            nc + 1,
            page.0,
            os,
            root.0,
            info.0,
            last_startxref(&f.signed),
            xd.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&xd);
    out.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{off_xs}\n%%EOF\n").as_bytes());
    assert_changed(&format!("A4 object stream duplicate, {tag}"), &f, &out);
    if !protect {
        assert!(drawn_changed(&out), "the attack changes the page");
    }

    // A5: the content redefined under another generation.
    let (a5, _) = append(
        &f,
        &f.signed,
        &Update {
            objs: vec![(c.0, 1, stream_body(enc, (c.0, 1), "", HACKED))],
            extra: vec![],
            trailer_extra: String::new(),
            size,
            listed: vec![c.0],
        },
    );
    assert_changed(&format!("A5 generation, {tag}"), &f, &a5);
}

fn page_root(doc: &lopdf::Document) -> ObjectId {
    doc.trailer.get(b"Root").unwrap().as_reference().unwrap()
}

#[test]
fn cross_reference_shadowing_is_a_change() {
    run(0, false);
}

#[test]
fn cross_reference_shadowing_breaks_certification() {
    run(1, false);
}

#[test]
fn cross_reference_shadowing_of_protected_files_is_a_change() {
    run(0, true);
    run(1, true);
}

#[test]
fn many_end_markers_in_a_stream_are_read_quickly() {
    // A large signed file followed by one revision whose stream holds a
    // thousand end-of-file markers: one revision, read once.
    let ca = DigitalId::create_with(
        &who("Markers CA"),
        KeyKind::EcP256,
        &CertProfile {
            not_before: SIGN - 400 * DAY,
            not_after: SIGN + 400 * DAY,
            ..CertProfile::default()
        },
    )
    .unwrap();
    let mut doc = Document::new_blank(1, 100.0, 100.0).unwrap();
    for i in 0..20_000 {
        doc.inner_mut()
            .add_object(dictionary! { "K" => i as i64, "Pad" => Object::string_literal("x".repeat(80)) });
    }
    doc.commit().unwrap();
    let signed = doc
        .sign(
            &ca,
            &SignOptions {
                time: SIGN as u64,
                ..SignOptions::default()
            },
        )
        .unwrap();
    let prev = lopdf::Document::load_mem(&signed).unwrap();
    let root = prev.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let info = prev.trailer.get(b"Info").unwrap().as_reference().unwrap();
    let mut v = signed.clone();
    let num = prev.max_id + 1;
    let off = v.len();
    let body: Vec<u8> = b"%%EOF\n".repeat(1000);
    v.extend_from_slice(format!("{num} 0 obj\n<< /Length {} >>\nstream\n", body.len()).as_bytes());
    v.extend_from_slice(&body);
    v.extend_from_slice(b"\nendstream\nendobj\n");
    let x = v.len();
    v.extend_from_slice(
        format!(
            "xref\n{num} 1\n{off:010} 00000 n \ntrailer\n<< /Size {} /Root {} 0 R /Info {} 0 R /Prev {} >>\nstartxref\n{x}\n%%EOF\n",
            num + 1,
            root.0,
            info.0,
            last_startxref(&signed)
        )
        .as_bytes(),
    );
    let d = Document::from_bytes(v, None, None).unwrap();
    let started = std::time::Instant::now();
    let s = d.validate_signatures_with(
        std::slice::from_ref(&ca.certificate),
        &RevocationPool::default(),
        NOW,
    );
    let took = started.elapsed();
    // The object nothing reaches adds no material and changes nothing.
    assert!(s[0].later_revisions_material, "{:?}", s[0].problems);
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
}

/// The objects of an update written after `base`: their offsets.
fn write_objects(base: &[u8], objs: &[(u32, u16, Vec<u8>)]) -> (Vec<u8>, Vec<(u32, u16, usize)>) {
    let mut out = base.to_vec();
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    let mut offs = Vec::new();
    for (n, g, body) in objs {
        offs.push((*n, *g, out.len()));
        out.extend_from_slice(format!("{n} {g} obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    (out, offs)
}

/// A cross-reference table with one subsection per entry line given.
fn table(lines: &[(u32, Vec<String>)]) -> String {
    let mut t = String::from("xref\n");
    for (start, entries) in lines {
        t.push_str(&format!("{start} {}\n", entries.len()));
        for e in entries {
            t.push_str(e);
        }
    }
    t
}

fn line(offset: usize, generation: u16, kind: char) -> String {
    format!("{offset:010} {generation:05} {kind} \n")
}

/// A material update with the lines of its table, its /Size and where its
/// startxref points (`None`: at the table), changed by `irregular`.
struct Material {
    objs: Vec<(u32, u16, Vec<u8>)>,
    root: ObjectId,
    info: ObjectId,
    prev_x: usize,
    size: u32,
}

fn material_update(f: &Fixture) -> Material {
    let prev = load(f, &f.signed);
    let root = page_root(&prev);
    let info = prev.trailer.get(b"Info").unwrap().as_reference().unwrap();
    let size = lopdf::Document::load_mem(&f.signed)
        .unwrap()
        .max_id
        .max(prev.max_id)
        + 10;
    let (cert, dss) = (size, size + 1);
    let mut cat = prev.get_dictionary(root).unwrap().clone();
    cat.set("DSS", Object::Reference((dss, 0)));
    Material {
        objs: vec![
            (cert, 0, stream_body(None, (cert, 0), "", &f.der)),
            (dss, 0, format!("<< /Certs [{cert} 0 R] >>").into_bytes()),
            (root.0, root.1, ser(&Object::Dictionary(cat)).into_bytes()),
        ],
        root,
        info,
        prev_x: last_startxref(&f.signed),
        size: size + 2,
    }
}

/// The update `m` written with a table built by `lines` from the offsets of
/// its objects, a trailer with `/Size size`, and `startxref` moved by `shift`.
fn with_table(
    f: &Fixture,
    m: &Material,
    lines: impl Fn(&[(u32, u16, usize)]) -> Vec<(u32, Vec<String>)>,
    size: u32,
    extra_section: bool,
    shift: isize,
) -> Vec<u8> {
    let (mut out, offs) = write_objects(&f.signed, &m.objs);
    let trailer = |prev: usize| {
        format!(
            "trailer\n<< /Size {size} /Root {} {} R /Info {} {} R /Prev {prev} >>\n",
            m.root.0, m.root.1, m.info.0, m.info.1
        )
    };
    if extra_section {
        // A second table nothing points to, between the objects and the real one.
        out.extend_from_slice(table(&lines(&offs)).as_bytes());
        out.extend_from_slice(trailer(m.prev_x).as_bytes());
    }
    let at = out.len();
    out.extend_from_slice(table(&lines(&offs)).as_bytes());
    out.extend_from_slice(trailer(m.prev_x).as_bytes());
    out.extend_from_slice(format!("startxref\n{}\n%%EOF\n", at as isize + shift).as_bytes());
    out
}

fn regular_lines(offs: &[(u32, u16, usize)]) -> Vec<(u32, Vec<String>)> {
    let mut v: Vec<(u32, Vec<String>)> = offs
        .iter()
        .map(|(n, g, o)| (*n, vec![line(*o, *g, 'n')]))
        .collect();
    v.sort_by_key(|l| l.0);
    v
}

#[test]
fn only_regular_updates_are_material() {
    for certify in [0, 1] {
        let f = fixture(certify, false);
        let m = material_update(&f);
        let tag = format!("certify {certify}");
        // The regular update is material.
        let regular = with_table(&f, &m, regular_lines, m.size, false, 0);
        let s = first_signature(&f, &regular);
        assert!(s.later_revisions_material, "{tag}: {:?}", s.problems);
        // Two sections in one update.
        let two = with_table(&f, &m, regular_lines, m.size, true, 0);
        assert_changed(&format!("two sections, {tag}"), &f, &two);
        // Repeated and overlapping subsections.
        let repeated = with_table(
            &f,
            &m,
            |offs| {
                let mut v = regular_lines(offs);
                v.push(v[0].clone());
                v
            },
            m.size,
            false,
            0,
        );
        assert_changed(&format!("repeated subsection, {tag}"), &f, &repeated);
        let overlapping = with_table(
            &f,
            &m,
            |offs| {
                let mut v = regular_lines(offs);
                // The two new numbers in one subsection, the second again alone.
                let i = v.iter().position(|l| v.iter().any(|m| m.0 == l.0 + 1)).unwrap();
                let j = v.iter().position(|m| m.0 == v[i].0 + 1).unwrap();
                let pair = vec![v[i].1[0].clone(), v[j].1[0].clone()];
                v[i].1 = pair;
                v
            },
            m.size,
            false,
            0,
        );
        assert_changed(&format!("overlapping subsections, {tag}"), &f, &overlapping);
        // A free entry for a number in use that the document does not reach.
        let prev = load(&f, &f.signed);
        let unused = prev
            .objects
            .iter()
            .find(|(_, o)| o.as_dict().is_ok_and(|d| d.has(b"Unused")))
            .map(|(id, _)| *id)
            .unwrap();
        let freed = with_table(
            &f,
            &m,
            |offs| {
                let mut v = regular_lines(offs);
                v.push((unused.0, vec![line(0, unused.1 + 1, 'f')]));
                v.sort_by_key(|l| l.0);
                v
            },
            m.size,
            false,
            0,
        );
        assert_changed(&format!("free entry of a number in use, {tag}"), &f, &freed);
        // A /Size that does not cover the new numbers.
        let small = with_table(&f, &m, regular_lines, m.size - 1, false, 0);
        assert_changed(&format!("/Size too small, {tag}"), &f, &small);
        // A startxref that is not the offset of the section.
        let moved = with_table(&f, &m, regular_lines, m.size, false, -1);
        assert_changed(&format!("startxref beside the section, {tag}"), &f, &moved);
        let at_object = {
            let (mut out, offs) = write_objects(&f.signed, &m.objs);
            out.extend_from_slice(table(&regular_lines(&offs)).as_bytes());
            out.extend_from_slice(
                format!(
                    "trailer\n<< /Size {} /Root {} 0 R /Info {} 0 R /Prev {} >>\nstartxref\n{}\n%%EOF\n",
                    m.size, m.root.0, m.info.0, m.prev_x, offs[1].2
                )
                .as_bytes(),
            );
            out
        };
        assert_changed(&format!("startxref at an object, {tag}"), &f, &at_object);
        // Object streams: a regular one is material, one extending another is not.
        for extends in [false, true] {
            let bytes = objstm_update(&f, &m, extends);
            if extends {
                assert_changed(&format!("object stream with /Extends, {tag}"), &f, &bytes);
            } else {
                let s = first_signature(&f, &bytes);
                assert!(
                    s.later_revisions_material,
                    "object stream, {tag}: {:?}",
                    s.problems
                );
            }
        }
    }
}

/// The material update with its store dictionary in an object stream
/// (extending another one when `extends`) and a cross-reference stream.
fn objstm_update(f: &Fixture, m: &Material, extends: bool) -> Vec<u8> {
    let base = f.signed.windows(5).position(|w| w == b"%PDF-").unwrap();
    let (cert, dss) = (m.objs[0].0, m.objs[1].0);
    let (os, other, xs) = (m.size, m.size + 1, m.size + 2);
    let header = format!("{dss} 0 ");
    let body = format!("{header}<< /Certs [{cert} 0 R] >>");
    let extra = if extends {
        format!(" /Extends {other} 0 R")
    } else {
        String::new()
    };
    let mut objs = vec![
        m.objs[0].clone(),
        m.objs[2].clone(),
        (
            os,
            0,
            stream_body(
                None,
                (os, 0),
                &format!("/Type /ObjStm /N 1 /First {}{extra}", header.len()),
                body.as_bytes(),
            ),
        ),
    ];
    if extends {
        objs.push((
            other,
            0,
            stream_body(None, (other, 0), "/Type /ObjStm /N 0 /First 0", b""),
        ));
    }
    let (mut out, offs) = write_objects(&f.signed, &objs);
    let at = out.len();
    let mut rows: Vec<(u32, u8, u32, u16)> = offs
        .iter()
        .map(|(n, g, o)| (*n, 1, (o - base) as u32, *g))
        .collect();
    rows.push((dss, 2, os, 0));
    rows.push((xs, 1, (at - base) as u32, 0));
    rows.sort_by_key(|r| r.0);
    let mut data = Vec::new();
    let mut index = Vec::new();
    for (n, t, a, b) in &rows {
        index.push(format!("{n} 1"));
        data.push(*t);
        data.extend_from_slice(&a.to_be_bytes());
        data.extend_from_slice(&b.to_be_bytes());
    }
    out.extend_from_slice(
        format!(
            "{xs} 0 obj\n<< /Type /XRef /Size {} /W [1 4 2] /Index [{}] /Root {} 0 R /Info {} 0 R /Prev {} /Length {} >>\nstream\n",
            xs + 1,
            index.join(" "),
            m.root.0,
            m.info.0,
            m.prev_x,
            data.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&data);
    out.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{}\n%%EOF\n", at - base).as_bytes());
    out
}
