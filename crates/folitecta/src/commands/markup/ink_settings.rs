//! How freehand strokes are drawn: the diameter of the eraser, how much a
//! stroke is smoothed, how much a pen's pressure changes the width
//! (Thickness Scale) and the line width profile (the pen's pressure, one
//! width, or thin ends). Kept in the settings; each command opens a small
//! window for its value.

use crate::commands::handlers;
use crate::state::{AppState, Dialog};

/// The diameter of the eraser, in screen pixels (4 to 80; 12 by default).
pub fn eraser_diameter(st: AppState) -> f32 {
    st.value("eraser.diameter")
        .parse()
        .unwrap_or(12.0f32)
        .clamp(4.0, 80.0)
}

/// How much strokes are smoothed: 0 (the points as drawn) to 4; 1 by default.
pub fn smooth_level(st: AppState) -> u8 {
    st.value("ink.smooth").parse().unwrap_or(1u8).min(4)
}

/// The jitter dropped and the length of a piece of curve, in screen pixels,
/// for a smoothing level.
pub fn smoothing(level: u8) -> (f32, f32) {
    match level {
        0 => (0.01, 1000.0),
        1 => (0.5, 2.0),
        2 => (2.0, 3.0),
        3 => (4.0, 4.0),
        _ => (8.0, 6.0),
    }
}

/// How much the pressure changes the width: 0 to 2 (1 by default).
pub fn thickness_scale(st: AppState) -> f32 {
    st.value("ink.thickness_scale")
        .parse::<f32>()
        .map(|v| v / 100.0)
        .unwrap_or(1.0)
        .clamp(0.0, 2.0)
}

/// The line width profile: "pressure" (default), "uniform" or "taper".
pub fn width_profile(st: AppState) -> String {
    match st.value("ink.width_profile").as_str() {
        "uniform" => "uniform".into(),
        "taper" => "taper".into(),
        _ => "pressure".into(),
    }
}

/// The pressures a stroke is drawn with: the pen's, spread by the
/// thickness scale; none for one width; rising and falling at the ends for
/// thin ends (also with a mouse).
pub fn stroke_pressures(st: AppState, points: &[(f64, f64)], pen: &[f32], pen_on: bool) -> Vec<f32> {
    match width_profile(st).as_str() {
        "uniform" => Vec::new(),
        "taper" => {
            let mut len = vec![0.0f64];
            for w in points.windows(2) {
                let l = len.last().copied().unwrap_or(0.0);
                len.push(l + (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1));
            }
            let total = len.last().copied().unwrap_or(0.0).max(1e-6);
            len.iter()
                .map(|l| {
                    let t = (l / total) as f32;
                    0.15 + 0.35 * (t * 5.0).min((1.0 - t) * 5.0).min(1.0) * 2.0
                })
                .map(|p| p.min(1.0))
                .collect()
        }
        _ if pen_on && pen.len() == points.len() => {
            let k = thickness_scale(st);
            pen.iter()
                .map(|p| (0.5 + (p - 0.5) * k).clamp(0.0, 1.0))
                .collect()
        }
        _ => Vec::new(),
    }
}

fn open(st: AppState, what: &str) {
    st.set_dialog(Dialog::InkSetting(what.into()));
}

handlers! {
    "eraserDiameter" => |st| open(st, "eraserDiameter"),
    "inkSmoothLevel" => |st| open(st, "inkSmoothLevel"),
    "thicknessScale" => |st| open(st, "thicknessScale"),
    "lineWidthProfile" => |st| open(st, "lineWidthProfile"),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothing_grows_with_the_level() {
        let gaps: Vec<f32> = (0..5).map(|l| smoothing(l).0).collect();
        assert!(gaps.windows(2).all(|w| w[0] < w[1]));
    }
}
