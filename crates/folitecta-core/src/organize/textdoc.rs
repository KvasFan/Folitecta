//! Plain text to PDF: word wrapping and pagination with the built-in Helvetica font.

use lopdf::Object;

use crate::document::Document;
use crate::error::Result;
use crate::util::{fmt_num, helvetica_width, pdf_literal};

/// Page geometry and font size for [`Document::from_text`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLayout {
    pub font_size: f32,
    pub page_width: f32,
    pub page_height: f32,
    pub margin: f32,
}

impl Default for TextLayout {
    fn default() -> Self {
        TextLayout {
            font_size: 11.0,
            page_width: 595.28,
            page_height: 841.89,
            margin: 56.0,
        }
    }
}

/// Break text into lines no wider than `max_width` points.
pub fn wrap_text(text: &str, font_size: f32, max_width: f32) -> Vec<String> {
    wrap_text_by(text, max_width.max(font_size), &|s| helvetica_width(s, font_size))
}

/// Break text into lines no wider than `max_width` points, measured by `width`.
pub(crate) fn wrap_text_by(text: &str, max_width: f32, width: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        let para = raw.replace('\t', "    ");
        if para.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let indent: String = para.chars().take_while(|c| *c == ' ').collect();
        let mut line = indent.clone();
        let mut line_has_word = false;
        for word in para.split(' ').filter(|w| !w.is_empty()) {
            let candidate = if line_has_word {
                format!("{line} {word}")
            } else {
                format!("{line}{word}")
            };
            if width(&candidate) <= max_width {
                line = candidate;
                line_has_word = true;
                continue;
            }
            if line_has_word {
                out.push(std::mem::take(&mut line));
            }
            // A word longer than the line is broken by characters.
            let mut piece = String::new();
            for ch in word.chars() {
                piece.push(ch);
                if width(&piece) > max_width && piece.chars().count() > 1 {
                    piece.pop();
                    out.push(std::mem::take(&mut piece));
                    piece.push(ch);
                }
            }
            line = piece;
            line_has_word = !line.is_empty();
        }
        out.push(line);
    }
    out
}

impl Document {
    /// A new document with `text` typeset in Helvetica, wrapped and paginated.
    pub fn from_text(text: &str, layout: TextLayout) -> Result<Document> {
        let fs = layout.font_size.clamp(4.0, 72.0);
        let margin = layout
            .margin
            .clamp(0.0, layout.page_width.min(layout.page_height) / 3.0);
        let leading = fs * 1.25;
        let per_page = (((layout.page_height - 2.0 * margin) / leading).floor() as usize).max(1);
        // A form feed starts a new page.
        let mut sections: Vec<&str> = text.split('\u{c}').collect();
        if sections.len() > 1 && sections.last().is_some_and(|s| s.trim().is_empty()) {
            sections.pop();
        }
        let mut chunks: Vec<Vec<String>> = Vec::new();
        for section in sections {
            let lines = wrap_text(
                section.trim_start_matches(['\r', '\n']),
                fs,
                layout.page_width - 2.0 * margin,
            );
            if lines.is_empty() {
                chunks.push(Vec::new());
            } else {
                chunks.extend(lines.chunks(per_page).map(|c| c.to_vec()));
            }
        }
        let mut doc = Document::new_blank(chunks.len(), layout.page_width, layout.page_height)?;
        let font = doc.helvetica_font_id();
        let ids = doc.page_ids();
        for (chunk, page_id) in chunks.iter().zip(ids) {
            if chunk.iter().all(|l| l.is_empty()) {
                continue;
            }
            let mut content = format!(
                "BT /FoliHelv {} Tf {} TL {} {} Td\n",
                fmt_num(fs),
                fmt_num(leading),
                fmt_num(margin),
                fmt_num(layout.page_height - margin - fs)
            )
            .into_bytes();
            for line in chunk.iter() {
                content.extend_from_slice(&pdf_literal(line));
                content.extend_from_slice(b" Tj T*\n");
            }
            content.extend_from_slice(b"ET\n");
            doc.add_page_resource(page_id, "Font", "FoliHelv", Object::Reference(font))?;
            doc.append_page_content(page_id, content, None)?;
        }
        doc.commit()?;
        doc.clear_history();
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_feeds_start_pages() {
        let doc = Document::from_text("one\u{c}two\n\u{c}three\u{c}", TextLayout::default()).unwrap();
        assert_eq!(doc.page_count(), 3);
        assert_eq!(doc.page_text(1), "two");
    }

    #[test]
    fn wraps_long_lines_and_keeps_blank_lines() {
        let lines = wrap_text("alpha beta gamma delta\n\nepsilon", 12.0, 80.0);
        assert!(lines.len() >= 4, "{lines:?}");
        assert!(lines.iter().all(|l| helvetica_width(l, 12.0) <= 80.0 + 0.01));
        assert!(lines.contains(&String::new()));
        let broken = wrap_text(&"x".repeat(200), 12.0, 100.0);
        assert!(broken.len() > 1);
    }

    #[test]
    fn wrap_text_by_matches_wrap_text() {
        for text in [
            "alpha beta gamma delta\n\nepsilon",
            &"x".repeat(200),
            "  indented line\twith a tab and a long word: supercalifragilistic",
            "",
        ] {
            assert_eq!(
                wrap_text_by(text, 80.0, &|s| helvetica_width(s, 12.0)),
                wrap_text(text, 12.0, 80.0)
            );
        }
    }
}
