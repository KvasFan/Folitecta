## Folitecta 0.0.10 — icon ribbon, faster pages, barcodes

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- The ribbon shows its tools as icons, with their names in the tooltips: more than twice as many
  tools fit in the same space. View › Tool Names (or the preferences) shows the names again.
- Pages appear sooner: each document draws its pages on up to three threads, and the picture of
  a page is encoded about four times faster. Rasterizing, redacting, cropping to the margins,
  finding duplicate pages and exporting pages as pictures work on the pixels directly.
- Barcodes: draw a QR Code, Data Matrix, PDF417, Aztec or linear barcode (Code 128, Code 39,
  Code 93, EAN, UPC, ITF, Codabar) on a page; read every barcode of the document or of an area,
  with its kind, content and page; put web links over the barcodes that hold an address.
- Create Web Links from URLs adds all its links in one undo step.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.10-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.10-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.10-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
