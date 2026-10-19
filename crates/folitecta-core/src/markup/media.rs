//! Sound and video comments: Sound annotations (sampled sound, played as
//! WAV) and Screen annotations that play an embedded audio or video file
//! through a rendition action. Media embedded in rich media annotations is
//! read too.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, dict_text, fmt_num, pdf_literal};

/// A sound or video comment.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaItem {
    pub page: usize,
    pub index: usize,
    /// `sound`, `audio` or `video`.
    pub kind: String,
    pub title: String,
    pub file_name: String,
    pub mime: String,
    /// Size of the media data in bytes.
    pub size: usize,
}

/// Media types of file extensions Folitecta embeds.
pub fn mime_for(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_lowercase();
    Some(match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        _ => return None,
    })
}

fn kind_of(mime: &str) -> &'static str {
    if mime.starts_with("video/") {
        "video"
    } else {
        "audio"
    }
}

fn stream_data(s: &Stream) -> Vec<u8> {
    if s.dict.has(b"Filter") {
        s.decompressed_content().unwrap_or_else(|_| s.content.clone())
    } else {
        s.content.clone()
    }
}

/// Sampled sound of a Sound object: rate, channels, bits, encoding, samples.
struct Sampled {
    rate: u32,
    channels: u16,
    bits: u16,
    encoding: Vec<u8>,
    data: Vec<u8>,
}

fn mulaw(b: u8) -> i16 {
    let u = !b;
    let sign = u & 0x80;
    let exponent = (u >> 4) & 0x07;
    let mantissa = u & 0x0f;
    let magnitude = ((((mantissa as i32) << 3) + 0x84) << exponent) - 0x84;
    if sign != 0 {
        -(magnitude as i16)
    } else {
        magnitude as i16
    }
}

fn alaw(b: u8) -> i16 {
    let a = b ^ 0x55;
    let sign = a & 0x80;
    let exponent = (a >> 4) & 0x07;
    let mantissa = (a & 0x0f) as i32;
    let magnitude = if exponent == 0 {
        (mantissa << 4) + 8
    } else {
        ((mantissa << 4) + 0x108) << (exponent - 1)
    };
    if sign != 0 {
        magnitude as i16
    } else {
        -(magnitude as i16)
    }
}

/// A WAV file of PCM samples (little endian).
fn wav(rate: u32, channels: u16, bits: u16, pcm: &[u8]) -> Vec<u8> {
    let block = channels * bits.div_ceil(8);
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * block as u32).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

impl Sampled {
    /// The sound as a WAV file.
    fn to_wav(&self) -> Vec<u8> {
        match (self.encoding.as_slice(), self.bits) {
            (b"muLaw", _) | (b"ALaw", _) => {
                let law = self.encoding.as_slice() == b"muLaw";
                let mut pcm = Vec::with_capacity(self.data.len() * 2);
                for &b in &self.data {
                    let v = if law { mulaw(b) } else { alaw(b) };
                    pcm.extend_from_slice(&v.to_le_bytes());
                }
                wav(self.rate, self.channels, 16, &pcm)
            }
            (enc, 8) => {
                // WAV keeps 8-bit samples unsigned.
                let pcm: Vec<u8> = if enc == b"Signed" {
                    self.data.iter().map(|b| b.wrapping_add(128)).collect()
                } else {
                    self.data.clone()
                };
                wav(self.rate, self.channels, 8, &pcm)
            }
            (enc, bits) => {
                // Samples of more bytes are big endian in the PDF.
                let n = bits.div_ceil(8) as usize;
                let mut pcm = Vec::with_capacity(self.data.len());
                for chunk in self.data.chunks_exact(n) {
                    let mut v = chunk.to_vec();
                    if enc != b"Signed" {
                        // Unsigned values become signed ones.
                        v[0] ^= 0x80;
                    }
                    v.reverse();
                    pcm.extend_from_slice(&v);
                }
                wav(self.rate, self.channels, n as u16 * 8, &pcm)
            }
        }
    }
}

/// PCM samples of a WAV file: rate, channels, bits and little-endian samples.
pub fn parse_wav(data: &[u8]) -> Result<(u32, u16, u16, Vec<u8>)> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(Error::Invalid("not a WAV file".into()));
    }
    let mut pos = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let len = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
        let body = &data[pos + 8..(pos + 8 + len).min(data.len())];
        if id == b"fmt " && body.len() >= 16 {
            fmt = Some((
                u16::from_le_bytes([body[0], body[1]]),
                u16::from_le_bytes([body[2], body[3]]),
                u32::from_le_bytes([body[4], body[5], body[6], body[7]]),
                u16::from_le_bytes([body[14], body[15]]),
            ));
        } else if id == b"data" {
            let Some((format, channels, rate, bits)) = fmt else {
                return Err(Error::Invalid("the WAV file has no format".into()));
            };
            if format != 1 || !matches!(bits, 8 | 16 | 24 | 32) || channels == 0 {
                return Err(Error::Invalid(
                    "only uncompressed (PCM) WAV files can be used".into(),
                ));
            }
            return Ok((rate, channels, bits, body.to_vec()));
        }
        pos += 8 + len + (len & 1);
    }
    Err(Error::Invalid("the WAV file has no sound data".into()))
}

/// Embedded file of a file specification: name and data.
fn filespec_file(doc: &lopdf::Document, spec: &Object) -> Option<(String, Vec<u8>, Option<String>)> {
    let d = deref(doc, spec).as_dict().ok()?;
    let name = dict_text(doc, d, b"UF")
        .or_else(|| dict_text(doc, d, b"F"))
        .unwrap_or_default();
    let ef = deref(doc, d.get(b"EF").ok()?).as_dict().ok()?;
    let f = ef.get(b"UF").or_else(|_| ef.get(b"F")).ok()?;
    let s = deref(doc, f).as_stream().ok()?;
    let subtype = s
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string());
    Some((name, stream_data(s), subtype))
}

/// Media of an annotation dictionary: kind, file name, media type and data.
fn media_of(doc: &lopdf::Document, d: &Dictionary) -> Option<(String, String, String, Vec<u8>)> {
    match d.get(b"Subtype").ok()?.as_name().ok()? {
        b"Sound" => {
            let s = deref(doc, d.get(b"Sound").ok()?).as_stream().ok()?;
            let num = |k: &[u8], def: i64| s.dict.get(k).ok().and_then(|v| v.as_i64().ok()).unwrap_or(def);
            let rate = s.dict.get(b"R").ok().and_then(crate::util::num).unwrap_or(8000.0);
            let sampled = Sampled {
                rate: rate.round().clamp(1000.0, 384000.0) as u32,
                channels: num(b"C", 1).clamp(1, 8) as u16,
                bits: num(b"B", 8).clamp(8, 32) as u16,
                encoding: s
                    .dict
                    .get(b"E")
                    .ok()
                    .and_then(|e| e.as_name().ok())
                    .unwrap_or(b"Raw")
                    .to_vec(),
                data: stream_data(s),
            };
            Some((
                "sound".into(),
                "sound.wav".into(),
                "audio/wav".into(),
                sampled.to_wav(),
            ))
        }
        b"Screen" => {
            let action = deref(doc, d.get(b"A").ok()?).as_dict().ok()?;
            let rendition = deref(doc, action.get(b"R").ok()?).as_dict().ok()?;
            let clip = deref(doc, rendition.get(b"C").ok()?).as_dict().ok()?;
            let ct = dict_text(doc, clip, b"CT");
            let (name, data, sub) = filespec_file(doc, clip.get(b"D").ok()?)?;
            let mime = ct
                .or(sub)
                .or_else(|| mime_for(&name).map(String::from))
                .unwrap_or_default();
            Some((kind_of(&mime).into(), name, mime, data))
        }
        b"RichMedia" => {
            let content = deref(doc, d.get(b"RichMediaContent").ok()?).as_dict().ok()?;
            let assets = deref(doc, content.get(b"Assets").ok()?).as_dict().ok()?;
            let mut pairs = Vec::new();
            crate::util::walk_name_tree(doc, assets, &mut pairs, 0);
            pairs.into_iter().find_map(|(_, spec)| {
                let (name, data, sub) = filespec_file(doc, &spec)?;
                let mime = mime_for(&name).map(String::from).or(sub)?;
                (mime.starts_with("audio/") || mime.starts_with("video/"))
                    .then(|| (kind_of(&mime).into(), name, mime, data))
            })
        }
        _ => None,
    }
}

/// Appearance of a media comment: a frame with a play mark and the file name.
fn media_appearance(rect: [f32; 4], name: &str, video: bool) -> String {
    let f = fmt_num;
    let (x0, y0, x1, y1) = (rect[0], rect[1], rect[2], rect[3]);
    let (w, h) = (x1 - x0, y1 - y0);
    let mut c = String::new();
    let fill = if video { "0.12 0.12 0.14" } else { "0.93 0.95 1" };
    c.push_str(&format!("{fill} rg {} {} {} {} re f\n", f(x0), f(y0), f(w), f(h)));
    c.push_str(&format!(
        "0.45 0.45 0.5 RG 1 w {} {} {} {} re S\n",
        f(x0 + 0.5),
        f(y0 + 0.5),
        f(w - 1.0),
        f(h - 1.0)
    ));
    let s = (w.min(h) * 0.35).clamp(6.0, 40.0);
    let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0 + if h > 40.0 { 6.0 } else { 0.0 });
    let mark = if video { "1 1 1" } else { "0.31 0.27 0.9" };
    c.push_str(&format!(
        "{mark} rg {} {} m {} {} l {} {} l h f\n",
        f(cx - s * 0.4),
        f(cy - s * 0.5),
        f(cx - s * 0.4),
        f(cy + s * 0.5),
        f(cx + s * 0.5),
        f(cy)
    ));
    if h > 40.0 && w > 40.0 && !name.is_empty() {
        let size = 8.0f32.min(h / 6.0);
        let mut shown = name.to_string();
        while !shown.is_empty() && crate::util::helvetica_width(&shown, size) > w - 8.0 {
            shown.pop();
        }
        let tc = if video { "0.9 0.9 0.9" } else { "0.2 0.2 0.2" };
        let tw = crate::util::helvetica_width(&shown, size);
        c.push_str(&format!(
            "BT /Helv {} Tf {tc} rg {} {} Td {} Tj ET\n",
            f(size),
            f(cx - tw / 2.0),
            f(y0 + 5.0),
            String::from_utf8_lossy(&pdf_literal(&shown))
        ));
    }
    c
}

impl Document {
    /// Sound and video comments of the document.
    pub fn media_items(&self) -> Vec<MediaItem> {
        let doc = &self.inner;
        let mut out = Vec::new();
        for (p, pid) in self.page_ids().into_iter().enumerate() {
            let Ok(pd) = doc.get_dictionary(pid) else { continue };
            for (i, (_, d)) in crate::annots::annot_refs(doc, pd).into_iter().enumerate() {
                if let Some((kind, file_name, mime, data)) = media_of(doc, &d) {
                    out.push(MediaItem {
                        page: p,
                        index: i,
                        kind,
                        title: dict_text(doc, &d, b"T").unwrap_or_default(),
                        file_name,
                        mime,
                        size: data.len(),
                    });
                }
            }
        }
        out
    }

    /// Media data of a comment: file name, media type and bytes (sampled
    /// sound becomes a WAV file).
    pub fn media_data(&self, page: usize, index: usize) -> Option<(String, String, Vec<u8>)> {
        let id = self.annotation_object(page, index).ok()?;
        let d = self.inner.get_dictionary(id).ok()?;
        media_of(&self.inner, d).map(|(_, name, mime, data)| (name, mime, data))
    }

    /// A sound comment at `at` (PDF space) playing the samples of a WAV file.
    pub fn add_sound(
        &mut self,
        page: usize,
        at: (f32, f32),
        wav_data: &[u8],
        author: &str,
    ) -> Result<ObjectId> {
        let (rate, channels, bits, pcm) = parse_wav(wav_data)?;
        let page_id = self.page_id(page)?;
        // PDF samples of more than one byte are big endian.
        let n = (bits / 8) as usize;
        let data: Vec<u8> = if n > 1 {
            pcm.chunks_exact(n)
                .flat_map(|c| c.iter().rev().copied().collect::<Vec<u8>>())
                .collect()
        } else {
            pcm
        };
        let mut sound = Stream::new(
            dictionary! {
                "Type" => "Sound",
                "R" => rate as i64,
                "C" => channels as i64,
                "B" => bits as i64,
                "E" => if n > 1 { "Signed" } else { "Raw" },
            },
            data,
        );
        let _ = sound.compress();
        let r = [at.0, at.1 - 20.0, at.0 + 20.0, at.1];
        self.begin_edit();
        let sid = self.inner.add_object(sound);
        let mut annot = self.base_annotation(page_id, "Sound", r, author, "", [0.31, 0.27, 0.9]);
        annot.set("Sound", Object::Reference(sid));
        annot.set("Name", Object::Name(b"Speaker".to_vec()));
        let res = self.helv_resources();
        self.add_appearance(&mut annot, r, media_appearance(r, "", false), res);
        let id = self.push_drawn_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// A screen comment inside `rect` (PDF space) playing an embedded audio
    /// or video file.
    pub fn add_media(
        &mut self,
        page: usize,
        rect: [f32; 4],
        name: &str,
        data: Vec<u8>,
        author: &str,
    ) -> Result<ObjectId> {
        let mime =
            mime_for(name).ok_or_else(|| Error::Invalid("this kind of file cannot be played".into()))?;
        let page_id = self.page_id(page)?;
        let r = crate::geom::normalize(rect);
        let r = if r[2] - r[0] < 24.0 || r[3] - r[1] < 24.0 {
            [r[0], r[3] - 90.0, r[0] + 160.0, r[3]]
        } else {
            r
        };
        let video = mime.starts_with("video/");
        let size = data.len() as i64;
        self.begin_edit();
        let mut file = Stream::new(
            dictionary! {
                "Type" => "EmbeddedFile",
                "Subtype" => Object::Name(mime.as_bytes().to_vec()),
                "Params" => dictionary! { "Size" => size },
            },
            data,
        );
        let _ = file.compress();
        let fid = self.inner.add_object(file);
        let spec = dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal(name.as_bytes().to_vec()),
            "UF" => lopdf::text_string(name),
            "EF" => dictionary! { "F" => fid, "UF" => fid },
        };
        let clip = dictionary! {
            "Type" => "MediaClip",
            "S" => "MCD",
            "N" => lopdf::text_string(name),
            "CT" => Object::string_literal(mime.as_bytes().to_vec()),
            "D" => spec,
            "P" => dictionary! { "TF" => Object::string_literal(b"TEMPACCESS".to_vec()) },
        };
        let rendition =
            dictionary! { "Type" => "Rendition", "S" => "MR", "N" => lopdf::text_string(name), "C" => clip };
        let mut annot = self.base_annotation(page_id, "Screen", r, author, name, [0.31, 0.27, 0.9]);
        annot.set("T", lopdf::text_string(name));
        let res = self.helv_resources();
        self.add_appearance(&mut annot, r, media_appearance(r, name, video), res);
        let id = self.push_drawn_annotation(page_id, annot)?;
        // The rendition action names its own comment.
        let action = dictionary! { "S" => "Rendition", "OP" => 0, "R" => rendition, "AN" => id };
        self.inner
            .get_dictionary_mut(id)?
            .set("A", Object::Dictionary(action));
        self.commit()?;
        Ok(id)
    }

    /// Change the title and the description of a media comment.
    pub fn set_media_text(
        &mut self,
        page: usize,
        index: usize,
        title: &str,
        description: &str,
    ) -> Result<()> {
        if self.media_data(page, index).is_none() {
            return Err(Error::Invalid("the comment is not a sound or video".into()));
        }
        let id = self.annotation_object(page, index)?;
        self.begin_edit();
        let d = self.inner.get_dictionary_mut(id)?;
        for (k, v) in [("T", title), ("Contents", description)] {
            if v.trim().is_empty() {
                d.remove(k.as_bytes());
            } else {
                d.set(k, lopdf::text_string(v.trim()));
            }
        }
        self.commit()
    }

    fn helv_resources(&mut self) -> Dictionary {
        let font = self.helvetica_font_id();
        dictionary! { "Font" => dictionary! { "Helv" => font } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_round_trip_and_laws() {
        let pcm: Vec<u8> = (0..400u16)
            .flat_map(|i| ((i as i16 - 200) * 100).to_le_bytes())
            .collect();
        let file = wav(22050, 1, 16, &pcm);
        let (rate, ch, bits, back) = parse_wav(&file).unwrap();
        assert_eq!((rate, ch, bits), (22050, 1, 16));
        assert_eq!(back, pcm);
        // Big-endian signed samples of a PDF become the same WAV.
        let be: Vec<u8> = pcm.chunks(2).flat_map(|c| [c[1], c[0]]).collect();
        let s = Sampled {
            rate: 22050,
            channels: 1,
            bits: 16,
            encoding: b"Signed".to_vec(),
            data: be,
        };
        assert_eq!(s.to_wav(), file);
        assert_eq!(mulaw(0xff), 0);
        assert!(mulaw(0x00) < -30000);
        assert!(alaw(0xd5).abs() <= 8);
        assert!(parse_wav(b"RIFFxxxxWAVE").is_err());
        assert_eq!(mime_for("clip.MP4"), Some("video/mp4"));
        assert_eq!(mime_for("notes.txt"), None);
    }
}
