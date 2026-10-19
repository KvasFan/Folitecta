//! XFDF: comments written to and read from the XML format, measurements included.

use base64::Engine;
use lopdf::{Dictionary, Object, ObjectId, dictionary};

use super::*;
use crate::util::{deref, dict_text, fmt_num, num, rect_of};
use crate::xml::{self, XmlElement};

pub(super) fn measure_to_xfdf(doc: &lopdf::Document, m: &Dictionary) -> XmlElement {
    let mut el = XmlElement::new("measure");
    if let Some(r) = dict_text(doc, m, b"R") {
        el.set("rateValue", r);
    }
    for (key, tag) in MEASURE_AXES {
        let formats = m
            .get(key.as_bytes())
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default();
        for nf in formats {
            let Ok(nf) = deref(doc, &nf).as_dict() else {
                continue;
            };
            let mut c = XmlElement::new(tag);
            for (k, v) in nf.iter() {
                if k.as_slice() == b"Type" {
                    continue;
                }
                let value = match deref(doc, v) {
                    Object::Name(n) => String::from_utf8_lossy(n).to_string(),
                    Object::Boolean(b) => b.to_string(),
                    other => crate::util::text_of(doc, other).unwrap_or_default(),
                };
                c.set(&String::from_utf8_lossy(k).to_ascii_lowercase(), value);
            }
            el.push(c);
        }
    }
    el
}

pub(super) fn measure_from_xfdf(el: &XmlElement) -> Dictionary {
    let mut m = dictionary! { "Type" => "Measure", "Subtype" => "RL" };
    if let Some(r) = el.attr("rateValue") {
        m.set("R", lopdf::text_string(r));
    }
    for (key, tag) in MEASURE_AXES {
        let formats: Vec<Object> = el
            .children_named(tag)
            .map(|nf| {
                let mut d = dictionary! { "Type" => "NumberFormat" };
                for (k, v) in &nf.attrs {
                    let k = k.to_ascii_uppercase();
                    let value = match k.as_str() {
                        "F" | "O" => name(v),
                        "FD" => Object::Boolean(yes(Some(v))),
                        "C" => Object::Real(parse_number(Some(v)).unwrap_or(1.0)),
                        "D" => Object::Integer(parse_number(Some(v)).unwrap_or(100.0).round() as i64),
                        _ => lopdf::text_string(v),
                    };
                    d.set(k, value);
                }
                Object::Dictionary(d)
            })
            .collect();
        if !formats.is_empty() {
            m.set(*key, Object::Array(formats));
        }
    }
    m
}

/// Convert one XFDF comment element. `fallback_name` names comments without a name.
pub(super) fn xfdf_comment(
    el: &XmlElement,
    subtype: &str,
    page: usize,
    fallback_name: String,
) -> Option<Imported> {
    let a = |k: &str| el.attr(k);
    let mut d = dictionary! { "Type" => "Annot", "Subtype" => subtype };
    let mut geometry: Vec<f32> = Vec::new();
    let endings = |d: &mut Dictionary| {
        d.set(
            "LE",
            Object::Array(vec![
                name(a("head").unwrap_or("None")),
                name(a("tail").unwrap_or("None")),
            ]),
        );
    };
    match subtype {
        "Line" => {
            let s = parse_numbers(a("start").unwrap_or_default());
            let e = parse_numbers(a("end").unwrap_or_default());
            if s.len() < 2 || e.len() < 2 {
                return None;
            }
            let l = [s[0], s[1], e[0], e[1]];
            geometry.extend(l);
            d.set("L", number_list(&l));
            endings(&mut d);
            for (attr, key) in [
                ("leaderLength", "LL"),
                ("leaderExtend", "LLE"),
                ("leaderOffset", "LLO"),
            ] {
                if let Some(v) = parse_number(a(attr)) {
                    d.set(key, Object::Real(v));
                }
            }
            if yes(a("caption")) {
                d.set("Cap", true);
            }
            if let Some(cp) = a("caption-style") {
                d.set("CP", name(cp));
            }
            if let (Some(h), Some(v)) = (
                parse_number(a("caption-offset-h")),
                parse_number(a("caption-offset-v")),
            ) {
                d.set("CO", number_list(&[h, v]));
            }
        }
        "Polygon" | "PolyLine" => {
            let v = el
                .child("vertices")
                .map(|v| parse_numbers(&v.text()))
                .unwrap_or_default();
            if v.len() < 4 {
                return None;
            }
            let v = v[..v.len() / 2 * 2].to_vec();
            geometry.extend(&v);
            d.set("Vertices", number_list(&v));
            if subtype == "PolyLine" {
                endings(&mut d);
            }
        }
        "Ink" => {
            let gestures: Vec<Vec<f32>> = el
                .child("inklist")
                .map(|l| {
                    l.children_named("gesture")
                        .map(|g| parse_numbers(&g.text()))
                        .filter(|g| g.len() >= 2)
                        .map(|g| g[..g.len() / 2 * 2].to_vec())
                        .collect()
                })
                .unwrap_or_default();
            if gestures.is_empty() {
                return None;
            }
            for g in &gestures {
                geometry.extend(g);
            }
            d.set(
                "InkList",
                Object::Array(gestures.iter().map(|g| number_list(g)).collect()),
            );
        }
        "Highlight" | "Underline" | "Squiggly" | "StrikeOut" | "Redact" => {
            let q = parse_numbers(a("coords").unwrap_or_default());
            if q.len() >= 8 {
                let q = q[..q.len() / 8 * 8].to_vec();
                geometry.extend(&q);
                d.set("QuadPoints", number_list(&q));
            } else if subtype != "Redact" {
                return None;
            }
        }
        _ => {}
    }
    let r = parse_numbers(a("rect").unwrap_or_default());
    let rect = if r.len() >= 4 {
        crate::geom::normalize([r[0], r[1], r[2], r[3]])
    } else if geometry.len() >= 2 {
        bbox(&geometry)
    } else {
        return None;
    };
    d.set("Rect", number_list(&rect));
    if let Some(c) = a("color").and_then(parse_color) {
        d.set("C", number_list(&c));
    }
    if let Some(c) = a("interior-color").and_then(parse_color) {
        d.set("IC", number_list(&c));
    }
    if let Some(v) = parse_number(a("opacity")) {
        d.set("CA", Object::Real(v.clamp(0.0, 1.0)));
    }
    if let Some(f) = a("flags") {
        d.set("F", parse_flags(f));
    }
    let nm = a("name")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or(fallback_name);
    d.set("NM", lopdf::text_string(&nm));
    for (attr, key) in [("title", "T"), ("subject", "Subj")] {
        if let Some(v) = a(attr) {
            d.set(key, lopdf::text_string(v));
        }
    }
    for (attr, key) in [("date", "M"), ("creationdate", "CreationDate")] {
        if let Some(v) = a(attr) {
            d.set(key, Object::string_literal(v.trim()));
        }
    }
    if let Some(v) = a("intent") {
        d.set("IT", name(v));
    }
    if let Some(v) = parse_number(a("rotation")) {
        d.set("Rotate", v.round() as i64);
    }
    let fringe = parse_numbers(a("fringe").unwrap_or_default());
    if fringe.len() == 4 {
        d.set("RD", number_list(&fringe));
    }
    // Border.
    let style = a("style").map(|s| s.trim().to_ascii_lowercase());
    let mut bs = dictionary! { "Type" => "Border" };
    if let Some(w) = parse_number(a("width")) {
        bs.set("W", Object::Real(w.max(0.0)));
    }
    if let Some(s) = style
        .as_deref()
        .and_then(|s| BORDER_STYLES.iter().find(|(_, n)| *n == s))
    {
        bs.set("S", name(s.0));
    }
    let dashes = parse_numbers(a("dashes").unwrap_or_default());
    if !dashes.is_empty() {
        bs.set("D", number_list(&dashes));
        if !bs.has(b"S") {
            bs.set("S", name("D"));
        }
    }
    if bs.len() > 1 {
        d.set("BS", bs);
    }
    if style.as_deref() == Some("cloudy") {
        let intensity = parse_number(a("intensity")).unwrap_or(1.0);
        d.set("BE", dictionary! { "S" => "C", "I" => Object::Real(intensity) });
    }
    // Subtype entries.
    match subtype {
        "Text" => {
            d.set("Name", name(a("icon").unwrap_or("Note")));
            if let Some(s) = a("state") {
                d.set("State", lopdf::text_string(s));
            }
            if let Some(s) = a("statemodel") {
                d.set("StateModel", lopdf::text_string(s));
            }
        }
        "FreeText" => {
            if let Some(q) = parse_number(a("justification")) {
                d.set("Q", (q.round() as i64).clamp(0, 2));
            }
            let da = el
                .child("defaultappearance")
                .map(|e| e.text())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "/Helv 12 Tf 0 0 0 rg".to_string());
            d.set("DA", Object::string_literal(da));
            if let Some(ds) = el.child("defaultstyle").map(|e| e.text()) {
                d.set("DS", lopdf::text_string(&ds));
            }
            let cl = parse_numbers(a("callout").unwrap_or_default());
            if cl.len() >= 4 {
                d.set("CL", number_list(&cl[..cl.len() / 2 * 2]));
            }
            if let Some(h) = a("head") {
                d.set("LE", name(h));
            }
        }
        "Stamp" | "Sound" => {
            if let Some(icon) = a("icon") {
                d.set("Name", name(icon));
            }
        }
        "Caret" => {
            let sym = a("symbol").unwrap_or("none").trim().to_ascii_lowercase();
            d.set(
                "Sy",
                name(if sym == "paragraph" || sym == "p" {
                    "P"
                } else {
                    "None"
                }),
            );
        }
        "FileAttachment" => {
            d.set("Name", name(a("icon").unwrap_or("PushPin")));
        }
        "Redact" => {
            if let Some(t) = a("overlay-text") {
                d.set("OverlayText", lopdf::text_string(t));
            }
            if let Some(q) = parse_number(a("justification")) {
                d.set("Q", (q.round() as i64).clamp(0, 2));
            }
        }
        _ => {}
    }
    if let Some(m) = el.child("measure") {
        d.set("Measure", measure_from_xfdf(m));
    }
    let rich = el
        .child("contents-richtext")
        .and_then(|r| r.elements().next().cloned());
    let contents = el
        .child("contents")
        .map(|c| c.text())
        .or_else(|| rich.as_ref().map(|b| b.text()));
    if let Some(c) = contents.filter(|c| !c.is_empty()) {
        d.set("Contents", lopdf::text_string(&c));
    }
    if let Some(body) = rich {
        d.set(
            "RC",
            lopdf::text_string(&format!("<?xml version=\"1.0\"?>{}", body.to_xml())),
        );
    }
    let in_reply_to = a("inreplyto")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if in_reply_to.is_some() {
        let group = a("replyType")
            .map(|t| t.eq_ignore_ascii_case("group"))
            .unwrap_or(false);
        d.set("RT", name(if group { "Group" } else { "R" }));
    }
    let popup = el.child("popup").and_then(|p| {
        let r = parse_numbers(p.attr("rect").unwrap_or_default());
        if r.len() < 4 {
            return None;
        }
        let mut pd = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Popup",
            "Rect" => number_list(&crate::geom::normalize([r[0], r[1], r[2], r[3]])),
            "Open" => yes(p.attr("open")),
        };
        if let Some(f) = p.attr("flags") {
            pd.set("F", parse_flags(f));
        }
        Some(pd)
    });
    let file = if subtype == "FileAttachment" {
        el.child("data").map(|data| {
            let raw = data.text();
            let bytes = match data
                .attr("encoding")
                .map(|e| e.trim().to_ascii_lowercase())
                .as_deref()
            {
                Some("base64") => base64::engine::general_purpose::STANDARD
                    .decode(raw.split_whitespace().collect::<String>())
                    .unwrap_or_default(),
                _ => hex_decode(&raw),
            };
            let flate = data.attr("filter").map(|f| f.contains("Flate")).unwrap_or(false);
            let file_name = a("file").unwrap_or("attachment").to_string();
            (file_name, bytes, flate)
        })
    } else {
        None
    };
    Some(Imported {
        page,
        dict: d,
        popup,
        name: nm,
        in_reply_to,
        keep_appearance: false,
        file,
        // XFDF does not name layers: a comment updated in place keeps its own.
        layer: None,
    })
}

impl Document {
    pub(super) fn comment_to_xfdf(&self, page: usize, id: ObjectId, d: &Dictionary) -> Option<XmlElement> {
        let doc = &self.inner;
        let subtype = name_str(d, b"Subtype")?;
        let tag = SUBTYPES.iter().find(|(s, _)| *s == subtype)?.1;
        let rect = d.get(b"Rect").ok().and_then(|r| rect_of(doc, r))?;
        let mut el = XmlElement::new(tag);
        el.set("page", page.to_string());
        el.set("rect", join(&rect, ","));
        if let Some(c) = color_from(doc, d, b"C") {
            el.set("color", hex_color(c));
        }
        if let Some(v) = dict_text(doc, d, b"M") {
            el.set("date", v);
        }
        let flags = d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
        if flags != 0 {
            el.set("flags", flags_text(flags));
        }
        el.set("name", comment_name(doc, d, id));
        if let Some(v) = dict_text(doc, d, b"T") {
            el.set("title", v);
        }
        if let Some(v) = dict_text(doc, d, b"Subj") {
            el.set("subject", v);
        }
        if let Some(v) = dict_text(doc, d, b"CreationDate") {
            el.set("creationdate", v);
        }
        if let Some(v) = number(doc, d, b"CA").filter(|v| *v < 0.999) {
            el.set("opacity", fmt_num(v));
        }
        if let Some(c) = color_from(doc, d, b"IC") {
            el.set("interior-color", hex_color(c));
        }
        let bs = sub_dict(doc, d, b"BS");
        let width = bs.and_then(|b| number(doc, b, b"W")).or_else(|| {
            let border = numbers(doc, d, b"Border");
            border.get(2).copied()
        });
        if let Some(w) = width {
            el.set("width", fmt_num(w));
        }
        if let Some(s) = bs.and_then(|b| name_str(b, b"S")) {
            if let Some((_, n)) = BORDER_STYLES.iter().find(|(k, _)| *k == s) {
                el.set("style", *n);
            }
        }
        let dashes = bs.map(|b| numbers(doc, b, b"D")).unwrap_or_default();
        if !dashes.is_empty() {
            el.set("dashes", join(&dashes, ","));
        }
        if let Some(be) = sub_dict(doc, d, b"BE") {
            if name_str(be, b"S").as_deref() == Some("C") {
                el.set("style", "cloudy");
                el.set("intensity", fmt_num(number(doc, be, b"I").unwrap_or(1.0)));
            }
        }
        if let Some(v) = name_str(d, b"IT") {
            el.set("intent", v);
        }
        if let Ok(irt) = d.get(b"IRT").and_then(|o| o.as_reference()) {
            if let Ok(target) = doc.get_dictionary(irt) {
                el.set("inreplyto", comment_name(doc, target, irt));
                let group = name_str(d, b"RT").as_deref() == Some("Group");
                el.set("replyType", if group { "group" } else { "reply" });
            }
        }
        if let Some(r) = d
            .get(b"Rotate")
            .ok()
            .and_then(|r| r.as_i64().ok())
            .filter(|r| *r != 0)
        {
            el.set("rotation", r.to_string());
        }
        let rd = numbers(doc, d, b"RD");
        if rd.len() == 4 {
            el.set("fringe", join(&rd, ","));
        }
        match subtype.as_str() {
            "Text" => {
                el.set("icon", name_str(d, b"Name").unwrap_or_else(|| "Note".into()));
                if let Some(s) = dict_text(doc, d, b"State") {
                    el.set("state", s);
                }
                if let Some(s) = dict_text(doc, d, b"StateModel") {
                    el.set("statemodel", s);
                }
            }
            "Line" => {
                let l = numbers(doc, d, b"L");
                if l.len() >= 4 {
                    el.set("start", format!("{},{}", fmt_num(l[0]), fmt_num(l[1])));
                    el.set("end", format!("{},{}", fmt_num(l[2]), fmt_num(l[3])));
                }
                let (head, tail) = line_endings(doc, d);
                el.set("head", head);
                el.set("tail", tail);
                for (key, attr) in [
                    (&b"LL"[..], "leaderLength"),
                    (b"LLE", "leaderExtend"),
                    (b"LLO", "leaderOffset"),
                ] {
                    if let Some(v) = number(doc, d, key) {
                        el.set(attr, fmt_num(v));
                    }
                }
                if d.get(b"Cap").ok().and_then(|c| c.as_bool().ok()) == Some(true) {
                    el.set("caption", "yes");
                }
                if let Some(cp) = name_str(d, b"CP") {
                    el.set("caption-style", cp);
                }
                let co = numbers(doc, d, b"CO");
                if co.len() == 2 {
                    el.set("caption-offset-h", fmt_num(co[0]));
                    el.set("caption-offset-v", fmt_num(co[1]));
                }
            }
            "PolyLine" | "Polygon" => {
                if subtype == "PolyLine" {
                    let (head, tail) = line_endings(doc, d);
                    el.set("head", head);
                    el.set("tail", tail);
                }
                el.push(XmlElement::new("vertices").with_text(&points_text(&numbers(doc, d, b"Vertices"))));
            }
            "Ink" => {
                let mut list = XmlElement::new("inklist");
                let paths = d
                    .get(b"InkList")
                    .ok()
                    .map(|l| deref(doc, l))
                    .and_then(|l| l.as_array().ok())
                    .cloned()
                    .unwrap_or_default();
                for p in paths {
                    let v: Vec<f32> = deref(doc, &p)
                        .as_array()
                        .map(|a| a.iter().filter_map(|o| num(deref(doc, o))).collect())
                        .unwrap_or_default();
                    list.push(XmlElement::new("gesture").with_text(&points_text(&v)));
                }
                el.push(list);
            }
            "Highlight" | "Underline" | "Squiggly" | "StrikeOut" | "Redact" => {
                let q = numbers(doc, d, b"QuadPoints");
                if !q.is_empty() {
                    el.set("coords", join(&q, ","));
                }
                if subtype == "Redact" {
                    if let Some(t) = dict_text(doc, d, b"OverlayText") {
                        el.set("overlay-text", t);
                    }
                    if let Some(q) = d.get(b"Q").ok().and_then(|q| q.as_i64().ok()) {
                        el.set("justification", q.to_string());
                    }
                }
            }
            "FreeText" => {
                if let Some(q) = d.get(b"Q").ok().and_then(|q| q.as_i64().ok()) {
                    el.set("justification", q.to_string());
                }
                let cl = numbers(doc, d, b"CL");
                if !cl.is_empty() {
                    el.set("callout", join(&cl, ","));
                }
                if let Some(Object::Name(n)) = d.get(b"LE").ok().map(|l| deref(doc, l)) {
                    el.set("head", String::from_utf8_lossy(n).to_string());
                }
                if let Some(da) = dict_text(doc, d, b"DA") {
                    el.push(XmlElement::new("defaultappearance").with_text(&da));
                }
                if let Some(ds) = dict_text(doc, d, b"DS") {
                    el.push(XmlElement::new("defaultstyle").with_text(&ds));
                }
            }
            "Stamp" | "Sound" => {
                if let Some(icon) = name_str(d, b"Name") {
                    el.set("icon", icon);
                }
            }
            "Caret" => {
                let sym = name_str(d, b"Sy").unwrap_or_default();
                el.set("symbol", if sym == "P" { "paragraph" } else { "none" });
            }
            "FileAttachment" => {
                el.set("icon", name_str(d, b"Name").unwrap_or_else(|| "PushPin".into()));
                if let Some(fs) = sub_dict(doc, d, b"FS") {
                    let file_name = dict_text(doc, fs, b"UF")
                        .or_else(|| dict_text(doc, fs, b"F"))
                        .unwrap_or_default();
                    if !file_name.is_empty() {
                        el.set("file", file_name);
                    }
                    let stream = sub_dict(doc, fs, b"EF")
                        .and_then(|ef| ef.get(b"F").or_else(|_| ef.get(b"UF")).ok())
                        .map(|o| deref(doc, o))
                        .and_then(|o| o.as_stream().ok());
                    if let Some(data) = stream.and_then(|s| s.get_plain_content().ok()) {
                        el.push(
                            XmlElement::new("data")
                                .with("mode", "raw")
                                .with("encoding", "hex")
                                .with("length", data.len().to_string())
                                .with_text(&hex_encode(&data)),
                        );
                    }
                }
            }
            _ => {}
        }
        if let Some(m) = sub_dict(doc, d, b"Measure") {
            el.push(measure_to_xfdf(doc, m));
        }
        if let Some(c) = dict_text(doc, d, b"Contents").filter(|c| !c.is_empty()) {
            el.push(XmlElement::new("contents").with_text(&c));
        }
        if let Some(body) = dict_text(doc, d, b"RC").and_then(|rc| rich_text_element(&rc)) {
            let mut rich = XmlElement::new("contents-richtext");
            rich.push(body);
            el.push(rich);
        }
        if let Ok(pid) = d.get(b"Popup").and_then(|p| p.as_reference()) {
            if let Ok(p) = doc.get_dictionary(pid) {
                if let Some(pr) = p.get(b"Rect").ok().and_then(|r| rect_of(doc, r)) {
                    let open = p
                        .get(b"Open")
                        .ok()
                        .and_then(|o| o.as_bool().ok())
                        .unwrap_or(false);
                    let mut pe = XmlElement::new("popup")
                        .with("page", page.to_string())
                        .with("rect", join(&pr, ","))
                        .with("open", if open { "yes" } else { "no" });
                    let pf = p.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
                    if pf != 0 {
                        pe.set("flags", flags_text(pf));
                    }
                    el.push(pe);
                }
            }
        }
        Some(el)
    }

    /// Export comments as XFDF: every comment, or only the listed `(page, index)` pairs.
    pub fn export_comments_xfdf(&self, only: Option<&[(usize, usize)]>) -> String {
        let mut root = XmlElement::new("xfdf")
            .with("xmlns", "http://ns.adobe.com/xfdf/")
            .with("xml:space", "preserve");
        let mut annots = XmlElement::new("annots");
        for (page, id, d) in self.exchange_comments(only) {
            if let Some(el) = self.comment_to_xfdf(page, id, &d) {
                annots.push(el);
            }
        }
        root.push(annots);
        if let Some(file) = self.path().and_then(|p| p.file_name()) {
            root.push(XmlElement::new("f").with("href", file.to_string_lossy().to_string()));
        }
        if let Ok(Object::Array(ids)) = self.inner.trailer.get(b"ID") {
            let hex = |o: Option<&Object>| match o {
                Some(Object::String(b, _)) => hex_encode(b),
                _ => String::new(),
            };
            if ids.len() == 2 {
                root.push(
                    XmlElement::new("ids")
                        .with("original", hex(ids.first()))
                        .with("modified", hex(ids.get(1))),
                );
            }
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}\n",
            root.to_pretty_xml()
        )
    }

    /// Import the comments of an XFDF document (one undo step). When comments
    /// cannot be drawn nothing is imported ([`Error::Undrawable`]).
    pub fn import_comments_xfdf(&mut self, xml_text: &str) -> Result<usize> {
        self.xfdf_import(xml_text, false).map(|(n, _)| n)
    }

    pub(super) fn xfdf_import(
        &mut self,
        xml_text: &str,
        skip: bool,
    ) -> Result<(usize, Vec<UndrawableComment>)> {
        let root = xml::parse(xml_text)?;
        if !root.is("xfdf") {
            return Err(Error::Invalid("the file is not an XFDF document".into()));
        }
        let pages = self.page_count();
        let prefix = format!("folitecta-xfdf-{}-{}", self.revision(), self.inner.max_id);
        let items: Vec<Imported> = root
            .child("annots")
            .map(|annots| {
                annots
                    .elements()
                    .enumerate()
                    .filter_map(|(i, el)| {
                        let subtype = SUBTYPES.iter().find(|(_, t)| el.is(t))?.0;
                        let page: usize = el.attr("page").and_then(|p| p.trim().parse().ok()).unwrap_or(0);
                        if page >= pages {
                            return None;
                        }
                        xfdf_comment(el, subtype, page, format!("{prefix}-{i}"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if items.is_empty() {
            return Err(Error::Invalid(
                "the file contains no comments for the pages of this document".into(),
            ));
        }
        // Every comment of an XFDF file is drawn here: check the text first.
        let (keep, skipped) = {
            let candidates: Vec<Candidate<'_>> = items
                .iter()
                .map(|it| Candidate {
                    page: it.page,
                    name: it.name.clone(),
                    in_reply_to: it.in_reply_to.clone(),
                    dict: &it.dict,
                    keeps_drawing: it.keep_appearance,
                })
                .collect();
            plan_import(&self.inner, &candidates)
        };
        let items: Vec<Imported> = items
            .into_iter()
            .zip(keep)
            .filter_map(|(it, kept)| kept.then_some(it))
            .collect();
        if !skipped.is_empty() && !skip {
            return Err(Error::Undrawable(skipped));
        }
        if items.is_empty() {
            return Ok((0, skipped));
        }
        let n = self.edit_step(|doc| doc.insert_imported(items))?;
        Ok((n, skipped))
    }
}
