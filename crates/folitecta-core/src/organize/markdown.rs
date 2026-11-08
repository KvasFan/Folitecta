//! Markdown to PDF (FOL-L005): CommonMark with tables, strikethrough and
//! task lists, read with pulldown-cmark and set with [`crate::typeset`]:
//! headings (which become bookmarks), paragraphs with bold, italic, code,
//! struck text and links, lists, block quotes, code blocks, rules and tables.
//! A picture standing as a paragraph of its own is drawn, read from the
//! folder of the file (at 96 pixels to the inch, no wider than the text);
//! one within text, one that cannot be read and one on the web are shown by
//! their description. HTML is shown as text.

use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};

use std::path::Path;

use crate::document::Document;
use crate::error::Result;
use crate::textdoc::TextLayout;
use crate::typeset::{Align, Fonts, Sheet, Span, Style, Typeset, column_widths};

/// A block of a Markdown document.
#[derive(Debug, Clone, PartialEq)]
enum Block {
    Heading(u8, Vec<Span>),
    Paragraph(Vec<Span>),
    /// Text of a tight list item (no space around it).
    Plain(Vec<Span>),
    List {
        start: Option<u64>,
        items: Vec<Vec<Block>>,
    },
    Quote(Vec<Block>),
    Code(String),
    Rule,
    /// A picture as a paragraph of its own: where it is, its description.
    Picture {
        src: String,
        alt: String,
    },
    Table {
        aligns: Vec<Align>,
        head: Vec<Vec<Span>>,
        rows: Vec<Vec<Vec<Span>>>,
    },
}

type Events<'a> = std::iter::Peekable<Parser<'a>>;

/// The link the parts of a picture carry while a paragraph is read: this,
/// then where the picture is.
const PICTURE: &str = "\u{0}picture:";

fn is_inline_start(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. }
            | Tag::Image { .. }
    )
}

fn is_inline_end(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::Image
    )
}

/// The inline content up to the end of its block (consumed), or, for the
/// text of a tight list item, up to the next block (not consumed).
fn inlines(ev: &mut Events, implicit: bool) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let (mut bold, mut italic, mut strike) = (0u32, 0u32, 0u32);
    let mut links: Vec<String> = Vec::new();
    let mut images = 0u32;
    let push = |spans: &mut Vec<Span>, text: &str, style: Style, link: Option<String>| {
        if text.is_empty() {
            return;
        }
        match spans.last_mut() {
            Some(last) if last.style == style && last.link == link => last.text.push_str(text),
            _ => spans.push(Span {
                text: text.to_string(),
                style,
                link,
            }),
        }
    };
    loop {
        let style = Style {
            bold: bold > 0,
            italic: italic > 0 || images > 0,
            mono: false,
            strike: strike > 0,
        };
        let link = links.last().cloned();
        let Some(e) = ev.peek() else { break };
        match e {
            Event::Start(tag) if is_inline_start(tag) => {}
            Event::End(tag) if is_inline_end(tag) => {}
            Event::Start(_) | Event::Rule if implicit => break,
            Event::End(_) => {
                if !implicit {
                    ev.next();
                }
                break;
            }
            _ => {}
        }
        let Some(e) = ev.next() else { break };
        match e {
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold = bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic = italic.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => strike += 1,
            Event::End(TagEnd::Strikethrough) => strike = strike.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => links.push(dest_url.to_string()),
            Event::End(TagEnd::Link) => {
                links.pop();
            }
            // A picture is shown by its description, in brackets.
            Event::Start(Tag::Image { dest_url, .. }) => {
                images += 1;
                links.push(format!("{PICTURE}{dest_url}"));
                let link = links.last().cloned();
                push(
                    &mut spans,
                    "[",
                    Style {
                        italic: true,
                        ..style
                    },
                    link,
                );
            }
            Event::End(TagEnd::Image) => {
                images = images.saturating_sub(1);
                push(&mut spans, "]", style, link);
                links.pop();
            }
            Event::Text(t)
            | Event::Html(t)
            | Event::InlineHtml(t)
            | Event::InlineMath(t)
            | Event::DisplayMath(t) => push(&mut spans, &t, style, link),
            Event::Code(t) => push(&mut spans, &t, Style { mono: true, ..style }, link),
            Event::SoftBreak => push(&mut spans, " ", style, link),
            Event::HardBreak => push(&mut spans, "\n", style, link),
            Event::FootnoteReference(name) => push(&mut spans, &format!("[{name}]"), style, link),
            Event::TaskListMarker(done) => push(&mut spans, if done { "[x] " } else { "[ ] " }, style, link),
            _ => {}
        }
    }
    spans
}

/// [`inlines`] without the marks of pictures: their parts take the link
/// they stand in.
fn plain_inlines(ev: &mut Events, implicit: bool) -> Vec<Span> {
    unmark(inlines(ev, implicit))
}

fn unmark(spans: Vec<Span>) -> Vec<Span> {
    spans
        .into_iter()
        .map(|mut s| {
            if s.link.as_deref().is_some_and(|l| l.starts_with(PICTURE)) {
                s.link = None;
            }
            s
        })
        .collect()
}

/// A paragraph: a picture when it holds only one, else its text.
fn paragraph(ev: &mut Events) -> Block {
    let spans = inlines(ev, false);
    let srcs: Vec<&str> = spans
        .iter()
        .filter(|s| !s.text.trim().is_empty())
        .map(|s| {
            s.link
                .as_deref()
                .and_then(|l| l.strip_prefix(PICTURE))
                .unwrap_or("")
        })
        .collect();
    match srcs.first() {
        Some(src) if !src.is_empty() && srcs.iter().all(|s| s == src) => {
            let text: String = spans.iter().map(|s| s.text.as_str()).collect();
            let alt = text
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
            Block::Picture {
                src: src.to_string(),
                alt,
            }
        }
        _ => Block::Paragraph(unmark(spans)),
    }
}

/// The text of a code block or an HTML block, up to its end.
fn raw_text(ev: &mut Events) -> String {
    let mut text = String::new();
    for e in ev.by_ref() {
        match e {
            Event::Text(t) | Event::Html(t) | Event::Code(t) => text.push_str(&t),
            Event::End(_) => break,
            _ => {}
        }
    }
    text.trim_end_matches('\n').to_string()
}

/// The cells of a table row, up to its end.
fn cells(ev: &mut Events) -> Vec<Vec<Span>> {
    let mut out = Vec::new();
    while let Some(e) = ev.next() {
        match e {
            Event::Start(Tag::TableCell) => out.push(plain_inlines(ev, false)),
            Event::End(_) => break,
            _ => {}
        }
    }
    out
}

/// Blocks up to the end of their container (consumed), or to the end of
/// the document.
fn blocks(ev: &mut Events) -> Vec<Block> {
    let mut out = Vec::new();
    while let Some(e) = ev.peek() {
        match e {
            Event::Start(tag) if is_inline_start(tag) => {
                out.push(Block::Plain(plain_inlines(ev, true)));
                continue;
            }
            Event::Text(_)
            | Event::Code(_)
            | Event::InlineHtml(_)
            | Event::InlineMath(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::FootnoteReference(_)
            | Event::TaskListMarker(_) => {
                out.push(Block::Plain(plain_inlines(ev, true)));
                continue;
            }
            _ => {}
        }
        let Some(e) = ev.next() else { break };
        match e {
            Event::End(_) => break,
            Event::Rule => out.push(Block::Rule),
            Event::Html(t) | Event::DisplayMath(t) => out.push(Block::Code(t.trim_end().to_string())),
            Event::Start(tag) => match tag {
                Tag::Paragraph => out.push(paragraph(ev)),
                Tag::Heading { level, .. } => out.push(Block::Heading(level as u8, plain_inlines(ev, false))),
                Tag::BlockQuote(_) => out.push(Block::Quote(blocks(ev))),
                Tag::CodeBlock(_) | Tag::HtmlBlock => out.push(Block::Code(raw_text(ev))),
                Tag::List(start) => {
                    let mut items = Vec::new();
                    while let Some(e) = ev.next() {
                        match e {
                            Event::Start(Tag::Item) => items.push(blocks(ev)),
                            Event::End(_) => break,
                            _ => {}
                        }
                    }
                    out.push(Block::List { start, items });
                }
                Tag::Table(aligns) => {
                    let aligns = aligns
                        .iter()
                        .map(|a| match a {
                            Alignment::Center => Align::Center,
                            Alignment::Right => Align::Right,
                            _ => Align::Left,
                        })
                        .collect();
                    let mut head = Vec::new();
                    let mut rows = Vec::new();
                    while let Some(e) = ev.next() {
                        match e {
                            Event::Start(Tag::TableHead) => head = cells(ev),
                            Event::Start(Tag::TableRow) => rows.push(cells(ev)),
                            Event::End(_) => break,
                            _ => {}
                        }
                    }
                    out.push(Block::Table { aligns, head, rows });
                }
                // Footnotes, definitions and front matter: their blocks as they are.
                _ => out.extend(blocks(ev)),
            },
            _ => {}
        }
    }
    out
}

fn parse(text: &str) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    blocks(&mut Parser::new_ext(text, opts).peekable())
}

/// Every text the blocks show, for the choice of the fonts.
fn texts<'a>(blocks: &'a [Block], out: &mut Vec<&'a str>) {
    let spans = |s: &'a [Span], out: &mut Vec<&'a str>| out.extend(s.iter().map(|s| s.text.as_str()));
    for b in blocks {
        match b {
            Block::Heading(_, s) | Block::Paragraph(s) | Block::Plain(s) => spans(s, out),
            Block::List { items, .. } => items.iter().for_each(|i| texts(i, out)),
            Block::Quote(b) => texts(b, out),
            Block::Code(t) => out.push(t),
            Block::Picture { alt, .. } => out.push(alt),
            Block::Rule => {}
            Block::Table { head, rows, .. } => {
                head.iter().for_each(|c| spans(c, out));
                rows.iter().flatten().for_each(|c| spans(c, out));
            }
        }
    }
}

fn plain_text(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

const TEXT: [f32; 3] = [0.1, 0.1, 0.12];
const QUOTE: [f32; 3] = [0.32, 0.34, 0.38];

/// Sizes of headings, by level, as a share of the body size.
const HEADING: [f32; 6] = [2.0, 1.6, 1.3, 1.15, 1.0, 0.95];

#[derive(Clone, Copy)]
struct Ctx<'a> {
    size: f32,
    x0: f32,
    width: f32,
    color: [f32; 3],
    depth: usize,
    /// The folder pictures are read from.
    folder: Option<&'a Path>,
}

/// The picture `src` (a path, from `folder` when relative), with its size
/// in pixels; none for addresses on the web and what cannot be read.
fn picture(src: &str, folder: Option<&Path>) -> Option<(Vec<u8>, (f32, f32))> {
    let folder = folder?;
    let src = src.strip_prefix("file://").unwrap_or(src);
    if src.contains("://") || src.starts_with("data:") || src.is_empty() {
        return None;
    }
    let decoded = src.replace("%20", " ");
    let path = folder.join(decoded);
    let data = std::fs::read(path).ok()?;
    let image = crate::images::image_object(data.clone()).ok()?;
    Some((data, (image.width, image.height)))
}

fn render(ts: &mut Typeset, blocks: &[Block], cx: &Ctx) -> Result<()> {
    let size = cx.size;
    let leading = size * 1.42;
    for (i, b) in blocks.iter().enumerate() {
        match b {
            Block::Heading(level, spans) => {
                let l = (*level).clamp(1, 6);
                let hs = cx.size * HEADING[usize::from(l) - 1];
                ts.space(hs * 0.7);
                // A heading stays with the line after it.
                ts.need(hs * 1.3 + leading * 2.0);
                ts.mark(l, &plain_text(spans));
                let bold: Vec<Span> = spans
                    .iter()
                    .map(|s| Span {
                        style: Style {
                            bold: true,
                            ..s.style
                        },
                        ..s.clone()
                    })
                    .collect();
                ts.paragraph(&bold, hs, hs * 1.3, cx.x0, cx.width, cx.color, Align::Left)?;
                if l <= 2 {
                    ts.rule(cx.x0, cx.x0 + cx.width, hs * 0.15, 0.8);
                }
                ts.space(hs * 0.3);
            }
            Block::Paragraph(spans) => {
                ts.paragraph(spans, size, leading, cx.x0, cx.width, cx.color, Align::Left)?;
                ts.space(size * 0.6);
            }
            Block::Plain(spans) => {
                ts.paragraph(spans, size, leading, cx.x0, cx.width, cx.color, Align::Left)?;
                // Tight items: a little room only before what follows inside them.
                if i + 1 < blocks.len() {
                    ts.space(size * 0.2);
                }
            }
            Block::List { start, items } => {
                let indent = size * 1.7;
                for (n, item) in items.iter().enumerate() {
                    let marker = match start {
                        Some(s) => format!("{}.", s + n as u64),
                        None => if cx.depth.is_multiple_of(2) { "•" } else { "–" }.to_string(),
                    };
                    ts.marker(&marker, size, leading, cx.x0 + indent - size * 0.45, cx.color)?;
                    let inner = Ctx {
                        x0: cx.x0 + indent,
                        width: (cx.width - indent).max(size * 4.0),
                        depth: cx.depth + 1,
                        ..*cx
                    };
                    render(ts, item, &inner)?;
                }
                ts.space(size * 0.5);
            }
            Block::Quote(inner) => {
                ts.bars.push(cx.x0 + size * 0.3);
                let q = Ctx {
                    x0: cx.x0 + size * 1.2,
                    width: (cx.width - size * 1.2).max(size * 4.0),
                    color: QUOTE,
                    ..*cx
                };
                let result = render(ts, inner, &q);
                ts.bars.pop();
                result?;
                ts.space(size * 0.5);
            }
            Block::Code(text) => {
                let cs = size * 0.88;
                let mono = Style {
                    mono: true,
                    ..Style::default()
                };
                let pad = cs * 0.6;
                let mut lines = Vec::new();
                for raw in text.split('\n') {
                    // Code lines are not reflowed, only cut when too long.
                    let raw = raw.replace('\t', "    ");
                    lines.extend(ts.cut_lines(&raw, mono, cs, cx.width - 2.0 * pad));
                }
                ts.boxed_lines(&lines, cs, cs * 1.35, cx.x0, cx.width, pad, cx.color)?;
                ts.space(size * 0.6);
            }
            Block::Rule => {
                ts.rule(cx.x0, cx.x0 + cx.width, size * 0.6, 0.75);
            }
            Block::Picture { src, alt } => match picture(src, cx.folder) {
                Some((data, (pw, ph))) => {
                    // 96 pixels to the inch, no wider than the text nor taller
                    // than most of the page.
                    let (w, h) = (pw * 0.75, ph * 0.75);
                    let tallest = (ts.sheet.height - 2.0 * ts.sheet.margin) * 0.8;
                    let s = (cx.width / w).min(tallest / h).min(1.0);
                    ts.picture(data, w * s, h * s, cx.x0);
                    ts.space(size * 0.8);
                }
                None => {
                    let shown = Span::styled(
                        format!("[{alt}]"),
                        Style {
                            italic: true,
                            ..Style::default()
                        },
                    );
                    ts.paragraph(&[shown], size, leading, cx.x0, cx.width, cx.color, Align::Left)?;
                    ts.space(size * 0.6);
                }
            },
            Block::Table { aligns, head, rows } => {
                let ts_size = size * 0.92;
                let pad = ts_size * 0.45;
                let widths = column_widths(ts, head, rows, ts_size, cx.width, pad);
                let head = (!head.is_empty()).then_some(head.as_slice());
                ts.table(head, rows, &widths, aligns, ts_size, ts_size * 1.3, cx.x0, pad)?;
                ts.space(size * 0.8);
            }
        }
    }
    Ok(())
}

impl Document {
    /// A new document made from Markdown `text`, on pages of `layout`; the
    /// headings become bookmarks. Text that cannot be drawn is refused.
    /// Pictures are shown by their description (see [`Document::from_markdown_in`]).
    pub fn from_markdown(text: &str, layout: TextLayout, title: &str) -> Result<Document> {
        Document::from_markdown_in(text, layout, title, None)
    }

    /// [`Document::from_markdown`] with the pictures that stand as
    /// paragraphs drawn, read from `folder` (the folder of the file).
    pub fn from_markdown_in(
        text: &str,
        layout: TextLayout,
        title: &str,
        folder: Option<&Path>,
    ) -> Result<Document> {
        let blocks = parse(text);
        let mut all = vec!["•–0123456789.[]x "];
        texts(&blocks, &mut all);
        let fonts = Fonts::new(all.iter().copied())?;
        let sheet = Sheet {
            width: layout.page_width,
            height: layout.page_height,
            margin: layout
                .margin
                .clamp(18.0, layout.page_width.min(layout.page_height) / 4.0),
        };
        let mut ts = Typeset::new(sheet, fonts);
        let cx = Ctx {
            size: layout.font_size.clamp(6.0, 24.0),
            x0: ts.left(),
            width: ts.right() - ts.left(),
            color: TEXT,
            depth: 0,
            folder,
        };
        render(&mut ts, &blocks, &cx)?;
        // The title: the one given, else the first heading.
        let first = blocks.iter().find_map(|b| match b {
            Block::Heading(_, s) => Some(plain_text(s)),
            _ => None,
        });
        let title = if title.is_empty() {
            first.unwrap_or_default()
        } else {
            title.to_string()
        };
        ts.finish(&title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_of_a_document() {
        let md = "# Title\n\nSome *soft* and **bold** text with `code` and a [link](https://example.org).\n\n\
                  - one\n- two\n  - nested\n\n1. first\n2. second\n\n> quoted\n\n```\nfn main() {}\n```\n\n---\n\n\
                  | A | B |\n|:--|--:|\n| 1 | 2 |\n";
        let b = parse(md);
        assert!(matches!(&b[0], Block::Heading(1, s) if plain_text(s) == "Title"));
        let Block::Paragraph(p) = &b[1] else {
            panic!("{b:?}")
        };
        assert!(p.iter().any(|s| s.style.italic && s.text == "soft"));
        assert!(p.iter().any(|s| s.style.bold && s.text == "bold"));
        assert!(p.iter().any(|s| s.style.mono && s.text == "code"));
        assert!(
            p.iter()
                .any(|s| s.link.as_deref() == Some("https://example.org") && s.text == "link")
        );
        let Block::List { start: None, items } = &b[2] else {
            panic!("{b:?}")
        };
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[1][0], Block::Plain(s) if plain_text(s) == "two"));
        assert!(matches!(&items[1][1], Block::List { .. }));
        assert!(matches!(&b[3], Block::List { start: Some(1), items } if items.len() == 2));
        assert!(matches!(&b[4], Block::Quote(q) if matches!(&q[0], Block::Paragraph(_))));
        assert!(matches!(&b[5], Block::Code(c) if c == "fn main() {}"));
        assert!(matches!(&b[6], Block::Rule));
        let Block::Table { aligns, head, rows } = &b[7] else {
            panic!("{b:?}")
        };
        assert_eq!(aligns, &[Align::Left, Align::Right]);
        assert_eq!(head.len(), 2);
        assert_eq!(plain_text(&rows[0][1]), "2");
    }

    #[test]
    fn task_lists_and_pictures() {
        let b =
            parse("- [x] done\n- [ ] to do\n\n![a chart](chart.png)\n\nSee ![a chart](chart.png) here.\n");
        let Block::List { items, .. } = &b[0] else {
            panic!("{b:?}")
        };
        assert!(matches!(&items[0][0], Block::Plain(s) if plain_text(s) == "[x] done"));
        // A picture on its own is a picture; within text, its description.
        assert_eq!(
            b[1],
            Block::Picture {
                src: "chart.png".into(),
                alt: "a chart".into()
            }
        );
        let Block::Paragraph(s) = &b[2] else {
            panic!("{b:?}")
        };
        assert_eq!(plain_text(s), "See [a chart] here.");
        assert!(
            s.iter()
                .any(|x| x.text.contains("a chart") && x.style.italic && x.link.is_none())
        );
    }
}
