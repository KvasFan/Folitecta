## Folitecta 0.73.0 — documents that leave whole

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release holds the
work planned as 0.44.0 to 0.73.0; 0.45.0 was prepared but never published, and comes with it.

- **Office exports that keep the page**: the export to Word (DOCX), rich text (RTF) and web page
  (HTML) carries the pictures of the pages where they stand, the tables found on them as tables
  (columns as wide as their contents, cells wrapped on several lines kept whole), bullet and
  numbered lists as lists, and the values typed in form fields in their place. Pages can also be
  exported as a PowerPoint presentation (PPTX), one slide a page.
- **Export Tables**: the tables in the text of the pages, those of filled forms included, written
  as an Excel workbook (XLSX, numbers kept as numbers) or as CSV.
- **Forms**: Find Form Fields proposes the fields a flat form leaves room for (lines, boxes, check
  boxes, combs), each named after the words beside it; picture fields take a picture when clicked.
- **A cleaner file**: Merge Duplicate Fonts and Pictures, Remove Hidden Information (metadata,
  attachments, hidden content and risky scripts at once) and an Object Inspector that shows the
  objects of the file as they are written.
- **Review and measure**: the history of a comment, a Measurements pane with totals, Compare
  Documents finding pictures added, removed, replaced or moved.
- **Drawing and scans**: snapping to the corners and line ends of the page, nodes added and removed
  in vector paths, sheets askew and sideways straightened and turned in one step, areas left out of
  text recognition, the current page alone converted to grey.
- **Everyday**: Markdown files show their pictures, web links of chosen sites open without asking,
  the settings can travel with the program (portable), and the word being read aloud is marked on
  the page.
- **Fixed**: the pictures of Markdown files were drawn mirrored down the page; list items ran into
  one paragraph; the last row of a table lost its wrapped words.

Everything earlier releases brought is described in their notes and in [CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.73.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.73.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.73.0-macos-universal.zip` (app bundle) or `folitecta-0.73.0-macos-universal.tar.gz` (binary) |

Each package carries the `dictionaries`, `ocr` and `fonts` folders next to the program (in the
app bundle's resources on macOS): keep them with it. SHA-256 checksums are attached.

- **Linux** needs glibc 2.35 or newer and the WebKitGTK 4.1 runtime (`sudo apt install
  libwebkit2gtk-4.1-0` on Debian and Ubuntu, `sudo dnf install webkit2gtk4.1` on Fedora,
  `sudo pacman -S webkit2gtk-4.1` on Arch). Reading aloud uses speech-dispatcher with a voice,
  scanning uses SANE's `scanimage`, and cards and tokens use p11-kit and the card's PKCS #11
  module; all are optional.
- **Windows**: the program is not code-signed, so Windows may warn before the first start.
- **macOS** builds are not notarized: the first time, right-click → Open (on macOS 15 and later,
  System Settings › Privacy & Security › Open Anyway).

The status of every command is in [docs/FEATURES.md][features]; known limitations are in
[docs/ARCHITECTURE.md][limits] and [ai/ROADMAP.md][roadmap].

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0730---2026-11-09
[features]: https://github.com/KvasFan/Folitecta/blob/v0.73.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
