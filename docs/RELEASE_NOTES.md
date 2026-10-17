## Folitecta 0.0.7 — explorer, picture editor and split pages

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- Explorer pane: the folders and files of the computer with places of your own, folder filters,
  thumbnails of PDF files, new folders, files moved to the recycle bin and lists of files.
- Combine files: a list of source files (PDF files, pictures, text files) added one by one, from
  a folder or from a list, reordered, with the pages to take from each PDF file.
- Picture editor: select a picture on the page with Edit Content › Images, then turn, mirror,
  crop, straighten, filter or convert it (gray, black and white, 256 colours) in place, in the
  editor or in another app; Apply to All changes it everywhere it is drawn.
- Split pages editor: cut lines drawn, dragged or typed over a page, a grid of equal parts,
  rulers and units, on every page, odd or even pages, or the page shown.
- Under the hood: smaller modules, a comparison that keeps memory linear, the tags tree read once
  per change for the panes.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.7-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.7-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.7-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
