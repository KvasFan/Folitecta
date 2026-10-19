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
small authority for the tests, while the interface's `http` module carries the requests; `ocsp`
and `crl` read OCSP answers and revocation lists from the bytes as received and hold a small
responder and list issuer for the tests; `revocation` decides the state of each certificate of a
chain for the time a signature is proven or claimed to exist; `dss` reads and extends the
document security store and tells later revisions that only add validation material from the
others by their structure; the validation reports integrity, signer, time and revocation as
separate checks, and the interface runs it off its thread through a `ValidationJob`), `redaction` (a
probe draws a copy of the marked pages with every text operation, `Do` and inline picture
wrapped in numbered marked content and optional content neutralized, so it tells which operation
of which stream — page or form, through the chain of drawings — drew each glyph and picture;
glyphs under the marks leave their text operations, each replaced by a move of its width, paths
lose their geometry there (fills and clips cut to the four bands around each mark, strokes cut
in their own space where their ink — caps and joins included — would enter it), pictures and
inline pictures get their pixels covered, forms drawn under a mark are redacted in place when
drawn once and copied otherwise, Type 3 fonts keep the glyph procedures of the codes still shown
(with the resources those use), and a pattern of a paint removed whole that holds what was under
a mark may not stay for another paint of the page; drawings are counted through the document
once per set of resources (the masks of pictures and the procedures of shown Type 3 glyphs
with them), and what nothing draws any more leaves the file with every reference to it, while
what the marked pages list and nothing ever drew only leaves their resources; comments and fields over the marks go with
their replies, signatures go, covers are tagged per redaction, and the document is sanitized as
chosen; every edited page is checked before its covers are drawn and the exact bytes of a save
can be checked — read again, decrypted with the kept key — before they are written; pages that
cannot be edited that way are drawn as pictures), `compare`, `combine` (documents made from several files), `delivery` (copies prepared to be
sent, made from a snapshot of the document: chosen pages that keep nothing of the others — the
resources of the kept pages are cut to what their content names, and the bookmarks, links, named
destinations, form fields, tags, opening view and article threads of the others go and are counted —,
comments kept, flattened or removed, the protection of the document kept or the copy refused,
signatures kept only by the unchanged signed file or an incremental update of it), `pictures` (the pictures
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

Generated text chooses its font in `fonts` (`TextWriter`): text whose characters all have a
WinAnsi code is drawn with the standard Helvetica, as before; other text with a subset of GNU
FreeFont FreeSans embedded in the appearance that draws it, as a Type 0 font (Identity-H,
CIDFontType2 with `/FontFile2`, `/CIDToGIDMap` and `/W`) with a `/ToUnicode` map, its codes the
characters in the order they are first drawn. The subsetter keeps the glyph numbers, records and
metrics of the glyphs used; each appearance owns its subset (marked `/FolitectaFont` with the key
of its characters), and a drawing made again with the same characters uses it again. Text that
needs right-to-left order, joining or combining marks, and characters the font lacks, are refused
before an edit begins (`check_text`, `appearance::text_problem`); a failed edit is abandoned with
its undo and redo history as it was (`Document::edit_step`). The font is a data file, never built
into the program (see `fonts/README.md`).

Encrypted files are decrypted in memory (password-protected files prompt for the password); the
encryption of the file — its key, passwords and permissions — is kept aside (`security::Protection`)
and applied again whenever the document is saved, so a protected file stays protected and opens
with the same passwords. Documents made from a protected one (extracted pages, filled copies,
summaries, reports, duplicates) carry its protection, and content of a protected file is refused
by unprotected documents. New protection is AES-256 (revision 6) by default, AES-128 on request.
Save Unrestricted Copy writes a plain copy on purpose. Signed documents are saved as an
incremental update, which keeps their signatures valid; for a protected signed document the
update is encrypted with the document's key (`protection::encrypted::protected_update`, over the
decrypted objects of the file as it was written). The value of a signature (its `/Contents`) is
never encrypted — the byte range leaves exactly that string out, as widely used readers and
writers expect — and is read back from the bytes the byte range leaves out, which must be the
`/Contents` string of that signature object. Every file is written to a temporary file
in the same folder and renamed over the target only when it is complete
(`document::write_pdf_file`, `util::write_atomic`), so a failed save never damages the original.

A later revision of a signed file counts as "validation material only" (the signature keeps
its integrity) by an allow-list, never by what the revision says of itself
(`digsig::dss::classify`): it must have the regular shape of a careful writer's update —
objects separated only by white space and comments, then one cross-reference section (a table
or one stream, never hybrid) whose subsections do not overlap or repeat numbers, whose in-use
entries point inside the update at headers of the same number and generation, which frees no
number in use, declares a `/Size` above its numbers, follows the previous section (`/Prev`) and
is the one `startxref` names, and whose object streams hold only new objects in index order
without `/Extends`. It may not define or free any object the document reaches (the catalog may
change only its `/DSS`, `/Version` and `/Extensions /ESIC`, the document information only its
modification date and producer), every such object must be unchanged, and the renderer's own
reader (hayro-syntax) must resolve each of them to the same values as the PDF library, before
and after. Anything else is reported as a change after signing.

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
  interface thread; protected documents are not copied, signed ones are copied
  as an incremental update.
- `mail.rs` — the files of an e-mail message written into a new private folder (one per message,
  open to the user only) and handed to the system's mail program (Simple MAPI on Windows, Mail on
  macOS, `xdg-email` on Linux), or saved with the message as an unsent draft (`.eml`); a mail
  program that is missing or cannot take the files is reported, nothing else is opened in its
  place. The e-mail dialog (`commands/document/mail.rs`, its model `mail_draft.rs`, drawn by
  `ui/features/mail.rs`) prepares every file away from the interface thread, two at a time, and
  shows its size, facts and first pages. Tests and the self-test use a stand-in mail program.
- `dictionaries/`, `ocr/` and `fonts/` (at the root of the repository) — the Hunspell
  dictionaries, the text recognition models and the font of generated text beyond WinAnsi
  (FreeSans) shipped with the program, as data files next to it in the release packages (the
  `Resources` of the macOS bundle; `fonts/source`, the source of the font, stays in the source
  archives). They are looked for in the user's `Folitecta/dictionaries` (or `Folitecta/ocr`,
  `Folitecta/fonts`) folder first, then next to the program, then in the source tree, and the
  dictionaries on Linux in `/usr/share/hunspell` too.
- `features.rs` — generates `docs/FEATURES.md` (`folitecta --features-md <path>`).
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
- Text boxes, typewriter text, callouts and measurement labels draw text beyond WinAnsi with the
  embedded FreeSans subset; right-to-left text, letters that join or combine (Arabic, Hebrew,
  Indic scripts, Thai marks) and characters FreeSans lacks (Chinese, Japanese, Korean, ...) are
  refused, with no choice of another font yet; `/DA` still names Helvetica. Text stamps refuse
  text beyond WinAnsi. The other generated text (designed stamps, tables, calculator tapes, form
  fields, watermarks, headers and footers, page numbers, Bates numbers, comment summaries,
  documents made from text or e-mail, signature appearances, typed signature marks, media
  captions, barcode captions, redaction overlay text) still uses Helvetica with WinAnsi encoding,
  where characters the encoding lacks appear as `?`.
- Text recognition reads the Latin script, written left to right (lines turned up to about 45
  degrees); the invisible text uses WinAnsi, so letters it lacks (such as Polish `ł`) are written
  without their accent.
- Revocation is checked from OCSP answers and from complete revocation lists of the
  certificate's own issuer; indirect, delta and partitioned lists are not read, missing issuers
  are not fetched and certificate policies are not checked. Revocation data are asked online
  only when the user turns that on. Validation material is added to the document security
  store, but no long-term validation profile is claimed. Later revisions after a signature are
  recognised when they only add validation material or end with document time stamps; other
  changes are reported, not analysed. There is no conversion to PDF/A.
- Redaction applies rectangles only (no quadrilateral marks) and does not remove content
  outside the crop box. The moves that replace removed glyphs keep their widths, so the length
  of a removed word can be measured; shapes of removed glyphs stay in embedded font programs
  that other text still uses, and the procedures of a Type 3 font stay whole where the codes
  another page or form shows with it cannot be told (a font set by a graphics state included).
  Patterns and forms that other pages still draw stay for them; Type 3 fonts and patterns that
  keep what was under the marks for other pages are reported (after the redaction and in the
  apply dialog), not removed. Something counts as shown elsewhere only in a filled rectangle
  under rectangular clips. Comments, form values and page text elsewhere that quote the removed
  words are reported, not changed; in documents of more than 100 pages only the redacted pages
  and the first others are searched for them (the check says how many were not). Gradients
  (radial ones with stitched functions, and any whose colours under a mark are not shown in
  such a rectangle) and transparency masks that could keep content under a mark, tiling
  patterns whose cell would keep content under a mark that no whole cell shows, vertical
  writing and drawings nested more than 12 deep make the page a picture. Strokes lose a little
  ink outside the marks where their ends are slanted or a corner is split, and cut pieces
  restart their dash pattern. A signed version brought back by Undo after a later save cannot
  be saved with its signatures unless its signed file is on the disk again.
- Commands marked *planned* in [FEATURES.md](FEATURES.md) open a roadmap notice.
