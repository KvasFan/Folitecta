# Architecture

Folitecta is a two-crate Cargo workspace.

```
crates/
  folitecta-core/   library: document model, operations, rendering, command registry
  folitecta/        binary: Dioxus desktop user interface
```

## folitecta-core

### Document model

`Document` wraps a `lopdf::Document` together with:

- the **serialized bytes** of the current state (`Arc<Vec<u8>>`), used by the renderer and as
  undo snapshots;
- an optional **file path**, a **revision counter** and a **dirty flag**;
- an **undo/redo history** of up to 40 snapshots. Every mutating operation calls `begin_edit()`
  (pushes a snapshot) and `commit()` (re-serializes, bumps the revision). Undo reloads the
  previous snapshot, so the model is always consistent with what the renderer shows.

Operations are grouped by module: `pages` (rotate, delete, insert, duplicate, move, swap,
reverse, extract, split, merge, crop, resize, margins), `annots` (comments with generated
appearance streams), `outline` (bookmarks), `forms`, `attachments`, `layers`, `links`
(including named destinations), `text` (extraction and search), `marks` (watermarks, headers
and footers, page numbering) and `export`.

Page-tree edits always go through `set_page_order`, which first copies inheritable attributes
(`Resources`, `MediaBox`, `CropBox`, `Rotate`) onto every page and then rewrites a flat `/Pages`
node, so nested page trees from any producer are handled uniformly.

Encrypted files that open with an empty user password are decrypted on load and written back as
plain PDFs; password-protected files prompt for the password.

### Rendering

`render::Renderer` rasterizes pages with hayro (CPU only, no GPU dependency) and returns PNG bytes.
`render::RenderService` runs one worker thread per open document that owns the parsed PDF, a hayro
render cache and a small memo of recent renders. The UI never blocks on rendering: the webview
requests `/page/{tab}/{page}/{revision}/{scale}.png` and the response is produced on the worker
thread. Because the revision is part of the URL the browser cache stays valid until the document
changes.

### Command registry

`assets/commands.json` lists every user-facing command (id, label, parent, classic menu, ribbon
tab and group, pane, kind, shortcut). `commands::Registry` loads it once and answers structural
questions: entries of a menu in display order, buttons of a ribbon group, commands of a pane,
label paths, fuzzy search and shortcut lookup. Unit tests guarantee that the data is closed
(every parent exists, every group and pane is known, every entry is placed somewhere).

## folitecta (UI)

- `state.rs` — `AppState` is a bundle of Dioxus signals (open tabs, active tab, preferences,
  current dialog, toasts, palette, tool). `DocTab` pairs a `Document` with its `RenderService`,
  view state (page, zoom, layout, rotation), selections and search state.
- `commands.rs` — `HANDLERS` maps registry ids to functions. It is the **single source of truth**
  for what is implemented: the feature matrix, the badges in menus and the roadmap notices all
  derive from it. `PARTIAL` marks reduced implementations.
- `files.rs` — open/save/export flows with native dialogs (rfd), unsaved-changes handling.
- `ui/` — `menubar` (classic menus generated from the registry), `ribbon` (tabs, groups, large
  buttons, overflow menus, contextual Format tab), `panes` (thumbnails, bookmarks, comments,
  attachments, layers, fields, links, destinations, search, signatures, planned panes), `canvas`
  (document tabs, page images, drawing tools, annotation hit boxes), `rightpane` (properties),
  `statusbar`, `dialogs`, `palette`, `toasts`, `welcome`.
- `i18n.rs` — embedded English and Italian string tables; Italian command labels override the
  registry labels where provided.
- `features.rs` — generates `docs/FEATURES.md` (`folitecta --features-md <path>`).

### Drawing tools

Mouse coordinates on a page image are converted to PDF user space with
`folitecta_core::marks::display_to_pdf`, which accounts for the page `/Rotate` value. Tools are
disabled while the *view* rotation is not zero (a toast explains why).

## Known limitations of 0.0.1

- Text selection and content editing are not implemented; search works on extracted text only.
- Comments on rotated pages are placed correctly but their hit boxes are shown only on unrotated
  pages.
- Appearance streams use Helvetica with WinAnsi encoding; characters outside Latin-1 are
  rendered as `?` in generated text (watermarks, headers, free text).
- Form field values are written to the document with `NeedAppearances`; viewers regenerate the
  appearance when they open the file.
- Saving writes a complete file (no incremental update), which invalidates existing digital
  signatures. Signatures are reported, not created.
