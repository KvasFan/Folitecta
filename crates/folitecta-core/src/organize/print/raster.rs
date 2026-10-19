//! Printing as an image: every sheet of a print file drawn as a picture on
//! a page of the same size (for printers that misdraw a file, and for
//! documents that may only print at a low resolution).

use std::io::Write;
use std::sync::Arc;

use lopdf::{Dictionary, Object, Stream};

use crate::error::{Error, Result};
use crate::render::{RenderOpts, Renderer};

fn flate(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    // Writing to memory does not fail.
    let _ = enc.write_all(data);
    enc.finish().unwrap_or_default()
}

/// The memory the compressed pictures of a print file may take. The job
/// holds them about three times at its peak (in the file being made, in
/// the file written, then in the check that reads it again), so about
/// 1.5 GB at most.
pub const PICTURES_BUDGET: usize = 512 << 20;

/// How drawing the sheets as pictures ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pictures {
    /// The print file of the pictures.
    Done(Vec<u8>),
    Cancelled,
    /// The pictures grew past the memory budget (lower the resolution or
    /// print fewer pages).
    TooLarge,
}

/// The sheets of the print file `pdf` as pictures at `dpi` (8-bit grey or
/// RGB), one page of the same size for each, while the compressed pictures
/// stay within `budget` bytes. `progress(done, total)` is called before each
/// sheet; when it returns false the work stops.
pub fn sheets_as_images(
    pdf: Arc<Vec<u8>>,
    dpi: f32,
    gray: bool,
    budget: usize,
    progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<Pictures> {
    if !(dpi.is_finite() && (36.0..=1200.0).contains(&dpi)) {
        return Err(Error::Invalid(format!(
            "{dpi} dpi is not a resolution to print at"
        )));
    }
    let renderer = Renderer::new(pdf)?;
    let sizes = renderer.page_sizes();
    let total = sizes.len();
    let mut out = lopdf::Document::with_version("1.7");
    let pages_id = out.new_object_id();
    let mut kids = Vec::with_capacity(total);
    let mut held = 0usize;
    for (i, (w, h)) in sizes.into_iter().enumerate() {
        if !progress(i, total) {
            return Ok(Pictures::Cancelled);
        }
        let img = renderer.render_rgba(
            i,
            dpi / 72.0,
            RenderOpts {
                annotations: true,
                transparent: false,
            },
        )?;
        let (pw, ph) = img.dimensions();
        let raw = img.into_raw();
        let data: Vec<u8> = if gray {
            raw.as_chunks::<4>()
                .0
                .iter()
                .map(|p| {
                    ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114 + 500) / 1000)
                        as u8
                })
                .collect()
        } else {
            raw.as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect()
        };
        let mut image = Dictionary::new();
        image.set("Type", Object::Name(b"XObject".to_vec()));
        image.set("Subtype", Object::Name(b"Image".to_vec()));
        image.set("Width", Object::Integer(i64::from(pw)));
        image.set("Height", Object::Integer(i64::from(ph)));
        image.set(
            "ColorSpace",
            Object::Name(if gray {
                b"DeviceGray".to_vec()
            } else {
                b"DeviceRGB".to_vec()
            }),
        );
        image.set("BitsPerComponent", Object::Integer(8));
        image.set("Filter", Object::Name(b"FlateDecode".to_vec()));
        let packed = flate(&data);
        drop(data);
        held += packed.len();
        if held > budget {
            return Ok(Pictures::TooLarge);
        }
        let image_id = out.add_object(Stream::new(image, packed));
        let content = format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q\n");
        let contents = out.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
        let mut xobjects = Dictionary::new();
        xobjects.set("Im0", Object::Reference(image_id));
        let mut res = Dictionary::new();
        res.set("XObject", Object::Dictionary(xobjects));
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), Object::Real(w), Object::Real(h)]),
        );
        page.set("Resources", Object::Dictionary(res));
        page.set("Contents", Object::Reference(contents));
        kids.push(Object::Reference(out.add_object(page)));
    }
    let mut tree = Dictionary::new();
    tree.set("Type", Object::Name(b"Pages".to_vec()));
    tree.set("Count", Object::Integer(kids.len() as i64));
    tree.set("Kids", Object::Array(kids));
    out.objects.insert(pages_id, Object::Dictionary(tree));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = out.add_object(catalog);
    out.trailer.set("Root", Object::Reference(catalog_id));
    let mut bytes = Vec::new();
    out.save_to(&mut bytes)?;
    Ok(Pictures::Done(bytes))
}
