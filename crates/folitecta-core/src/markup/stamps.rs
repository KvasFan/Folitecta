//! Text stamps ("APPROVED", "DRAFT", …) as `Stamp` annotations with a generated appearance.

use lopdf::{ObjectId, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::fmt_num;

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
        // Text stamps are drawn with Helvetica only: text beyond WinAnsi is
        // refused before anything changes, never drawn as `?`.
        if let Some(problem) = crate::appearance::text_problem(&self.inner, &annot) {
            return Err(problem.into());
        }
        self.begin_edit();
        annot.set("BS", dictionary! { "W" => 1.0f32, "S" => "S" });
        self.finish_annotation(page_id, annot)
    }
}
