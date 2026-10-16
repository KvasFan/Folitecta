## Folitecta 0.0.1 — first pre-release

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- Full command surface: 1,977 commands in a ribbon (11 tabs, 63 groups), 9 classic menus,
  27 side panes and a command palette. Commands not yet implemented show a roadmap notice;
  `docs/FEATURES.md` lists the status of each one.
- Viewing: tabs, three layouts, fit modes, zoom, rotation, full screen, light/dark themes,
  thumbnails, keyboard navigation.
- Page operations: insert, delete, duplicate, extract, move, swap, reverse, rotate, crop,
  resize, margins, split, merge, combine.
- Page marks: watermarks, headers/footers with placeholders, page and Bates numbering.
- Comments with appearance streams: notes, text boxes, shapes, lines, arrows, ink, highlights.
- Bookmarks, named destinations, links, attachments, layers, form fields, signature status.
- Text extraction, search, word count; export to PNG/JPEG/TXT; embedded image extraction.
- Undo/redo, document properties, recent files, English and Italian interface.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.1-x86_64-pc-windows-msvc.zip` |
| Linux x64 | `folitecta-0.0.1-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple silicon | `folitecta-0.0.1-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `folitecta-0.0.1-x86_64-apple-darwin.tar.gz` |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

**Known limitations** are listed in `docs/ARCHITECTURE.md`.
