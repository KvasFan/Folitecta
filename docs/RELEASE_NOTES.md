## Folitecta 0.0.5 — accessibility, comparison, stamps and tables

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- Accessibility: tag a document automatically from what its pages draw (headings, paragraphs,
  figures, links and other annotations, artifacts for drawings and running headers), then
  refine the tags in the Tags pane, the reading order in the Order pane and the marked content in
  the Content pane. A 32-rule accessibility check lists what to fix, fixes what it can by itself,
  asks for the language and the alternate text of figures, and writes a report.
- Compare documents: the text of two documents compared word by word, with the changes
  highlighted on both, a list of the differences and a comparison report.
- Stamps: a Stamps pane with collections, favourites and recent stamps, and a stamp designer
  with templates, shapes, pictures and fields filled in when the stamp is placed.
- Tables: draw a table on the page and edit it cell by cell: rows, columns, merged cells,
  colours and alignment.
- History and sessions: documents opened before with thumbnails and pins; save the open
  documents as a session and open it again later.
- Document tools: pages of typed or RTF text, a space usage audit, picture recompression,
  document scripts, pages split at the guides or cut to their crop box, leaving PDF/A.
- Form editor: copies of a field in rows and columns, tab and calculation order numbers on the
  fields.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.5-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.5-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.5-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
