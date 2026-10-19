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

The modules are grouped in folders by area — `organize` (pages and documents), `markup`
(comments), `graphics` (pictures and barcodes), `page_text`, `interactive` (forms and scripts),
`navigation`, `protection` and `tags` — while the document model, rendering, the registry and the
shared helpers (`util`, `geom`, `xml`) stay at the root; the root re-exports every module, so
paths such as `folitecta_core::pages` stay short. The larger modules are folders of their own:
`appearance` (the style of comments, the shapes and line endings, the note icons, measurement
labels and the drawing of each kind of comment) and `comment_exchange` (XFDF, FDF); the drawing
of form fields is `form_appearance`, next to `form_design`.

Operations are grouped by module, for example `pages` and `pagetools` (page tree edits, crop,
resize, split), `annots` and `appearance` (comments with generated appearance streams), `outline`
(bookmarks), `forms`, `form_design` and `form_calc` (simple field calculations, written as
`AFSimple_Calculate` actions and computed in Rust in the `/CO` order), `attachments`, `layers`, `links`, `text` and `textlayer`
(extraction, selection and search), `ocr` (text recognition: a PaddleOCR detection model finds
the lines as connected areas of its probability map, each fitted with a turned rectangle from the
moments of its pixels; a recognition model reads each line cut out straight at 48 pixels high,
and the steps of its CTC output where each character is read place the words; both models run
with tract, the plans cached per input size and the lines read in parallel; the text is written
as invisible Helvetica text, one stretched run per word, in a content stream tagged `OCR`),
`spelling` (the words of the page text, comments and text
fields that Hunspell dictionaries do not know, checked with spellbook against every dictionary
loaded; suggestions ranked by typing distance and by the language of the text around the word;
corrections through the page text editor, the comment text or the field value, several in one
undo step; the dictionaries are files read at run time, not built in), `marks` (watermarks,
headers and footers, page numbering),
`structure` and `accessibility` (tags, reading order, the accessibility check), `digsig`
(signatures; `timestamp` builds RFC 3161 requests, checks the answers and tokens and holds a
small authority for the tests, while the interface's `http` module carries the requests; the
validation reports integrity, signer, time and revocation as separate checks), `redaction` (the glyphs under the marks leave their text operations, each
replaced by a move of its width found by the text probe, the pixels of pictures under the marks
are covered, comments and fields over them go; pages that cannot be edited that way are drawn as
pictures), `compare`, `combine` (documents made from several files), `pictures` (the pictures
a page draws: where they are, their pixels, an edited picture put back), `imaging` (turning,
cropping, straightening, filters and pixel formats) and `barcodes` (barcodes written by rxing and
drawn as vector shapes; read from rendered pages one at a time, each painted out before the next
read, with bullseye and dark-area searches for the codes rxing finds only near the middle of a
picture; pages read side by side with rayon). `geom` holds the rectangle helpers shared by
the modules; matrices and paths use kurbo, the geometry library of the renderer.

Page-tree edits always go through `set_page_order`, which first copies inheritable attributes
(`Resources`, `MediaBox`, `CropBox`, `Rotate`) onto every page and then rewrites a flat `/Pages`
node, so nested page trees from any producer are handled uniformly.

Content streams are rewritten operation by operation with a small lexer (`content_lex`) that keeps
the bytes of every operation it does not change.

Encrypted files are decrypted in memory (password-protected files prompt for the password); the
encryption of the file — its key, passwords and permissions — is kept aside and applied again
whenever the document is saved, so a protected file stays protected and opens with the same
passwords. Save Unrestricted Copy writes a plain copy on purpose. Signed documents are saved as an
incremental update, which keeps their signatures valid. Every file is written to a temporary file
in the same folder and renamed over the target only when it is complete
(`document::write_pdf_file`, `util::write_atomic`), so a failed save never damages the original.

### Rendering

`render::Renderer` rasterizes pages with hayro (CPU only, no GPU dependency) and returns PNG bytes,
encoded with the fast DEFLATE of fdeflate, or the pixels themselves (`render_rgba`) for the
operations that work on them. `render::RenderService` runs up to three worker threads per open
document; each owns its own parse of the PDF, a hayro render cache and a small memo of recent
renders, and a page always goes to the same thread (page number modulo the threads), so visible
pages are drawn side by side and keep their cache. The UI never blocks on rendering: the webview
requests `/page/{tab}/{page}/{revision}/{scale}.png` and the response is produced on a worker
thread. Because the revision is part of the URL the browser cache stays valid until the document
changes.

### Printing

`print` plans the sheets of a print job as pure geometry (one page per sheet, several pages per
sheet, booklets, posters, the order of two-sided output) and writes them as a vector PDF: each page
used becomes form XObjects (its content, then its printable annotations) placed on sheets of the
paper's size, with the objects copied once and the layers fixed to their print state. The file is
checked before it is sent or saved, and encrypted like its source as the last step when it is
saved. `raster` draws sheets as pictures, and the renderer draws pages in bands for printers that
receive pixels.

### Command registry

`assets/commands.json` lists every user-facing command (id, label, parent, classic menu, ribbon
tab and group, pane, kind, shortcut). `commands::Registry` loads it once and answers structural
questions: entries of a menu in display order, buttons of a ribbon group, commands of a pane,
label paths, fuzzy search and shortcut lookup. Unit tests guarantee that the data is closed
(every parent exists, every group and pane is known, every entry is placed somewhere).

## folitecta (UI)

- `state/` — `AppState` is a bundle of Dioxus signals (open tabs, active tab, preferences,
  current dialog, toasts, palette, tool). `DocTab` pairs a `Document` with its `RenderService`,
  view state (page, zoom, layout, rotation), selections and search state. `comment_styles`
  keeps the default and named styles of the comment tools; `prefs` the saved preferences,
  including the `Theme`.
- `commands/` — one module per area, each with a `HANDLERS` table that maps registry ids to
  functions, grouped by what they work on: `document` (files, pages, pictures, barcodes,
  combining, comparing), `markup` (comments, review, stamps, tables, calculator, media), `text`
  (search, Find and Replace, spell checking, page text), `fields` (forms and scripts), `navigation` (bookmarks,
  layers, attachments, panes, portfolios, explorer, history, sessions), `protection`
  (redaction, passwords, signatures), `window` (Edit, View and Help menus, split window,
  interface) and `accessibility`. Together they are the **single source of truth** for what is implemented: the
  feature matrix, the badges in menus and the roadmap notices all derive from them. `PARTIAL`
  marks reduced implementations.
- `files/` — the flows that go through native file dialogs (rfd), one module per area:
  `documents` (open, new, save, close, revert, protected copies), `export` (pictures of pages,
  extracted and split documents, text), `images` (picture files placed, attached or made into
  pages), `forms` (form data), `comments` (XFDF and FDF exchange), `print`, `pages` (pages from
  another PDF, Bates numbering) and `settings`.
- `ui/` — the interface, in four parts:
  - `chrome/` — the frame of the window: `menubar` (classic menus generated from the registry),
    `ribbon` and `ribbon_layout` (tabs, groups, the fitting of the buttons to the width, overflow
    menus, contextual tabs; the tools are icons with a short name under each, hidden by the
    View › Tool Names option, and the whole name in the tooltips), `toolbars`, `statusbar`, `palette`, `toasts`, `welcome`, and
    `layout`, the sizes the user drags (pane widths, rows of the ribbon, group widths), saved as
    preferences;
  - `panes/` — the left panes (thumbnails, bookmarks, comments, attachments, layers, fields,
    search, explorer, history, comment styles, accessibility) and `right` (properties);
  - `canvas/` — document tabs, page images, drawing tools, comment and picture hit boxes. A
    split window shows two views of a tab: the focused one keeps its state in the tab (`page`,
    `zoom`, `layout`, `view_rotation`) and the ids `canvas` and `pg-N` the commands look for;
    the other one (`SplitView`) shows the pictures of its pages, and the two change places when
    it is clicked;
  - `dialogs/` and `features/` — dialogs, and the windows and editors of the features
    (pictures, stamps, signatures, forms, tables, media, calculator, scripts, …).
- `theme.rs` and `assets/styles/` — `theme.css` holds the colour tokens of every theme (light,
  white, dark, black); one style sheet per part of the window and per feature uses only those
  tokens. `theme::CSS` puts them together in cascade order.
- `icons/` — `set` gives each icon name its drawing (Tabler Icons, MIT, from `icondata_tb`),
  `commands` picks the icon of every command (a table for the main tools, word rules for the
  rest), `Icon` draws one.
- `i18n.rs` and `assets/i18n/<lang>/<area>.json` — English and Italian strings, one file per
  area (commands, interface, dialogs, messages, comments, pages, forms, protect, accessibility,
  search); a key lives in the file of the area of its first word. Italian command labels override
  the registry labels where provided.
- `recovery.rs` — copies of the documents with unsaved changes (every minute and after saving or
  closing, in the local data folder `Folitecta/recovery`, with a note of where each came from),
  removed when a document is saved or closed and on Exit. Each window is a session (process id
  and start time) holding a lock file while it runs; copies of sessions whose lock is free are
  offered. Copies are written through a temporary file, one checkpoint at a time, off the
  interface thread; documents opened from encrypted files are not copied, signed ones are copied
  as an incremental update.
- `dictionaries/` and `ocr/` (at the root of the repository) — the Hunspell dictionaries and the
  text recognition models shipped with the program, as data files next to it in the release
  packages (the `Resources` of the macOS bundle). They are looked for in the user's
  `Folitecta/dictionaries` (or `Folitecta/ocr`) folder first, then next to the program, then in
  the source tree, and the dictionaries on Linux in `/usr/share/hunspell` too.
- `features.rs` — generates `docs/FEATURES.md` (`folitecta --features-md <path>`).
- `printing/` — the printers: IPP to the local print server on Linux and macOS (the vector print
  file is sent), GDI on Windows (the sheets drawn in bands), and a fake printer for the tests and
  the self-test; the print window is `ui/features/print.rs`, its jobs `commands/document/print.rs`.
- `selftest/` — the interface self-test (`folitecta --ui-selftest <dir>`): layout audits at
  several window sizes and simulated input on the canvas, with a report and screenshots.

### Drawing tools

Mouse coordinates on a page image are converted to PDF user space with
`folitecta_core::marks::display_to_pdf`, which accounts for the page `/Rotate` value.

## Known limitations

- The text of the page content can be changed when the page draws it itself: text inside reused
  drawings (form XObjects) and fonts whose codes cannot be matched to their glyphs are refused.
  The rest of the line moves to make room, but text does not flow to the next line. Characters
  missing from an embedded font come from a standard font of the same family (WinAnsi only).
  Find and Replace uses the same edits, so the same occurrences are left (and counted).
- Populate Forms reads one table row per line: values with line breaks inside quotes are not
  supported.
- Spell checking reads UTF-8 and Latin-1 (ISO8859-1, ISO8859-15, Windows-1252) dictionaries;
  dictionaries in other legacy encodings must be converted to UTF-8 first. A misspelled word in
  the page text is corrected with the page text editor, so the same limits apply.
- The shapes of the page content cannot be edited. Pictures can, when the page content
  draws them itself (pictures inside forms and inline pictures are not found); edited pictures are
  written as 8-bit RGB, gray, palette or black and white pictures (CMYK, ICC profiles and 16-bit
  samples are not kept, and stencil masks become black and white pictures).
- Generated text (comments, stamps, page marks) uses Helvetica with WinAnsi encoding: characters
  the WinAnsi encoding lacks appear as `?`.
- Text recognition reads the Latin script, written left to right (lines turned up to about 45
  degrees); the invisible text uses WinAnsi, so letters it lacks (such as Polish `ł`) are written
  without their accent.
- Revocation of certificates (revocation lists, OCSP) is not checked and no long-term validation
  material is added; later revisions after a signature are reported, but what they change is
  not analysed (only whether they end with document time stamps). There is no conversion to
  PDF/A.
- Commands marked *planned* in [FEATURES.md](FEATURES.md) open a roadmap notice.
