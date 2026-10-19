//! Raster images: placing pictures on pages, image pages, rasterizing and flattening.

use std::collections::{HashMap, HashSet};

use lopdf::xobject::image_from;
use lopdf::{Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::marks::display_to_pdf;
use crate::render::{RenderOpts, Renderer};
use crate::util::fmt_num;

/// PDF-space direction vectors of the display u (rightwards) and v (upwards) axes.
pub(crate) fn display_axes(rotation: i32) -> ((f32, f32), (f32, f32)) {
    match rotation {
        90 => ((0.0, 1.0), (-1.0, 0.0)),
        180 => ((-1.0, 0.0), (0.0, -1.0)),
        270 => ((0.0, -1.0), (1.0, 0.0)),
        _ => ((1.0, 0.0), (0.0, 1.0)),
    }
}

fn stream_dims(stream: &Stream) -> (f32, f32) {
    let w = stream
        .dict
        .get(b"Width")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0) as f32;
    let h = stream
        .dict
        .get(b"Height")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0) as f32;
    (w, h)
}

/// An image XObject ready to be added: the stream, an optional soft mask (alpha)
/// and the pixel size.
pub(crate) struct ImageObject {
    pub stream: Stream,
    pub smask: Option<Stream>,
    pub width: f32,
    pub height: f32,
}

/// Decode image bytes (PNG, JPEG, GIF, BMP, TIFF, WebP). JPEG data without
/// alpha is embedded unchanged; everything else becomes Flate RGB with a soft
/// mask when the picture has transparency.
pub(crate) fn image_object(data: Vec<u8>) -> Result<ImageObject> {
    let format = image::guess_format(&data).map_err(|e| Error::Image(e.to_string()))?;
    if format == image::ImageFormat::Jpeg {
        let stream = image_from(data)?;
        let (w, h) = stream_dims(&stream);
        if w >= 1.0 && h >= 1.0 {
            return Ok(ImageObject {
                stream,
                smask: None,
                width: w,
                height: h,
            });
        }
        return Err(Error::Invalid("the image has no size".into()));
    }
    let img = image::load_from_memory(&data).map_err(|e| Error::Image(e.to_string()))?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err(Error::Invalid("the image has no size".into()));
    }
    let has_alpha = img.color().has_alpha();
    let gray = matches!(
        img.color(),
        image::ColorType::L8 | image::ColorType::La8 | image::ColorType::L16 | image::ColorType::La16
    );
    let (pixels, cs): (Vec<u8>, &[u8]) = if gray {
        (img.to_luma8().into_raw(), b"DeviceGray")
    } else {
        (img.to_rgb8().into_raw(), b"DeviceRGB")
    };
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => w as i64,
            "Height" => h as i64,
            "ColorSpace" => Object::Name(cs.to_vec()),
            "BitsPerComponent" => 8i64,
        },
        pixels,
    );
    stream.compress()?;
    let smask = if has_alpha {
        let alpha: Vec<u8> = img.to_rgba8().pixels().map(|p| p[3]).collect();
        if alpha.iter().all(|a| *a == 255) {
            None
        } else {
            let mut m = Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => w as i64,
                    "Height" => h as i64,
                    "ColorSpace" => "DeviceGray",
                    "BitsPerComponent" => 8i64,
                },
                alpha,
            );
            m.compress()?;
            Some(m)
        }
    } else {
        None
    };
    Ok(ImageObject {
        stream,
        smask,
        width: w as f32,
        height: h as f32,
    })
}

/// Build an image XObject stream from encoded image bytes (alpha dropped).
fn image_stream(data: Vec<u8>) -> Result<(Stream, f32, f32)> {
    let obj = image_object(data)?;
    Ok((obj.stream, obj.width, obj.height))
}

impl Document {
    /// Add an image (with its soft mask) to the object table and return its id.
    pub(crate) fn add_image_object(&mut self, mut obj: ImageObject) -> ObjectId {
        if let Some(mask) = obj.smask.take() {
            let mid = self.inner.add_object(mask);
            obj.stream.dict.set("SMask", Object::Reference(mid));
        }
        self.inner.add_object(obj.stream)
    }

    /// Draw an image XObject on a page inside a display-space rectangle
    /// (`[u0, v0, u1, v1]`, origin bottom-left of the page as shown).
    fn place_image(&mut self, page_id: ObjectId, stream: Stream, rect: [f32; 4], tag: &str) -> Result<()> {
        self.place_image_object(
            page_id,
            ImageObject {
                width: 0.0,
                height: 0.0,
                smask: None,
                stream,
            },
            rect,
            tag,
            false,
        )
    }

    /// Draw an image on a page; `drawn`: placed by the user, so on the
    /// drawing layer when there is one.
    fn place_image_object(
        &mut self,
        page_id: ObjectId,
        image: ImageObject,
        rect: [f32; 4],
        tag: &str,
        drawn: bool,
    ) -> Result<()> {
        self.inherit_attributes(&[page_id]);
        let size = self.page_size_of(page_id);
        let xid = self.add_image_object(image);
        let name = format!("FoliImg{}", xid.0);
        self.inner.add_xobject(page_id, name.as_str(), xid)?;
        let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
        let (du, dv) = display_axes(size.rotation);
        let (ex, ey) = display_to_pdf(&size, rect[0], rect[1]);
        let f = fmt_num;
        let content = format!(
            "q {} {} {} {} {} {} cm /{} Do Q\n",
            f(w * du.0),
            f(w * du.1),
            f(h * dv.0),
            f(h * dv.1),
            f(ex),
            f(ey),
            name
        );
        let content = if drawn {
            self.drawn_content(page_id, content.into_bytes())?
        } else {
            content.into_bytes()
        };
        self.append_page_content(page_id, content, Some(tag))?;
        Ok(())
    }

    /// Place an image on a page. Without `display_rect` the picture is centred and
    /// scaled to 60% of the page. Returns the rectangle used (display space, points).
    pub fn add_image(
        &mut self,
        page: usize,
        data: Vec<u8>,
        display_rect: Option<[f32; 4]>,
    ) -> Result<[f32; 4]> {
        let page_id = self.page_id(page)?;
        let image = image_object(data)?;
        let (iw, ih) = (image.width, image.height);
        let size = self.page_size_of(page_id);
        let rect = display_rect.unwrap_or_else(|| {
            let s = (size.width * 0.6 / iw).min(size.height * 0.6 / ih);
            let (w, h) = (iw * s, ih * s);
            let u0 = (size.width - w) / 2.0;
            let v0 = (size.height - h) / 2.0;
            [u0, v0, u0 + w, v0 + h]
        });
        if rect[2] - rect[0] < 1.0 || rect[3] - rect[1] < 1.0 {
            return Err(Error::Invalid("image rectangle is empty".into()));
        }
        self.edit_step(|doc| doc.place_image_object(page_id, image, rect, "Image", true))?;
        Ok(rect)
    }

    /// Pixel size of encoded image data.
    pub fn image_size(data: &[u8]) -> Result<(u32, u32)> {
        let img = image::load_from_memory(data).map_err(|e| Error::Image(e.to_string()))?;
        Ok((img.width(), img.height()))
    }

    /// A new document with one page per image.
    pub fn from_images(images: Vec<Vec<u8>>) -> Result<Document> {
        if images.is_empty() {
            return Err(Error::Invalid("no images selected".into()));
        }
        let mut doc = Document::new_blank(1, 595.0, 842.0)?;
        doc.insert_image_pages(0, images)?;
        doc.delete_pages(&[doc.page_count() - 1])?;
        doc.clear_history();
        Ok(doc)
    }

    /// Place an image file on a page.
    pub fn add_image_from_path(
        &mut self,
        page: usize,
        path: impl AsRef<std::path::Path>,
        display_rect: Option<[f32; 4]>,
    ) -> Result<[f32; 4]> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|e| Error::io(path, e))?;
        self.add_image(page, data, display_rect)
    }

    /// Insert one page per image before index `at`. Pages are A4 (portrait or
    /// landscape following the picture) with 36 pt margins.
    pub fn insert_image_pages(&mut self, at: usize, images: Vec<Vec<u8>>) -> Result<usize> {
        let n = self.page_count();
        if at > n {
            return Err(Error::InvalidPage(at));
        }
        let mut streams = Vec::with_capacity(images.len());
        for data in images {
            streams.push(image_stream(data)?);
        }
        if streams.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let mut ids = self.page_ids();
        let mut new_ids = Vec::with_capacity(streams.len());
        for (stream, iw, ih) in streams {
            let (pw, ph) = if iw >= ih { (842.0, 595.0) } else { (595.0, 842.0) };
            let content_id = self.inner.add_object(Stream::new(dictionary! {}, Vec::new()));
            let page_id = self.inner.add_object(dictionary! {
                "Type" => "Page",
                "MediaBox" => vec![0.into(), 0.into(), pw.into(), ph.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {},
            });
            new_ids.push(page_id);
            let margin = 36.0;
            let s = ((pw - 2.0 * margin) / iw).min((ph - 2.0 * margin) / ih);
            let (w, h) = (iw * s, ih * s);
            let u0 = (pw - w) / 2.0;
            let v0 = (ph - h) / 2.0;
            // The page is not in the tree yet; set_page_order below links it, so place after.
            self.pending_image_pages_placeholder(page_id, stream, [u0, v0, u0 + w, v0 + h])?;
        }
        let tail = ids.split_off(at);
        let count = new_ids.len();
        ids.extend(new_ids);
        ids.extend(tail);
        self.set_page_order(ids)?;
        self.commit()?;
        Ok(count)
    }

    fn pending_image_pages_placeholder(
        &mut self,
        page_id: ObjectId,
        stream: Stream,
        rect: [f32; 4],
    ) -> Result<()> {
        // A detached page has no parent, so page_size_of falls back to its own MediaBox.
        self.place_image(page_id, stream, rect, "Image")
    }

    /// Replace page content with a raster image of the page rendered at `dpi`.
    /// With `with_annotations` the comments are baked into the picture and removed.
    pub fn rasterize_pages(&mut self, pages: &[usize], dpi: f32, with_annotations: bool) -> Result<()> {
        self.rasterize_pages_with_view(pages, dpi, with_annotations, &HashMap::new())
    }

    /// [`Document::rasterize_pages`] as a view shows the pages: the default
    /// visibility of the layers changed by `view`. What hidden layers hide is
    /// not drawn, and the comments of hidden layers stay comments (on their
    /// layer) instead of going with the others.
    pub fn rasterize_pages_with_view(
        &mut self,
        pages: &[usize],
        dpi: f32,
        with_annotations: bool,
        view: &HashMap<ObjectId, bool>,
    ) -> Result<()> {
        let n = self.page_count();
        for &p in pages {
            if p >= n {
                return Err(Error::InvalidPage(p));
            }
        }
        if pages.is_empty() {
            return Ok(());
        }
        let bytes = self.view_bytes(&HashSet::new(), view)?;
        let kept = self.annotations_in_hidden_layers(view);
        self.edit_step(|doc| doc.rasterize_from(bytes, pages, dpi, with_annotations, &kept))
    }

    /// Draw the pages of `bytes` as pictures; `kept`: comments not drawn
    /// (their layer is hidden) that stay on their page.
    fn rasterize_from(
        &mut self,
        bytes: std::sync::Arc<Vec<u8>>,
        pages: &[usize],
        dpi: f32,
        with_annotations: bool,
        kept: &HashSet<ObjectId>,
    ) -> Result<()> {
        let renderer = Renderer::new(bytes)?;
        let ids = self.page_ids();
        let scale = (dpi.max(36.0) / 72.0).min(8.0);
        for &p in pages {
            let id = ids[p];
            let size = self.page_size_of(id);
            let opts = RenderOpts {
                annotations: with_annotations,
                transparent: false,
            };
            let jpeg = crate::export::to_jpeg(renderer.render_rgba(p, scale, opts)?, 88)?;
            let stream = image_from(jpeg)?;
            let xid = self.inner.add_object(stream);
            let f = fmt_num;
            let content = format!(
                "q {} 0 0 {} 0 0 cm /FoliRaster Do Q\n",
                f(size.width),
                f(size.height)
            );
            // The comments of hidden layers on this page, which stay.
            let left: Vec<Object> = self
                .inner
                .get_dictionary(id)
                .ok()
                .and_then(|d| d.get(b"Annots").ok())
                .and_then(|a| crate::util::deref(&self.inner, a).as_array().ok())
                .map(|a| {
                    a.iter()
                        .filter(|o| o.as_reference().is_ok_and(|r| kept.contains(&r)))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            {
                let page = self.inner.get_dictionary_mut(id)?;
                page.set(
                    "MediaBox",
                    Object::Array(vec![0.into(), 0.into(), size.width.into(), size.height.into()]),
                );
                page.remove(b"CropBox");
                page.remove(b"Rotate");
                page.set(
                    "Resources",
                    Object::Dictionary(dictionary! { "XObject" => dictionary! { "FoliRaster" => xid } }),
                );
                if size.rotation != 0 {
                    page.remove(b"Annots");
                } else if with_annotations {
                    // Those drawn go; those of hidden layers stay.
                    if left.is_empty() {
                        page.remove(b"Annots");
                    } else {
                        page.set("Annots", Object::Array(left));
                    }
                }
            }
            self.inner.change_page_content(id, content.into_bytes())?;
        }
        self.inner.prune_objects();
        Ok(())
    }

    /// Merge comment appearances into the page content (vector, see [`crate::flatten`]).
    pub fn flatten_comments(&mut self, pages: &[usize]) -> Result<()> {
        self.flatten(pages, crate::flatten::FlattenScope::Comments)
            .map(|_| ())
    }
}
