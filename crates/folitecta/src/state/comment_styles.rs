//! Default comment styles per tool, stored in the preferences, and the style clipboard.

use dioxus::prelude::ReadableExt;
use folitecta_core::AnnotStyle;
use serde::{Deserialize, Serialize};

use crate::state::{AppState, Tool};

/// Serializable mirror of [`AnnotStyle`] for the preferences file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stored {
    color: Option<[f32; 3]>,
    fill: Option<[f32; 3]>,
    opacity: f32,
    width: f32,
    dashed: bool,
    line_start: String,
    line_end: String,
    font_size: f32,
    text_color: [f32; 3],
    align: u8,
}

impl Default for Stored {
    fn default() -> Self {
        Stored::from(&AnnotStyle::default())
    }
}

impl From<&AnnotStyle> for Stored {
    fn from(s: &AnnotStyle) -> Self {
        Stored {
            color: s.color,
            fill: s.fill,
            opacity: s.opacity,
            width: s.width,
            dashed: s.dashed,
            line_start: s.line_start.clone(),
            line_end: s.line_end.clone(),
            font_size: s.font_size,
            text_color: s.text_color,
            align: s.align,
        }
    }
}

impl From<Stored> for AnnotStyle {
    fn from(s: Stored) -> Self {
        AnnotStyle {
            color: s.color,
            fill: s.fill,
            opacity: s.opacity.clamp(0.0, 1.0),
            width: s.width.max(0.0),
            dashed: s.dashed,
            line_start: s.line_start,
            line_end: s.line_end,
            font_size: s.font_size.clamp(2.0, 400.0),
            text_color: s.text_color,
            align: s.align.min(2),
        }
    }
}

/// The annotation subtype a drawing tool creates (style key).
pub fn tool_subtype(tool: Tool) -> Option<&'static str> {
    Some(match tool {
        Tool::Note => "Text",
        Tool::Rect => "Square",
        Tool::Ellipse => "Circle",
        Tool::Line | Tool::Arrow => "Line",
        Tool::Polygon => "Polygon",
        Tool::Polyline => "PolyLine",
        Tool::Highlight => "HighlightArea",
        Tool::TextBox => "FreeText",
        Tool::Typewriter => "Typewriter",
        Tool::Ink => "Ink",
        Tool::Stamp => "Stamp",
        Tool::Redact => "Redact",
        Tool::TextHighlight => "Highlight",
        Tool::TextUnderline => "Underline",
        Tool::TextStrike => "StrikeOut",
        Tool::Cloud => "Cloud",
        Tool::Callout => "Callout",
        Tool::Distance => "Distance",
        Tool::Perimeter => "Perimeter",
        Tool::Area => "Area",
        Tool::Angle => "Angle",
        Tool::Radius => "Radius",
        Tool::Volume => "Volume",
        Tool::FreeHighlight => "FreeHighlight",
        _ => return None,
    })
}

/// Built-in style of new comments of a kind.
pub fn builtin(kind: &str) -> AnnotStyle {
    let base = AnnotStyle::default();
    match kind {
        "Text" => AnnotStyle {
            color: Some([1.0, 0.85, 0.2]),
            ..base
        },
        "Square" => AnnotStyle {
            color: Some([0.86, 0.15, 0.15]),
            width: 2.0,
            ..base
        },
        "Circle" => AnnotStyle {
            color: Some([0.1, 0.45, 0.85]),
            width: 2.0,
            ..base
        },
        "Line" => AnnotStyle {
            color: Some([0.1, 0.1, 0.1]),
            width: 1.5,
            ..base
        },
        "Polygon" => AnnotStyle {
            color: Some([0.55, 0.2, 0.75]),
            width: 2.0,
            ..base
        },
        "PolyLine" => AnnotStyle {
            color: Some([0.55, 0.2, 0.75]),
            width: 2.0,
            ..base
        },
        "HighlightArea" => AnnotStyle {
            color: Some([1.0, 0.9, 0.2]),
            ..base
        },
        "FreeText" => AnnotStyle {
            color: Some([0.4, 0.4, 0.4]),
            fill: Some([1.0, 1.0, 1.0]),
            width: 1.0,
            font_size: 12.0,
            text_color: [0.11, 0.3, 0.85],
            ..base
        },
        "Typewriter" => AnnotStyle {
            color: None,
            width: 0.0,
            font_size: 12.0,
            text_color: [0.0, 0.0, 0.0],
            ..base
        },
        "Ink" => AnnotStyle {
            color: Some([0.2, 0.2, 0.75]),
            width: 2.0,
            ..base
        },
        "Stamp" => AnnotStyle {
            color: Some([0.78, 0.12, 0.12]),
            ..base
        },
        "Redact" => AnnotStyle {
            color: Some([1.0, 0.0, 0.0]),
            fill: Some([0.0, 0.0, 0.0]),
            ..base
        },
        "Highlight" => AnnotStyle {
            color: Some([1.0, 0.9, 0.2]),
            ..base
        },
        "Underline" => AnnotStyle {
            color: Some([0.1, 0.4, 0.95]),
            ..base
        },
        "StrikeOut" => AnnotStyle {
            color: Some([0.9, 0.15, 0.15]),
            ..base
        },
        "Squiggly" => AnnotStyle {
            color: Some([0.1, 0.65, 0.3]),
            ..base
        },
        "Cloud" => AnnotStyle {
            color: Some([0.86, 0.15, 0.15]),
            width: 1.5,
            ..base
        },
        "Callout" => AnnotStyle {
            color: Some([0.4, 0.4, 0.4]),
            fill: Some([1.0, 1.0, 0.85]),
            width: 1.0,
            font_size: 11.0,
            text_color: [0.0, 0.0, 0.0],
            line_end: "OpenArrow".into(),
            ..base
        },
        "Distance" | "Perimeter" | "Area" | "Angle" | "Radius" | "Volume" => AnnotStyle {
            color: Some([0.85, 0.1, 0.1]),
            width: 1.0,
            line_start: if kind == "Distance" {
                "OpenArrow".into()
            } else {
                "None".into()
            },
            line_end: if kind == "Distance" {
                "OpenArrow".into()
            } else {
                "None".into()
            },
            font_size: 10.0,
            ..base
        },
        "FreeHighlight" => AnnotStyle {
            color: Some([1.0, 0.85, 0.0]),
            width: 12.0,
            opacity: 0.45,
            ..base
        },
        _ => base,
    }
}

/// Style used for new comments of `kind` (user default or built-in).
pub fn default_style(st: AppState, kind: &str) -> AnnotStyle {
    st.ui
        .peek()
        .value(&format!("style.{kind}"))
        .and_then(|json| serde_json::from_str::<Stored>(json).ok())
        .map(AnnotStyle::from)
        .unwrap_or_else(|| builtin(kind))
}

/// Remember `style` as the default for new comments of `kind`.
pub fn set_default_style(st: AppState, kind: &str, style: &AnnotStyle) {
    if let Ok(json) = serde_json::to_string(&Stored::from(style)) {
        st.set_value(&format!("style.{kind}"), json);
    }
}

/// Every style kind (subtypes plus tool variants such as typewriter or callout).
pub const KINDS: &[&str] = &[
    "Text",
    "Square",
    "Circle",
    "Line",
    "Polygon",
    "PolyLine",
    "HighlightArea",
    "FreeText",
    "Typewriter",
    "Ink",
    "Stamp",
    "Redact",
    "Highlight",
    "Underline",
    "StrikeOut",
    "Squiggly",
    "Cloud",
    "Callout",
    "Distance",
    "Perimeter",
    "Area",
    "Angle",
    "Radius",
    "Volume",
    "FreeHighlight",
    "Caret",
    "FileAttachment",
];

/// A comment style kept by name, for comments of one kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedStyle {
    pub name: String,
    pub kind: String,
    pub style: Stored,
}

impl NamedStyle {
    pub fn new(name: String, kind: &str, style: &AnnotStyle) -> Self {
        NamedStyle {
            name,
            kind: kind.to_string(),
            style: Stored::from(style),
        }
    }

    pub fn style(&self) -> AnnotStyle {
        AnnotStyle::from(self.style.clone())
    }
}

/// The comment styles kept by name.
pub fn named_styles(st: AppState) -> Vec<NamedStyle> {
    st.ui
        .peek()
        .value("comment.styles")
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

pub fn save_named_styles(st: AppState, list: &[NamedStyle]) {
    if let Ok(json) = serde_json::to_string(list) {
        st.set_value("comment.styles", json);
    }
}

/// Forget the default style of new comments of `kind` (the built-in one applies).
pub fn reset_default_style(st: AppState, kind: &str) {
    st.ui_mut().values.remove(&format!("style.{kind}"));
    st.save_prefs();
}

/// Style kind of an existing comment (typewriter text is a borderless text box).
pub fn kind_of(subtype: &str, style: &AnnotStyle) -> String {
    if subtype == "FreeText" && style.width == 0.0 {
        "Typewriter".into()
    } else {
        subtype.to_string()
    }
}
