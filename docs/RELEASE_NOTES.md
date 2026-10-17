## Folitecta 0.0.4 — signatures, layers, portfolios and a deeper search

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- Digital signatures: sign and certify documents with PKCS#12 digital IDs (RSA or ECDSA),
  create your own ID, validate signatures against trusted certificates. Signed documents are
  saved as incremental updates, so existing signatures stay valid.
- Handwritten signatures and initials: draw, type or use a picture, keep them in a library and
  place them on any page.
- Review: replies, review states with history, filters, comment summaries, and comment exchange
  as XFDF, FDF or PDF.
- Layers: show or hide layers in the view, make the view the default, create, merge, flatten,
  extract and import layers.
- Portfolios: put files and folders in a PDF portfolio, browse them with columns, sorting and a
  preview, and give each file its own information.
- Search: ignore accents, find words near each other, look in comments, bookmarks, fields,
  attachments and links, or through every PDF of a folder.
- Bookmarks, named destinations, attachments and fields panes with full editing; macros for
  headers, footers, watermarks and Bates numbers.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.4-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.4-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.4-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
