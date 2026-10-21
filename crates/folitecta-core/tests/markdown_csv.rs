//! Documents made from Markdown and CSV files: the text set with its styles,
//! headings as bookmarks, links that work, tables over several pages with
//! their header repeated, numbers aligned right, wide tables turned
//! sideways; text beyond WinAnsi in the embedded font, and text that cannot
//! be drawn refused.

use folitecta_core::{Document, Error, GlyphProblem, SearchOptions, TextLayout};
use lopdf::Object;

const GUIDE: &str = "# User guide

An **important** introduction with *emphasis*, `code` and a [web link](https://example.org/guide).

## Installing

1. Download the package.
2. Unpack it:
   - on Linux with `tar`;
   - on Windows with the file manager.

> A quoted remark
> on two lines.

```
cargo build --release
    indented line
```

## Prices

| Item | Price |
|:-----|------:|
| Basic | 10.00 |
| Professional | 1,250.00 |

---

### Notes

~~Struck~~ text and a task list:

- [x] done
- [ ] to do
";

fn all_text(doc: &Document) -> String {
    (0..doc.page_count())
        .map(|p| doc.page_text(p))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The base fonts named in the resources of page `page`.
fn base_fonts(doc: &Document, page: usize) -> Vec<String> {
    let inner = doc.inner();
    let get = |o: &Object| match o {
        Object::Reference(id) => inner.get_object(*id).unwrap().clone(),
        other => other.clone(),
    };
    let page = inner.get_dictionary(doc.page_id(page).unwrap()).unwrap();
    let res = get(page.get(b"Resources").unwrap());
    let fonts = get(res.as_dict().unwrap().get(b"Font").unwrap());
    let mut out: Vec<String> = fonts
        .as_dict()
        .unwrap()
        .iter()
        .map(|(_, f)| {
            let f = get(f);
            String::from_utf8_lossy(f.as_dict().unwrap().get(b"BaseFont").unwrap().as_name().unwrap())
                .to_string()
        })
        .collect();
    out.sort();
    out
}

#[test]
fn markdown_is_set_with_its_styles_bookmarks_and_links() {
    let doc = Document::from_markdown(GUIDE, TextLayout::default(), "").unwrap();
    let text = all_text(&doc);
    for words in [
        "User guide",
        "An important introduction with emphasis, code and a web link.",
        "1.",
        "Download the package.",
        "on Linux with tar;",
        "A quoted remark on two lines.",
        "cargo build --release",
        "indented line",
        "Professional",
        "1,250.00",
        "Struck text",
        "[x] done",
    ] {
        assert!(text.contains(words), "{words:?} in {text}");
    }
    // Code keeps its spaces.
    let content = String::from_utf8_lossy(&doc.inner().get_page_content(doc.page_id(0).unwrap())).to_string();
    assert!(content.contains("(    indented line) Tj"), "{content}");
    // The standard faces, bold, italic and Courier among them.
    let fonts = base_fonts(&doc, 0);
    for f in ["Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Courier"] {
        assert!(fonts.iter().any(|b| b == f), "{f} in {fonts:?}");
    }
    // Headings are bookmarks, nested by level; the title is the first one.
    let outline = doc.outline();
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].title, "User guide");
    let titles: Vec<&str> = outline[0].children.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, ["Installing", "Prices"]);
    assert_eq!(outline[0].children[1].children[0].title, "Notes");
    assert_eq!(doc.info().title, "User guide");
    // The link is a link annotation over its words.
    let links = doc.links(0).unwrap();
    assert_eq!(links.len(), 1, "{links:?}");
    assert_eq!(links[0].target.describe(), "https://example.org/guide");
    let found = &doc.search("web link", SearchOptions::default())[0];
    let (r, l) = (found.rects[0], links[0].rect);
    assert!(
        (r[0] - l[0]).abs() < 3.0 && (r[2] - l[2]).abs() < 3.0,
        "{r:?} {l:?}"
    );
    // The right-aligned column: both prices end at the same place.
    let a = doc.search("10.00", SearchOptions::default())[0].rects[0];
    let b = doc.search("1,250.00", SearchOptions::default())[0].rects[0];
    assert!((a[2] - b[2]).abs() < 1.5, "{a:?} {b:?}");
}

#[test]
fn long_markdown_runs_onto_pages_with_lines_inside_them() {
    let para = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor. ".repeat(12);
    let md = (1..=40)
        .map(|i| format!("## Section {i}\n\n{para}\n"))
        .collect::<String>();
    let layout = TextLayout::default();
    let doc = Document::from_markdown(&md, layout, "Long").unwrap();
    assert!(doc.page_count() > 10, "{}", doc.page_count());
    assert_eq!(doc.outline().len(), 40);
    assert_eq!(doc.info().title, "Long");
    // Every line stays inside the margins.
    for p in 0..doc.page_count() {
        for hit in doc
            .search("tempor", SearchOptions::default())
            .iter()
            .filter(|h| h.page == p)
        {
            for r in &hit.rects {
                assert!(r[0] >= 17.0 && r[2] <= layout.page_width - 17.0, "{r:?}");
                assert!(r[1] >= 17.0 && r[3] <= layout.page_height - 17.0, "{r:?}");
            }
        }
    }
}

#[test]
fn markdown_beyond_winansi_uses_the_embedded_font() {
    let md = "# Łódź\n\n**Zażółć** gęślą *jaźń* — Ελληνικά, Кириллица.\n";
    let doc = Document::from_markdown(md, TextLayout::default(), "").unwrap();
    let text = doc.page_text(0);
    assert!(
        text.contains("Zażółć gęślą jaźń — Ελληνικά, Кириллица."),
        "{text}"
    );
    let fonts = base_fonts(&doc, 0);
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    assert!(fonts[0].ends_with("+FreeSans"), "{fonts:?}");
    assert_eq!(doc.outline()[0].title, "Łódź");
    let err = Document::from_markdown("# 漢字\n", TextLayout::default(), "").unwrap_err();
    assert!(
        matches!(err, Error::Glyphs(GlyphProblem::Missing { .. })),
        "{err:?}"
    );
}

fn ledger(rows: usize) -> String {
    let mut s = String::from("Cliente;Città;Importo;Data\n");
    for i in 0..rows {
        s.push_str(&format!(
            "Cliente {i};\"Roma; centro\";{}.{:03},{:02};2026-10-{:02}\n",
            i + 1,
            i * 7 % 1000,
            i % 100,
            i % 28 + 1
        ));
    }
    s
}

#[test]
fn csv_tables_repeat_their_header_and_align_numbers() {
    let doc = Document::from_csv(&ledger(150), None, TextLayout::default(), "Ledger").unwrap();
    assert!(doc.page_count() >= 3, "{}", doc.page_count());
    let size = doc.page_size(0).unwrap();
    assert!(size.height > size.width, "upright: {size:?}");
    for p in 0..doc.page_count() {
        let text = doc.page_text(p);
        assert!(
            text.contains("Cliente Città Importo Data") || text.contains("Cliente\tCittà"),
            "page {p}: {text}"
        );
    }
    let text = all_text(&doc);
    assert!(
        text.contains("Roma; centro") && text.contains("Cliente 149"),
        "{text}"
    );
    // Amounts aligned right: a short and a long one end at the same place.
    let right = |q: &str| doc.search(q, SearchOptions::default())[0].rects.last().unwrap()[2];
    assert!((right("1.000,00") - right("101.700,00")).abs() < 1.5);
    assert_eq!(doc.info().title, "Ledger");
}

#[test]
fn wide_csv_tables_turn_the_pages_sideways() {
    let head: Vec<String> = (0..14).map(|i| format!("Column number {i}")).collect();
    let row: Vec<String> = (0..14).map(|i| format!("value {i} here")).collect();
    let csv = format!("{}\n{}\n", head.join(","), row.join(","));
    let doc = Document::from_csv(&csv, None, TextLayout::default(), "").unwrap();
    let size = doc.page_size(0).unwrap();
    assert!(size.width > size.height, "sideways: {size:?}");
    assert!(doc.page_text(0).contains("value 13 here"));
}

#[test]
fn csv_without_records_or_with_text_the_font_lacks_is_refused() {
    assert!(Document::from_csv("\n\n", None, TextLayout::default(), "").is_err());
    let err = Document::from_csv("a,b\n漢,字\n", None, TextLayout::default(), "").unwrap_err();
    assert!(matches!(err, Error::Glyphs(_)), "{err:?}");
}
