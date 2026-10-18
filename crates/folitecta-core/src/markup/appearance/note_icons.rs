//! The icons of notes, drawn in a box of 20 points.

use super::rgb;
use super::shapes::{ellipse, poly};
use crate::util::fmt_num;

/// The icons a note can have (its `/Name`): the seven of the PDF standard,
/// then four more that other viewers draw too.
pub const NOTE_ICONS: &[&str] = &[
    "Comment",
    "Note",
    "Key",
    "Help",
    "NewParagraph",
    "Paragraph",
    "Insert",
    "Check",
    "Circle",
    "Cross",
    "Star",
];

/// The drawing of the note icon `name` in a box of 20 points whose top-left
/// corner is (`x`, `y`), in the colour `c` with dark outlines. Unknown
/// names get the comment bubble.
pub(super) fn note_icon(name: &str, x: f32, y: f32, c: [f32; 3]) -> String {
    let f = fmt_num;
    // Box coordinates: `u` to the right, `v` down from the top-left corner.
    let p = |u: f32, v: f32| (x + u, y - v);
    let pts = |list: &[(f32, f32)]| list.iter().map(|(u, v)| p(*u, *v)).collect::<Vec<_>>();
    let oval = |u0: f32, v0: f32, u1: f32, v1: f32| ellipse(x + u0, y - v1, x + u1, y - v0);
    let square = |u0: f32, v0: f32, u1: f32, v1: f32| {
        format!("{} {} {} {} re ", f(x + u0), f(y - v1), f(u1 - u0), f(v1 - v0))
    };
    let fill = rgb(c, "rg");
    let dark = "0.2 0.2 0.2";
    // A thick line in the colour, edged in dark: outline first, colour over it.
    let edged = |path: String| format!("{dark} RG 4.4 w {path}S {} 2.6 w {path}S ", rgb(c, "RG"));
    // A colour tile behind a dark sign.
    let tile = format!("{fill}{dark} RG 1 w {}B {dark} rg ", square(1.0, 1.0, 19.0, 19.0));
    let mut s = String::from("1 J 1 j ");
    match name {
        "Note" => {
            s.push_str(&format!("{fill}{dark} RG 1 w "));
            s.push_str(
                &(poly(
                    &pts(&[(3.0, 1.0), (13.5, 1.0), (17.0, 4.5), (17.0, 19.0), (3.0, 19.0)]),
                    true,
                ) + "B "),
            );
            s.push_str(&(poly(&pts(&[(13.5, 1.0), (13.5, 4.5), (17.0, 4.5)]), false) + "S "));
            s.push_str("0.8 w ");
            for v in [8.0, 11.0, 14.0] {
                s.push_str(&(poly(&pts(&[(6.0, v), (14.0, v)]), false) + "S "));
            }
        }
        "Key" => {
            s.push_str(&format!("{dark} RG 3.4 w "));
            s.push_str(&(poly(&pts(&[(7.5, 7.5), (17.5, 17.5)]), false) + "S "));
            s.push_str(&(poly(&pts(&[(14.0, 14.0), (16.5, 11.5)]), false) + "S "));
            s.push_str(&(poly(&pts(&[(11.5, 11.5), (13.5, 9.5)]), false) + "S "));
            s.push_str(&format!("{} 1.8 w ", rgb(c, "RG")));
            s.push_str(&(poly(&pts(&[(7.5, 7.5), (17.5, 17.5)]), false) + "S "));
            s.push_str(&format!("{fill}{dark} RG 1 w {}B ", oval(1.0, 1.0, 10.0, 10.0)));
            s.push_str(&format!("{dark} rg {}f ", oval(4.2, 4.2, 6.8, 6.8)));
        }
        "Help" => {
            s.push_str(&format!("{fill}{dark} RG 1 w {}B ", oval(1.0, 1.0, 19.0, 19.0)));
            s.push_str(&format!("{dark} RG 2 w "));
            s.push_str(
                &(poly(
                    &pts(&[
                        (7.0, 7.5),
                        (7.6, 5.2),
                        (10.0, 4.4),
                        (12.4, 5.2),
                        (13.0, 7.4),
                        (11.6, 9.4),
                        (10.0, 10.6),
                        (10.0, 12.6),
                    ]),
                    false,
                ) + "S "),
            );
            s.push_str(&format!("{dark} rg {}f ", oval(8.8, 14.2, 11.2, 16.6)));
        }
        "NewParagraph" => {
            s.push_str(&tile);
            s.push_str(&(poly(&pts(&[(10.0, 3.5), (15.5, 10.0), (4.5, 10.0)]), true) + "f "));
            s.push_str(&square(4.5, 12.5, 15.5, 15.0));
            s.push_str("f ");
        }
        "Paragraph" => {
            s.push_str(&tile);
            s.push_str(&format!("{}f ", oval(3.5, 3.5, 11.0, 10.5)));
            s.push_str(&square(7.5, 3.5, 15.5, 5.0));
            s.push_str(&square(9.6, 3.5, 11.2, 17.0));
            s.push_str(&square(13.2, 3.5, 14.8, 17.0));
            s.push_str("f ");
        }
        "Insert" => {
            s.push_str(&format!("{fill}{dark} RG 1 w "));
            s.push_str(
                &(poly(
                    &pts(&[(1.5, 18.5), (10.0, 2.0), (18.5, 18.5), (10.0, 13.0)]),
                    true,
                ) + "B "),
            );
        }
        "Check" => s.push_str(&edged(poly(
            &pts(&[(3.0, 10.5), (8.0, 15.5), (17.0, 4.0)]),
            false,
        ))),
        "Circle" => s.push_str(&edged(oval(3.0, 3.0, 17.0, 17.0))),
        "Cross" => {
            s.push_str(&edged(poly(&pts(&[(4.0, 4.0), (16.0, 16.0)]), false)));
            s.push_str(&edged(poly(&pts(&[(16.0, 4.0), (4.0, 16.0)]), false)));
        }
        "Star" => {
            let star: Vec<(f32, f32)> = (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { 9.0 } else { 3.8 };
                    let a = std::f32::consts::PI * (i as f32 / 5.0 - 0.5);
                    p(10.0 + r * a.cos(), 10.5 + r * a.sin())
                })
                .collect();
            s.push_str(&format!("{fill}{dark} RG 1 w "));
            s.push_str(&(poly(&star, true) + "B "));
        }
        // The comment bubble.
        _ => {
            s.push_str(&format!("{fill}{dark} RG 1 w "));
            s.push_str(&format!("{}B ", square(1.0, 1.0, 19.0, 15.0)));
            s.push_str(&(poly(&pts(&[(4.0, 15.0), (4.0, 19.5), (9.0, 15.0)]), true) + "B "));
            s.push_str("0.25 0.25 0.25 RG 0.8 w ");
            for k in 0..3 {
                let v = 4.5 + k as f32 * 3.5;
                s.push_str(&(poly(&pts(&[(4.0, v), (15.0, v)]), false) + "S "));
            }
        }
    }
    s
}
