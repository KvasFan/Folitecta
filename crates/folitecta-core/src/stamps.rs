//! Text stamps ("APPROVED", "DRAFT", …) as `Stamp` annotations with a generated appearance.

use lopdf::{ObjectId, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{approx_text_width, fmt_num, pdf_literal};

/// Built-in stamp texts.
pub const STAMP_PRESETS: &[&str] = &[
    "APPROVED",
    "DRAFT",
    "CONFIDENTIAL",
    "FINAL",
    "REVIEWED",
    "RECEIVED",
    "VOID",
    "NOT APPROVED",
    "FOR PUBLIC RELEASE",
    "SIGN HERE",
];

impl Document {
    /// Add a picture as a movable stamp comment (transparency is kept).
    pub fn add_image_stamp(
        &mut self,
        page: usize,
        rect: [f32; 4],
        data: Vec<u8>,
        author: &str,
        contents: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let image = crate::images::image_object(data)?;
        self.begin_edit();
        let img_id = self.add_image_object(image);
        let mut annot = self.base_annotation(page_id, "Stamp", rect, author, contents, [0.0, 0.0, 0.0]);
        annot.set("Name", lopdf::Object::Name(b"FolitectaImage".to_vec()));
        let f = fmt_num;
        let content = format!(
            "q {} 0 0 {} {} {} cm /Img Do Q",
            f(rect[2] - rect[0]),
            f(rect[3] - rect[1]),
            f(rect[0]),
            f(rect[1])
        );
        let resources = dictionary! { "XObject" => dictionary! { "Img" => img_id } };
        self.add_appearance(&mut annot, rect, content, resources);
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// Add a text stamp inside `rect` (PDF user space).
    pub fn add_stamp(
        &mut self,
        page: usize,
        rect: [f32; 4],
        text: &str,
        color: [f32; 3],
        author: &str,
    ) -> Result<ObjectId> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::Invalid("stamp text is empty".into()));
        }
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let font_id = self.helvetica_font_id();
        let mut annot = self.base_annotation(page_id, "Stamp", rect, author, text, color);
        annot.set(
            "Name",
            lopdf::Object::Name(
                format!(
                    "Folitecta{}",
                    text.replace(|c: char| !c.is_ascii_alphanumeric(), "")
                )
                .into_bytes(),
            ),
        );
        let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
        let chars = text.chars().count().max(1) as f32;
        let fs = (h * 0.55).min((w - 16.0) / (0.52 * chars)).max(5.0);
        let tw = approx_text_width(text, fs);
        let tx = rect[0] + (w - tw) / 2.0;
        let ty = rect[1] + (h - fs * 0.72) / 2.0;
        let f = fmt_num;
        let [r, g, b] = color;
        let mut content = format!(
            "q {} {} {} RG 2 w {} {} {} {} re S BT /Helv {} Tf {} {} {} rg {} {} Td ",
            f(r),
            f(g),
            f(b),
            f(rect[0] + 1.0),
            f(rect[1] + 1.0),
            f(w - 2.0),
            f(h - 2.0),
            f(fs),
            f(r),
            f(g),
            f(b),
            f(tx),
            f(ty)
        );
        content.push_str(&String::from_utf8_lossy(&pdf_literal(text)));
        content.push_str(" Tj ET Q");
        let resources = dictionary! { "Font" => dictionary! { "Helv" => font_id } };
        self.add_appearance(&mut annot, rect, content, resources);
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }
}
