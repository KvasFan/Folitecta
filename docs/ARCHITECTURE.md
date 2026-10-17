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
- an **undo/redo history** of snapshots. Every mutating operation calls `begin_edit()` (pushes a
  snapshot) and `commit()` (re-serializes, bumps the revision). Undo reloads the previous
  snapshot, so the model is always consistent with what the renderer shows;
- caches keyed by revision: the positioned text of the pages, the measured marked content, the
  structure tree shown by the panes and the pictures of the pages.

Operations are grouped by module, for example `pages` and `pagetools` (page tree edits, crop,
resize, split), `annots` and `appearance` (comments with generated appearance streams), `outline`
(bookmarks), `forms` and `form_design`, `attachments`, `layers`, `links`, `text` and `textlayer`
(extraction, selection and search), `marks` (watermarks, headers and footers, page numbering),
`structure` and `accessibility` (tags, reading order, the accessibility check), `digsig`
(signatures), `compare`, `combine` (documents made from several files), `pictures` (the pictures
a page draws: where they are, their pixels, an edited picture put back) and `imaging` (turning,
cropping, straightening, filters and pixel formats). `geom` holds the rectangle helpers shared by
the modules; matrices and paths use kurbo, the geometry library of the renderer.

Page-tree edits always go through `set_page_order`, which first copies inheritable attributes
(`Resources`, `MediaBox`, `CropBox`, `Rotate`) onto every page and then rewrites a flat `/Pages`
node, so nested page trees from any producer are handled uniformly.

Content streams are rewritten operation by operation with a small lexer (`content_lex`) that keeps
the bytes of every operation it does not change.

Encrypted files that open with an empty user password are decrypted on load and written back as
plain PDFs; password-protected files prompt for the password. Signed documents are saved as an
incremental update, which keeps their signatures valid.

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

- `state/` — `AppState` is a bundle of Dioxus signals (open tabs, active tab, preferences,
  current dialog, toasts, palette, tool). `DocTab` pairs a `Document` with its `RenderService`,
  view state (page, zoom, layout, rotation), selections and search state.
- `commands/` — one module per area, each with a `HANDLERS` table that maps registry ids to
  functions. Together they are the **single source of truth** for what is implemented: the
  feature matrix, the badges in menus and the roadmap notices all derive from them. `PARTIAL`
  marks reduced implementations.
- `files.rs` — open/save/export flows with native dialogs (rfd), unsaved-changes handling.
- `ui/` — `menubar` (classic menus generated from the registry), `ribbon` (tabs, groups, large
  buttons, overflow menus, contextual tabs), `panes` (thumbnails, bookmarks, comments,
  attachments, layers, fields, search, explorer and more), `canvas` (document tabs, page images,
  drawing tools, comment and picture hit boxes), `dialogs`, `rightpane` (properties),
  `statusbar`, `palette`, `toasts`, `welcome`.
- `i18n.rs` — embedded English and Italian string tables; Italian command labels override the
  registry labels where provided.
- `features.rs` — generates `docs/FEATURES.md` (`folitecta --features-md <path>`).
- `selftest/` — the interface self-test (`folitecta --ui-selftest <dir>`): layout audits at
  several window sizes and simulated input on the canvas, with a report and screenshots.

### Drawing tools

Mouse coordinates on a page image are converted to PDF user space with
`folitecta_core::marks::display_to_pdf`, which accounts for the page `/Rotate` value.

## Known limitations

- The text and shapes of the page content cannot be edited. Pictures can, when the page content
  draws them itself (pictures inside forms and inline pictures are not found); edited pictures are
  written as 8-bit RGB, gray, palette or black and white pictures (CMYK, ICC profiles and 16-bit
  samples are not kept, and stencil masks become black and white pictures).
- Generated text (comments, stamps, page marks) uses Helvetica with WinAnsi encoding: characters
  the WinAnsi encoding lacks appear as `?`.
- There is no OCR, no timestamp on signatures and no conversion to PDF/A.
- Commands marked *planned* in [FEATURES.md](FEATURES.md) open a roadmap notice.
