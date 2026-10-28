//! Strokes of a pen that tells how hard it presses: thin where it pressed
//! lightly, thick where it pressed hard, kept so through saving, moving,
//! restyling and erasing part of them.

use folitecta_core::{AnnotStyle, Document};
use lopdf::Object;

fn style() -> AnnotStyle {
    AnnotStyle {
        color: Some([0.0, 0.6, 0.2]),
        width: 4.0,
        ..AnnotStyle::default()
    }
}

/// A straight stroke from (50, 100) to (250, 100), pressed lightly at the
/// start and hard at the end.
fn stroke() -> (Vec<(f32, f32)>, Vec<f32>) {
    let points: Vec<(f32, f32)> = (0..=20).map(|i| (50.0 + i as f32 * 10.0, 100.0)).collect();
    let pressures: Vec<f32> = (0..=20).map(|i| i as f32 / 20.0).collect();
    (points, pressures)
}

/// The content of the normal appearance of the first comment of page 0.
fn appearance(doc: &Document) -> String {
    let a = &doc.annotations(0).unwrap()[0];
    let d = doc.inner().get_dictionary(a.id.unwrap()).unwrap();
    let ap = d.get(b"AP").unwrap().as_dict().unwrap();
    let n = ap.get(b"N").unwrap().as_reference().unwrap();
    let s = doc.inner().get_object(n).unwrap().as_stream().unwrap();
    String::from_utf8_lossy(&s.decompressed_content().unwrap_or_else(|_| s.content.clone())).to_string()
}

/// The pressures kept with the first comment of page 0, one list per path.
fn pressures(doc: &Document) -> Vec<Vec<f32>> {
    let a = &doc.annotations(0).unwrap()[0];
    let d = doc.inner().get_dictionary(a.id.unwrap()).unwrap();
    match d.get(b"FolitectaPressure") {
        Ok(Object::Array(paths)) => paths
            .iter()
            .map(|p| {
                p.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_float().unwrap())
                    .collect()
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// How far the outline of the stroke reaches above and below its line,
/// near `x`: the half width there.
fn half_width_near(content: &str, x: f32) -> f32 {
    // The points of the path: two numbers before `m` or `l`.
    let tokens: Vec<&str> = content.split_whitespace().collect();
    tokens
        .windows(3)
        .filter(|w| w[2] == "m" || w[2] == "l")
        .filter_map(|w| Some((w[0].parse::<f32>().ok()?, w[1].parse::<f32>().ok()?)))
        .filter(|p| (p.0 - x).abs() < 3.0)
        .map(|p| (p.1 - 100.0).abs())
        .fold(0.0, f32::max)
}

#[test]
fn a_pen_stroke_grows_with_the_pressure_and_keeps_it() {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    let (points, p) = stroke();
    doc.add_pressed_ink(0, &points, &p, &style(), "t").unwrap();
    let a = &doc.annotations(0).unwrap()[0];
    assert_eq!(a.subtype, "Ink");
    // A filled outline, not a stroked line.
    let content = appearance(&doc);
    assert!(content.contains(" f ") && !content.contains(" S "), "{content}");
    // Light at the start: 0.3 times 4 points; hard at the end: 1.7 times.
    let start = half_width_near(&content, 60.0);
    let end = half_width_near(&content, 240.0);
    assert!(start < 1.2, "{start}");
    assert!(end > 2.9, "{end}");
    // The rectangle holds the widest part.
    assert!(
        a.rect[3] >= 100.0 + 3.4 && a.rect[1] <= 100.0 - 3.4,
        "{:?}",
        a.rect
    );

    // Saved and opened again: the same pressures, the same drawing.
    let dir = std::env::temp_dir().join(format!("folitecta-pen-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pen.pdf");
    doc.save_as(&path).unwrap();
    let mut back = Document::open(&path).unwrap();
    let kept = pressures(&back);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].len(), points.len());
    assert!((kept[0][10] - 0.5).abs() < 0.01, "{:?}", kept[0]);
    assert_eq!(appearance(&back), content);

    // Moved and restyled: still drawn from its pressures.
    back.move_annotation(0, 0, 0.0, 50.0).unwrap();
    let moved = appearance(&back);
    assert!(moved.contains(" f "), "{moved}");
    let mut thin = style();
    thin.width = 2.0;
    back.set_annotation_style(0, 0, &thin).unwrap();
    let restyled = appearance(&back);
    assert!(
        restyled.contains(" f ") && !restyled.contains(" S "),
        "{restyled}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn erasing_a_pen_stroke_keeps_the_pressures_of_its_pieces() {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    let (points, p) = stroke();
    doc.add_pressed_ink(0, &points, &p, &style(), "t").unwrap();
    // Cut in the middle: two pieces, each with the pressures of its points.
    assert_eq!(
        doc.erase_ink(0, &[(150.0, 120.0), (150.0, 80.0)], 6.0).unwrap(),
        1
    );
    let kept = pressures(&doc);
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert!(
        kept[0].iter().all(|v| *v < 0.5) && kept[1].iter().all(|v| *v > 0.5),
        "{kept:?}"
    );
    assert!(appearance(&doc).contains(" f "));
}

#[test]
fn strokes_without_pressure_are_drawn_as_before() {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    let (points, p) = stroke();
    doc.add_ink_styled(0, std::slice::from_ref(&points), &style(), "t")
        .unwrap();
    let content = appearance(&doc);
    assert!(content.contains(" S ") && pressures(&doc).is_empty(), "{content}");
    // A pressure missing for a point is refused, nothing added.
    assert!(doc.add_pressed_ink(0, &points, &p[1..], &style(), "t").is_err());
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
}
