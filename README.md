# Folitecta

**Folitecta** is a free PDF editor written entirely in Rust, with a modern desktop
interface built on [Dioxus](https://dioxuslabs.com). It renders pages with the pure-Rust
[hayro](https://github.com/LaurenzV/hayro) rasterizer and edits documents through the
[lopdf](https://github.com/J-F-Liu/lopdf) object model. Every dependency is free software under
an MIT, Apache or GNU licence.

![Folitecta screenshot](docs/screenshot.png)

## Status: version 0.0.1 (pre-release)

Folitecta ships the **complete command surface of a professional PDF editor** — 1,977 commands
across a ribbon with 11 tabs and 63 groups, 9 classic menus, 27 side panes and a command palette —
so the whole feature set is discoverable from day one. In this first release a subset of those
commands performs its operation; the rest are listed and open a roadmap notice. The exact
state of every command is in [docs/FEATURES.md](docs/FEATURES.md) (generated from the code) and in
the in-app **Feature matrix** dialog.

### Working today

- **Viewing**: tabs, continuous / single / two-page layouts, fit width / fit page / free zoom,
  view rotation, full screen, light and dark themes, thumbnails, go-to-page, keyboard navigation.
- **Pages**: insert blank pages, delete, duplicate, extract to a new file, move, swap, reverse,
  rotate, crop by margins, resize with content scaling, add margins, split a document, insert the
  pages of another PDF, combine several files into one, delete empty pages.
- **Page marks**: text watermarks (position, rotation, opacity, colour), headers and footers with
  placeholders (`{page}`, `{pages}`, `{date}`, `{title}`, `{file}`, `{author}`), page numbering and
  Bates numbering (`{bates:6}`), and removal of any mark Folitecta added.
- **Comments**: sticky notes, free text boxes, rectangles, ellipses, lines and arrows, freehand
  ink, area highlights, each with a real appearance stream; comments pane, edit text, hide/show,
  delete, summary export.
- **Navigation data**: bookmarks (create, rename, retarget, delete, one-per-page), named
  destinations, links (list, open, add web links and page links, remove), attachments (list,
  save, add, remove), layers (toggle visibility), form fields (list, edit values, reset,
  highlight, export values), signature fields (status and signer information).
- **Text**: extraction, search with match case / whole words, word count, export to `.txt`.
- **Export**: pages to PNG or JPEG at any resolution, embedded images, document properties
  editing, undo/redo of every edit, save / save as / save a copy, recent files, sessions of
  tabs, command palette (`Ctrl+Shift+P`), English and Italian interface.

### Planned

OCR, redaction, digital signing and certificate management, content editing (text and
image objects), measurement tools, stamps, form field design, PDF/A conversion, optimisation,
spell checking, accessibility tagging, cloud connectors and the remaining commands listed as
*planned* in the feature matrix.

## Download

Binaries for Windows (x64), Linux (x64) and macOS (Apple silicon and Intel) are attached to each
[GitHub release](https://github.com/KvasExpert/Folitecta/releases). On Linux the WebKitGTK 4.1
runtime must be installed (`libwebkit2gtk-4.1-0`, `libgtk-3-0`).

## Building from source

Requirements: Rust 1.92 or newer (stable) and the platform toolchain.

- **Windows**: Visual Studio Build Tools with the "Desktop development with C++" workload;
  the Edge WebView2 runtime (preinstalled on Windows 10/11).
- **Linux**: `libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev pkg-config`.
- **macOS**: Xcode command line tools.

```sh
cargo build --release
./target/release/folitecta my.pdf
```

Run the tests with `cargo test --workspace`, the lints with
`cargo clippy --workspace --all-targets -- -D warnings`, and regenerate the feature matrix with
`cargo run -p folitecta -- --features-md docs/FEATURES.md`.

## Project layout

| Crate | Purpose |
|---|---|
| `crates/folitecta-core` | Document model (lopdf), page operations, comments, bookmarks, forms, attachments, layers, links, text, page marks, hayro render service, command registry |
| `crates/folitecta` | Dioxus desktop application: ribbon, menus, panes, canvas with drawing tools, dialogs, palette, localization |

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design and
[docs/FEATURES.md](docs/FEATURES.md) for the command-by-command status.

## Licence

Folitecta is released under the [MIT License](LICENSE). Folitecta is an independent
implementation: it contains no code from, and is not affiliated with or endorsed by, any other
PDF product. Third-party crates keep their own MIT, Apache-2.0 or GNU licences; the allowed set is
enforced by `deny.toml` (`cargo deny check licenses`).
