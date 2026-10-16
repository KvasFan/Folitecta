# Folitecta

**Folitecta** is a free PDF editor written entirely in Rust, with a modern desktop
interface built on [Dioxus](https://dioxuslabs.com). It renders pages with the pure-Rust
[hayro](https://github.com/LaurenzV/hayro) rasterizer and edits documents through the
[lopdf](https://github.com/J-F-Liu/lopdf) object model. Every dependency is free software under
an MIT, Apache or GNU licence.

![Folitecta screenshot](docs/screenshot.png)

## Status: version 0.0.3 (pre-release)

Folitecta ships the **complete command surface of a professional PDF editor** — 1,977 commands
across a ribbon with 11 tabs and 63 groups, 9 classic menus, 27 side panes and a command palette —
so the whole feature set is discoverable. Each release turns more of those commands into working
operations; the rest are listed and open a roadmap notice. The exact state of every command is in
[docs/FEATURES.md](docs/FEATURES.md) (generated from the code) and in the in-app
**Feature matrix** dialog.

### Working today

- **Viewing**: tabs, continuous / single / two-page layouts, fit width / fit page / free zoom
  (`Ctrl`+wheel), view rotation, full screen, light and dark themes, dark and high-contrast page
  modes, rulers (mm, cm, in, pt, pc), grid, guides with snapping, loupe, link and form-field
  highlighting, transparency grid, document info bar, page labels, view history.
- **Text**: text layer with mouse selection, copy, select all; search with match case / whole
  words, highlighted results and previous/next navigation (`F3`); mark every result as
  highlight, underline, strikeout or redaction; word count; export to `.txt`.
- **Comments**: sticky notes, text boxes, callouts, typewriter text, rectangles, ellipses,
  polygons, polylines, clouds, lines, arrows, pencil with eraser, area and free highlights, text
  highlight / underline / strikeout, insert-text and replace-text marks, stamps (presets, custom
  text, images), file attachments, web and page links, each with a generated appearance stream.
  Style editor (colour, fill, opacity, thickness, dashes, line endings, text size and alignment)
  with per-tool defaults and style copy/paste. Select, move and resize with handles, nudge with
  the arrow keys, copy / cut / paste across documents, flatten one or all, hide/show, delete,
  summary export.
- **Measure**: distance, perimeter and area with a calibrated scale, labels on the page, CSV
  export.
- **Forms**: draw text, check box, radio button, combo box, list box, button and signature
  fields; edit their properties; fill them on the page; import and export values (XFDF, FDF,
  CSV, text); reset; tab order; field highlighting.
- **Pages**: insert blank pages, pages of another PDF or pictures, delete (also empty pages),
  duplicate, extract, move by dragging thumbnails, swap, reverse, replace with pages of another
  file, rotate, crop by margins or to the visible content, resize, add margins, normalise sizes,
  split into tiles, rasterise, find duplicates, split a document, combine several files.
- **Page marks**: watermarks, backgrounds, headers and footers with placeholders (`{page}`,
  `{pages}`, `{date}`, `{title}`, `{file}`, `{author}`), page numbering and Bates numbering
  (`{bates:6}`, also across several files), page labels (roman, letters, prefixes), page
  transitions, overlays and underlays from another PDF, removal of any mark Folitecta added.
- **Protect**: redaction (mark areas or text, apply to destroy the content underneath), password
  protection with AES-128 and permissions, document script listing and removal.
- **Create and convert**: blank documents of any paper size, from pictures, from text files, from
  the clipboard; export pages to PNG/JPEG, extract embedded images; snapshot of an area to the
  clipboard; paste pictures onto pages and text as text boxes.
- **Print**: page range, resolution, with or without comments, fit to paper, every open document.
- **Navigation data**: bookmarks, named destinations, links, attachments, layers, form fields
  (values, reset, highlight, export), signature fields (status and signer).
- **Interface**: ribbon and classic menus that adapt to any window size, side panes, properties
  pane, command palette (`Ctrl+Shift+P`), undo/redo of every edit, English and Italian.

### Planned

OCR, digital signing and certificate management, content editing (text and image objects),
PDF/A conversion, optimisation, spell checking, accessibility tagging, document comparison,
cloud connectors and the remaining commands listed as *planned* in the feature matrix.

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

`folitecta --ui-selftest <dir>` runs the interface self-test: it resizes the window through
several sizes, opens every ribbon tab, panes, menus and dialogs in both languages, checks the
layout for clipped or overlapping elements, simulates mouse and keyboard input on the canvas and
writes a report with a screenshot of every state (screenshots on Windows).

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
