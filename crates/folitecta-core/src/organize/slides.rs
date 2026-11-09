//! Pages as a presentation (FOL-L009): a PowerPoint file (PPTX) with a
//! slide for each page, the size of the first page; on it the picture of
//! the page as it is shown (comments and fields included), as large as the
//! slide holds, its text as the description of the picture (for search and
//! screen readers) and, paragraph by paragraph, as the notes of the slide.
//! Nothing else of the page is editable.

use std::ops::Range;

use crate::document::Document;
use crate::error::{Error, Result};
use crate::office::clean;
use crate::xml::{escape_attr, escape_text};
use crate::zip::{ZipEntry, write};

/// EMU to the point.
const EMU: f64 = 12700.0;
/// Pixels to the point the pages are drawn at (144 dpi).
const SCALE: f32 = 2.0;
/// Characters of the text of a page kept as the description of its picture.
const DESCRIPTION: usize = 2000;
/// Characters of the text of a page kept as the notes of its slide.
const NOTES: usize = 20_000;

const NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
                  xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
                  xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn rels(items: &[(String, &str, String)]) -> Vec<u8> {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for (id, kind, target) in items {
        out.push_str(&format!(
            "<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{}\"/>",
            escape_attr(target)
        ));
    }
    out.push_str("</Relationships>");
    out.into_bytes()
}

/// An empty shape tree, as masters and layouts have.
const EMPTY_TREE: &str = "<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
     <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr></p:spTree>";

fn theme() -> String {
    let colour = |name: &str, rgb: &str| format!("<a:{name}><a:srgbClr val=\"{rgb}\"/></a:{name}>");
    let colours = [
        ("dk1", "000000"),
        ("lt1", "FFFFFF"),
        ("dk2", "1F2937"),
        ("lt2", "E5E7EB"),
        ("accent1", "2563EB"),
        ("accent2", "DC2626"),
        ("accent3", "16A34A"),
        ("accent4", "CA8A04"),
        ("accent5", "9333EA"),
        ("accent6", "0891B2"),
        ("hlink", "1D4ED8"),
        ("folHlink", "7C3AED"),
    ]
    .iter()
    .map(|(n, c)| colour(n, c))
    .collect::<String>();
    let fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>";
    let line = "<a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>";
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Folitecta\"><a:themeElements>\
         <a:clrScheme name=\"Folitecta\">{colours}</a:clrScheme>\
         <a:fontScheme name=\"Folitecta\"><a:majorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont>\
         <a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme>\
         <a:fmtScheme name=\"Folitecta\"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst>\
         <a:lnStyleLst>{line}{line}{line}</a:lnStyleLst>\
         <a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>\
         <a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme>\
         </a:themeElements></a:theme>"
    )
}

/// The slide of a picture of `pixels` and `size` (points) on a slide of
/// `slide` (EMU), centred and as large as it fits; `description` its text.
fn slide(n: usize, size: (f32, f32), slide: (i64, i64), description: &str) -> String {
    let (w, h) = (size.0 as f64 * EMU, size.1 as f64 * EMU);
    let s = (slide.0 as f64 / w).min(slide.1 as f64 / h);
    let (cx, cy) = ((w * s).round() as i64, (h * s).round() as i64);
    let (x, y) = ((slide.0 - cx) / 2, (slide.1 - cy) / 2);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <p:sld {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>\
         <p:pic><p:nvPicPr><p:cNvPr id=\"2\" name=\"Page {n}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr>\
         <p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>\
         <p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>\
         </p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>",
        escape_attr(&clean(description))
    )
}

/// The notes master: where the picture of the slide and the notes stand on
/// a notes page.
fn notes_master() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <p:notesMaster {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>\
         <p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Slide image\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldImg\" idx=\"2\"/></p:nvPr></p:nvSpPr>\
         <p:spPr><a:xfrm><a:off x=\"1143000\" y=\"685800\"/><a:ext cx=\"4572000\" cy=\"3429000\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:sp>\
         <p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Notes\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" sz=\"quarter\" idx=\"3\"/></p:nvPr></p:nvSpPr>\
         <p:spPr><a:xfrm><a:off x=\"685800\" y=\"4400550\"/><a:ext cx=\"5486400\" cy=\"3600450\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr>\
         <p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp>\
         </p:spTree></p:cSld>\
         <p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>\
         </p:notesMaster>"
    )
}

/// The notes of slide `n`: the picture of the slide, and `paragraphs`.
fn notes(n: usize, paragraphs: &[String]) -> String {
    let text: String = if paragraphs.is_empty() {
        "<a:p><a:endParaRPr lang=\"en-US\"/></a:p>".into()
    } else {
        paragraphs
            .iter()
            .map(|p| {
                format!(
                    "<a:p><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>",
                    escape_text(&clean(p))
                )
            })
            .collect()
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <p:notes {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
         <p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>\
         <p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Slide image {n}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldImg\"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp>\
         <p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Notes {n}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/>\
         <p:txBody><a:bodyPr/><a:lstStyle/>{text}</p:txBody></p:sp>\
         </p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"
    )
}

impl Document {
    /// `pages` as a PowerPoint presentation (see the module); how many
    /// slides it has.
    pub(crate) fn export_slides(&self, pages: Range<usize>) -> Result<(Vec<u8>, usize)> {
        let first = self.page_size(pages.start)?;
        // PowerPoint takes slides from 1 to 56 inches a side.
        let side = |pt: f32| ((pt as f64 * EMU).round() as i64).clamp(914_400, 51_206_400);
        let slide_size = (side(first.width), side(first.height));
        let mut entries = Vec::new();
        let mut list = String::new();
        let mut pres_rels: Vec<(String, &str, String)> = vec![
            (
                "rId1".into(),
                "slideMaster",
                "slideMasters/slideMaster1.xml".into(),
            ),
            ("rId2".into(), "theme", "theme/theme1.xml".into()),
            (
                "rId3".into(),
                "notesMaster",
                "notesMasters/notesMaster1.xml".into(),
            ),
        ];
        let mut types = String::new();
        let layers = self.text_layers();
        let blocks = self.reading_blocks(pages.clone());
        for (k, page) in pages.clone().enumerate() {
            let n = k + 1;
            let size = self.page_size(page)?;
            let picture = self.render_page_png(page, SCALE)?;
            let text: String = layers
                .get(page)
                .map(|l| l.text.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
                .chars()
                .take(DESCRIPTION)
                .collect();
            entries.push(ZipEntry::new(
                format!("ppt/slides/slide{n}.xml"),
                slide(n, (size.width, size.height), slide_size, &text).into_bytes(),
            ));
            entries.push(ZipEntry::new(
                format!("ppt/slides/_rels/slide{n}.xml.rels"),
                rels(&[
                    (
                        "rId1".into(),
                        "slideLayout",
                        "../slideLayouts/slideLayout1.xml".into(),
                    ),
                    ("rId2".into(), "image", format!("../media/page{n}.png")),
                    (
                        "rId3".into(),
                        "notesSlide",
                        format!("../notesSlides/notesSlide{n}.xml"),
                    ),
                ]),
            ));
            // The notes: the paragraphs of the page, as far as they go.
            let mut left = NOTES;
            let paragraphs: Vec<String> = blocks
                .iter()
                .filter(|b| b.page == page && !b.text.trim().is_empty())
                .map_while(|b| {
                    let text: String = b.text.trim().chars().take(left).collect();
                    left = left.saturating_sub(text.chars().count());
                    (!text.is_empty()).then_some(text)
                })
                .collect();
            entries.push(ZipEntry::new(
                format!("ppt/notesSlides/notesSlide{n}.xml"),
                notes(n, &paragraphs).into_bytes(),
            ));
            entries.push(ZipEntry::new(
                format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                rels(&[
                    (
                        "rId1".into(),
                        "notesMaster",
                        "../notesMasters/notesMaster1.xml".into(),
                    ),
                    ("rId2".into(), "slide", format!("../slides/slide{n}.xml")),
                ]),
            ));
            entries.push(ZipEntry::new(format!("ppt/media/page{n}.png"), picture.png));
            list.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rIdS{n}\"/>", 255 + n));
            pres_rels.push((format!("rIdS{n}"), "slide", format!("slides/slide{n}.xml")));
            types.push_str(&format!(
                "<Override PartName=\"/ppt/slides/slide{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>\
                 <Override PartName=\"/ppt/notesSlides/notesSlide{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>"
            ));
        }
        let count = pages.len();
        if count == 0 {
            return Err(Error::Invalid("there are no pages to export".into()));
        }
        let presentation = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <p:presentation {NS}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst>\
             <p:notesMasterIdLst><p:notesMasterId r:id=\"rId3\"/></p:notesMasterIdLst>\
             <p:sldIdLst>{list}</p:sldIdLst><p:sldSz cx=\"{}\" cy=\"{}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>",
            slide_size.0, slide_size.1
        );
        let master = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <p:sldMaster {NS}><p:cSld>{EMPTY_TREE}</p:cSld>\
             <p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>\
             <p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst></p:sldMaster>"
        );
        let layout = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <p:sldLayout {NS} type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\">{EMPTY_TREE}</p:cSld>\
             <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"
        );
        let title = self.info().title;
        let mut all = vec![
            ZipEntry::new(
                "[Content_Types].xml",
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
                     <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
                     <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
                     <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
                     <Default Extension=\"png\" ContentType=\"image/png\"/>\
                     <Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>\
                     <Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/>\
                     <Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/>\
                     <Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>\
                     <Override PartName=\"/ppt/theme/theme2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>\
                     <Override PartName=\"/ppt/notesMasters/notesMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml\"/>\
                     <Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>\
                     {types}</Types>"
                )
                .into_bytes(),
            ),
            ZipEntry::new(
                "_rels/.rels",
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
                 <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/>\
                 <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
                 </Relationships>"
                    .as_bytes()
                    .to_vec(),
            ),
            ZipEntry::new(
                "docProps/core.xml",
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
                     <cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
                     xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title></cp:coreProperties>",
                    escape_text(&clean(&title))
                )
                .into_bytes(),
            ),
            ZipEntry::new("ppt/presentation.xml", presentation.into_bytes()),
            ZipEntry::new("ppt/_rels/presentation.xml.rels", rels(&pres_rels)),
            ZipEntry::new("ppt/slideMasters/slideMaster1.xml", master.into_bytes()),
            ZipEntry::new(
                "ppt/slideMasters/_rels/slideMaster1.xml.rels",
                rels(&[
                    ("rId1".into(), "slideLayout", "../slideLayouts/slideLayout1.xml".into()),
                    ("rId2".into(), "theme", "../theme/theme1.xml".into()),
                ]),
            ),
            ZipEntry::new("ppt/slideLayouts/slideLayout1.xml", layout.into_bytes()),
            ZipEntry::new(
                "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
                rels(&[("rId1".into(), "slideMaster", "../slideMasters/slideMaster1.xml".into())]),
            ),
            ZipEntry::new("ppt/theme/theme1.xml", theme().into_bytes()),
            ZipEntry::new("ppt/notesMasters/notesMaster1.xml", notes_master().into_bytes()),
            ZipEntry::new(
                "ppt/notesMasters/_rels/notesMaster1.xml.rels",
                rels(&[("rId1".into(), "theme", "../theme/theme2.xml".into())]),
            ),
            ZipEntry::new("ppt/theme/theme2.xml", theme().into_bytes()),
        ];
        all.append(&mut entries);
        Ok((write(&all)?, count))
    }
}
