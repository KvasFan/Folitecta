//! Measurement labels of dimension comments: lengths, perimeters and areas
//! written with the number formats of their measure dictionary.

use lopdf::Dictionary;

use super::name_of;
use super::shapes::signed_area;
use crate::util::{deref, num};

/// A number format of a measure dictionary (`/X`, `/A`, ...).
struct NumFmt {
    unit: String,
    factor: f32,
    decimals: usize,
    /// Separators of the decimals (`/RD`) and the thousands (`/RT`, none when absent).
    decimal_sep: String,
    group_sep: String,
}

fn num_fmt(doc: &lopdf::Document, measure: &Dictionary, key: &[u8]) -> Option<NumFmt> {
    let arr = measure
        .get(key)
        .ok()
        .map(|a| deref(doc, a))
        .and_then(|a| a.as_array().ok())?;
    let first = arr
        .first()
        .map(|f| deref(doc, f))
        .and_then(|f| f.as_dict().ok())?;
    let unit = first
        .get(b"U")
        .ok()
        .and_then(|u| crate::util::text_of(doc, u))
        .unwrap_or_default();
    let factor = first
        .get(b"C")
        .ok()
        .and_then(|c| num(deref(doc, c)))
        .unwrap_or(1.0);
    let denominator = first
        .get(b"D")
        .ok()
        .and_then(|v| num(deref(doc, v)))
        .unwrap_or(100.0)
        .max(1.0);
    let text = |k: &[u8]| first.get(k).ok().and_then(|v| crate::util::text_of(doc, v));
    Some(NumFmt {
        unit,
        factor,
        decimals: denominator.log10().round().clamp(0.0, 6.0) as usize,
        decimal_sep: text(b"RD")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| ".".into()),
        group_sep: text(b"RT").unwrap_or_default(),
    })
}

fn path_length(pts: &[(f32, f32)], closed: bool) -> f32 {
    let mut total: f32 = pts
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    if closed && pts.len() > 2 {
        let (a, b) = (pts[pts.len() - 1], pts[0]);
        total += ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    }
    total
}

/// Measurement text and its anchor for a dimension annotation (`/IT ...Dimension`).
pub(super) fn measure_label(
    doc: &lopdf::Document,
    d: &Dictionary,
    pts: &[(f32, f32)],
) -> Option<(String, (f32, f32))> {
    let it = name_of(d, b"IT")?;
    let measure = d
        .get(b"Measure")
        .ok()
        .map(|m| deref(doc, m))
        .and_then(|m| m.as_dict().ok())?;
    let n = pts.len();
    let fmt = |v: f32, nf: &NumFmt| {
        let n =
            crate::markup_tools::format_measure(v * nf.factor, nf.decimals, &nf.decimal_sep, &nf.group_sep);
        format!("{n} {}", nf.unit).trim().to_string()
    };
    match it.as_str() {
        "LineDimension" if n >= 2 => {
            let x = num_fmt(doc, measure, b"D").or_else(|| num_fmt(doc, measure, b"X"))?;
            let mid = ((pts[0].0 + pts[n - 1].0) / 2.0, (pts[0].1 + pts[n - 1].1) / 2.0);
            Some((fmt(path_length(pts, false), &x), mid))
        }
        "PolyLineDimension" if n >= 2 => {
            let x = num_fmt(doc, measure, b"D").or_else(|| num_fmt(doc, measure, b"X"))?;
            Some((fmt(path_length(pts, false), &x), pts[n - 1]))
        }
        "PolygonDimension" if n >= 3 => {
            let a = num_fmt(doc, measure, b"A")?;
            let c = pts.iter().fold((0.0, 0.0), |acc, p| {
                (acc.0 + p.0 / n as f32, acc.1 + p.1 / n as f32)
            });
            Some((fmt(signed_area(pts).abs(), &a), c))
        }
        _ => None,
    }
}
