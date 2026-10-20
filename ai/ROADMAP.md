# Folitecta roadmap and progress

State of every requirement of [REQUIREMENTS.md](REQUIREMENTS.md) on the `main` branch at
Folitecta 0.2.1, reviewed on **20 October 2026** against the code (not only the command
registry). Update the row of a requirement whenever a sub-task changes it, and log the work in
[PROGRESS.md](PROGRESS.md).

| Mark | Meaning |
|---|---|
| ✅ | Available for the profile described in the row, with tests; keep it working. |
| 🟡 | Partial: a concrete part works, the gaps are listed. |
| 🔵 | In progress on `main` (some parts committed, not finished). |
| ⬜ | Not started. |

Command-level detail (1981 registry commands: 1445 available, 59 partial, 477 planned) is in
[docs/FEATURES.md](../docs/FEATURES.md), generated with
`cargo run -p folitecta -- --features-md docs/FEATURES.md`.

## Now: work in progress

| Item | Requirement | State |
|---|---|---|
| Checks on real devices and services: printing on real printers (Linux, macOS, Windows), e-mail through real mail programs, time stamp and OCSP/CRL servers on the internet | FOL-L046, FOL-L053, FOL-C006, FOL-L040 | ⬜ to be done by hand; the tests and the self-test use stand-ins (a test printer, stand-in `xdg-email` programs, local servers on 127.0.0.1) |
| Speed on HiDPI screens and with pinch zoom (measured so far at pixel ratio 1 in a nested compositor); decoded JPEG pictures kept between renders | FOL-L001 | ⬜ after 0.2.1, which made scrolling and zoom smooth on long documents |

## Next releases

Big releases get a version tag and the full CI; work in between goes to `main` untagged.

### 0.2.0 — integrity and delivery (released 18 October 2026)

Released with steps 1, 2, 3, 5 and 6; steps 4 and 7 were released in **0.2.1**.

1. ~~**FOL-L051/S01 — atomic saving.**~~ Done: every PDF save writes a temporary file in the
   same folder, checks it is a whole PDF, flushes it and renames it over the target.
2. ~~**FOL-L036/S01 — keep the protection.**~~ Done: the encryption of the opened file (key,
   passwords, permissions) is applied again by Save, Save As and Save Copy; Save Unrestricted
   Copy is the explicit way to remove it. Next: **FOL-L036/S02** — AES-256 for new protection,
   protection of signed and time-stamped output, certificate encryption.
3. ~~**FOL-L037/S01 — selective redaction.**~~ Done for text and pictures: glyphs under the marks
   leave the content streams (each replaced by a move of its width), pixels of pictures under the
   marks are covered, comments and fields over the marks go with their values, the file is
   rewritten whole. Next: **FOL-L037/S02** — vector paths under the marks (today covered, not
   removed), text and pictures of reused drawings (today the page becomes a picture), and
   sanitizing metadata, attachments, hidden layers and bookmarks that quote the removed text.
4. ~~**FOL-L053** — finish the e-mail work (attachment dialog; the EML draft is done).~~ Done in
   0.2.1.
5. ~~**FOL-L006/S01** — PDF from EML.~~ Done.
6. ~~**FOL-C006/S01, FOL-L040/S01** — time stamps; integrity, identity, time and revocation as
   separate checks in the signature details.~~ Done (revocation reported as not checked).
7. ~~**FOL-L018/S01 — Unicode in generated text.**~~ Done in 0.2.1 for text boxes, typewriter
   text, callouts and measurement labels (a subset of GNU FreeFont FreeSans with a ToUnicode map,
   never `?`). Next: stamps, tables and tapes (S02), page marks (S03), form fields (S04).

### 0.2.1 — protection, printing and speed (released 20 October 2026)

As planned: FOL-L018/S01 (Unicode in generated text), FOL-L053 attachment dialog, FOL-L036/S02
(AES-256; protected documents signed and time-stamped encrypted), FOL-L040/S02 with FOL-C006/S02
(revocation from OCSP and revocation lists, validation material in the document security store),
FOL-L037/S02 (vector paths and reused drawings under redaction marks, sanitizing). Also:
FOL-L046/S01 (print window with sheet preview, pages per sheet, booklet, poster, direct
printing), FOL-L023/S02 (drawing and writing on the selected layer), the speed of scrolling and
zoom (FOL-L001), and the interface self-test on Linux.

### 0.3.0 — daily work

FOL-L014 (OCR review), FOL-L012 (scan clean-up workflow), FOL-L005 (CSV, Markdown, RTF), FOL-L035
(synchronized side-by-side comparison), FOL-L018/S02–S03 (Unicode in stamps, tables, tapes and
page marks), FOL-L042 (local text to
speech with the system voices, reading reflow), FOL-L022 (table of contents from bookmarks or
headings, article threads), FOL-L028 (angle, radius, volume), FOL-L029 (tick marks), FOL-L002
(tab groups and windows), FOL-L010 (template gallery), FOL-L011 (scanner through the system
imaging API).

### 0.4.0 — professional tools

FOL-L045 (recompress one picture, presets), FOL-L043 (PDF/A validation and conversion), FOL-L044
(colour conversion, output preview), FOL-L021 (vector path editing), FOL-L047 (action sequences,
batch, command line), FOL-L034 (sandboxed JavaScript engine, console), FOL-L039 (system
certificate store, PKCS#11), FOL-L009 (office exports).

### Later

FOL-L033 (XFA engine), FOL-L048 (on-device AI), FOL-L049 (3D), FOL-L006 (office and HTML import),
FOL-L007 (virtual printer, shell integration), FOL-L008 (XPS, metafiles, CAD), remote services
FOL-C001–C005 and C007–C014 (R4).

## State of every requirement

### Reading, workspace and search

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L001 | P1 | 🟡 | hayro rendering on worker threads with one queue per document ranked by the window, pages near the view only, small previews, tiles at high zoom, pages redrawn only when they change, scrolling and Ctrl+wheel without waiting for the program; thumbnails, single/continuous/two-page layouts, right-to-left option, loupe, pan and zoom, split window, view rotation, full screen, password-protected files (decrypted in memory) | Certificate-encrypted files; presentation mode with transitions; a test corpus (transparency, scans, rotated pages); HiDPI and pinch zoom measurements. |
| FOL-L002 | P1 | 🟡 | document tabs, split window, sessions (save/open), recovery of unsaved documents at the next start | Tab groups and several windows; report missing files of a session one by one. |
| FOL-L003 | P1 | 🟡 | search pane, Find and Replace, search in folders, annotate results | Patterns, filters on comments/bookmarks/attachments, embedded or local indexes, export of results. |
| FOL-L004 | P1 | 🟡 | panes for thumbnails, bookmarks, comments, attachments, layers, fields, links, destinations, signatures, tags, reading order, content; properties pane; document properties and XMP | Fonts pane, object inspector, history pane of changes. |

### Creation, import, export and templates

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L005 | P1 | 🟡 | blank pages, from images, from text files, from the clipboard, combine files | CSV (local separators), Markdown with CSS and formulas, RTF with styles. |
| FOL-L006 | P2 | 🟡 | e-mail messages (EML) made into documents: header lines, text, attachments embedded | Mail archives; office formats and HTML need a local engine decision. |
| FOL-L007 | P2 | ⬜ | — | File associations (`help.fileAssoc`), virtual printer, office add-ins. |
| FOL-L008 | P2 | ⬜ | — | XPS/OXPS, metafiles, CAD: engine decision first. |
| FOL-L009 | P1/P2 | 🟡 | export to pictures (PNG, JPEG, TIFF, …), text, embedded pictures, attachments, split and extracted documents | Accessible text export; office formats (P2). |
| FOL-L010 | P1 | ⬜ | stamps have categories and favourites | Template gallery for documents. |

### Scanning and OCR

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L011 | P1 | ⬜ | — | Scanner acquisition through the system imaging API (Windows first), presets, interleave fronts/backs. |
| FOL-L012 | P1 | 🟡 | picture tools: turn, crop, straighten, filters, black and white; delete blank pages | One clean-up workflow for scanned pages with preview of the candidates and thresholds. |
| FOL-L013 | P1 | 🟡 | local OCR (PaddleOCR models run by tract): pages, a selected area, pictures; invisible text tagged `OCR`, removed or replaced on demand; Latin-script languages | Editable text output; exclusions (existing text, fields, comments, logos); batches of files; more scripts (models). |
| FOL-L014 | P1 | ⬜ | — | Review of suspect words (crop + text + accept/correct); field recognition proposals. |

### Pages and composition

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L015 | P1 | 🟡 | insert, delete, extract, duplicate, move, swap, reverse, replace, insert pictures, split documents, split/merge pages, combine files, find duplicate pages, delete empty pages | Check links, fields, tags and destinations after every operation (FOL-L052); rename same-named fields when merging; list unresolved references before committing. |
| FOL-L016 | P1/P2 | 🟡 | rotate, crop, crop to white margins, resize, margins, normalize, overlay, rasterize pages | Box management (bleed/trim/art) in the interface; selective flattening; deleting content outside the crop box (today it is only clipped). |
| FOL-L017 | P1 | ✅ | watermarks, header and footer, Bates numbering (several files), background, page numbering, macros, Folitecta's marks tagged so they can be updated or removed | Logical page labels in every page selector. |

### Native editing

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L018 | P0 | 🟡 | text boxes, typewriter text, callouts and measurement labels: an embedded subset of GNU FreeFont FreeSans with ToUnicode for text beyond WinAnsi, refusals named before the change and while typing; text stamps refuse what they cannot draw; other generated text uses the standard Helvetica with WinAnsi | Right-to-left text and shaping (refused today); a choice of substitute font; the `/DA` font; font inspection and replacement; the other producers: S02 designed stamps, tables, tapes; S03 page marks, Bates, summaries, text and e-mail documents; S04 form fields; S05 OCR, barcodes, signatures, media, redaction text. |
| FOL-L019 | P1 | 🟡 | Edit Text tool (content stream editing), insert/replace/delete page text, find and replace in page text, spell checking with corrections | Paragraph reflow inside an area, joining/splitting blocks, scope for shared form objects. |
| FOL-L020 | P1/P2 | 🟡 | pictures pane and editor: replace, crop, turn, filters, export | Inline and nested pictures, colour profile preservation report, raster editor mode. |
| FOL-L021 | P2 | ⬜ | — | Vector path editing (nodes, clipping, gradients). |

### Navigation, containers and embedded content

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L022 | P1 | 🟡 | bookmarks (hierarchy, editing, export), links, named destinations, web links from text and barcodes | Table of contents generation, bookmarks from headings, article threads, duplicate cleanup. |
| FOL-L023 | P1/P2 | ✅ | attachments, layers (visibility, properties, drawing and writing on the selected layer, comments and pictures moved between layers, comments of hidden layers hidden everywhere, visibility expressions), audio/video with playback | Codec list and unsupported media report; layers given by membership dictionaries lost on export and paste; XFDF carries no layer; pictures inside reused drawings on layers. |
| FOL-L024 | P2 | 🟡 | PDF portfolios: create, view, extract, print | Binder project with sources and regeneration. |

### Comments and technical tools

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L025 | P1 | ✅ | every markup and drawing comment, styles and named styles, lock, grouping, align/distribute/order, eraser, clouds | Pressure-sensitive pen; check appearances in a second reader. |
| FOL-L026 | P1 | ✅ | comments pane with filters, replies, review states, XFDF/FDF/PDF exchange, comment summaries | CSV summary; history of a comment. |
| FOL-L027 | P1/P2 | 🟡 | text stamps, stamp designer with dynamic values, favourites and recent stamps | Sequential stamps; symbol libraries with fields asked when placed. |
| FOL-L028 | P2 | 🟡 | distance, perimeter, area, calibration, CSV export of measures | Angle, radius/diameter, volume; snapping profile; measure info pane. |
| FOL-L029 | P2 | 🟡 | calculator with an editable tape placed as a comment | Tick marks with sequences and styles. |
| FOL-L030 | P1/P2 | ✅ | barcodes written (vector) and read (rxing), tables as generated objects | Table extraction from PDF pages (P2). |

### Forms and scripts

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L031 | P1 | 🟡 | form designer for every field type, items of choice fields, pictures of push buttons, tab order, align/duplicate, fill & sign | A real date field (format, picker, validation); same-name policy when duplicating/merging. |
| FOL-L032 | P1 | 🟡 | XFDF/FDF/CSV/TXT import and export, filling copies from table rows, calculation order, simple calculations computed in Rust | Named XML profiles, number/date formats and validation, page templates. |
| FOL-L033 | P2 | ⬜ | — | XFA engine decision. |
| FOL-L034 | P2 | 🟡 | document scripts and actions listed and edited | A sandboxed JavaScript engine with a documented API, console, time-outs. |

### Comparison, encryption, redaction and signatures

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L035 | P1 | 🟡 | text comparison of two documents with a report | Synchronized side-by-side view, geometry/picture differences. |
| FOL-L036 | P0 | 🟡 | passwords and permissions with AES-256 (default) or AES-128, security policies; files opened encrypted are saved with the same encryption, passwords and permissions; protected documents signed, certified and time-stamped encrypted (encrypted updates keep the signatures); derived documents keep the protection, imports from protected files refused; print permissions applied; Save Unrestricted Copy removes the protection on purpose | Certificate (public-key) encryption; changing or removing the protection needs only the password to open (kept by choice); permissions other than printing not enforced in Folitecta; strings in stream dictionaries not encrypted. |
| FOL-L037 | P0 | 🟡 | mark areas and search results for redaction; apply by content (glyphs, picture pixels, vector paths, inline pictures, Type 3 text and the content of reused drawings under the marks removed, the rest stays text, comments and fields over the marks removed with their values, overlay codes upright on rotated pages, whole-file rewrite so earlier revisions go) or as pictures (chosen, or for pages that cannot be edited); sanitizing chosen when applying (metadata, attachments, hidden content, quotes of the removed words, scripts); the new file checked before it is written | Quadrilateral marks; content outside the crop box; quotes in page text, comments and field values reported, not changed; the width of a removed word can be measured; patterns and drawings other pages draw stay for them (reported). |
| FOL-L038 | P1 | ✅ | handwritten signatures and initials, placed on several pages, kept apart from digital signatures | — |
| FOL-L039 | P1/P2 | 🟡 | PKCS#12 digital IDs (import, create, export), sign and certify (visible or invisible), appearance with picture, incremental updates keep earlier signatures | System certificate store, PKCS#11 tokens, batches. |
| FOL-L040 | P1 | 🟡 | validation of every signature (digest, value, chain, trusted certificates), trusted list, signed revision opened in a tab; separate checks of integrity, signer, time and revocation; time stamps of signatures and document time stamps validated (imprint, nonce, authority signature and usage, trust); revocation from OCSP answers and revocation lists at hand or fetched (opt-in); validation material written to the document security store; revisions that only add it recognised when they are regular updates read the same by a second reader | Indirect, delta and reason-partitioned lists, points named relative to the issuer, AIA, policy checks; other changes of later revisions are not analysed (they count as changes); not tried against public servers. |

### Accessibility, standards, optimization and printing

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L041 | P1 | 🟡 | accessibility check and report, auto-tagging of the document and of form fields, tags pane, reading order, alternate text | Table repair (headers, spans); manual checks listed in the report. |
| FOL-L042 | P1 | 🟡 | English and Italian interface, keyboard shortcuts, themes (light, white, dark, black), short tool names with full tooltips | Reading reflow, local text to speech, a keyboard-only and screen reader pass. |
| FOL-L043 | P2 | ⬜ | enable editing of PDF/A files | Preflight profiles, PDF/A validation and conversion. |
| FOL-L044 | P2 | ⬜ | — | Colour conversion, output preview. |
| FOL-L045 | P2 | 🟡 | space usage audit, recompression of the pictures of pages | One selected picture, presets, size comparison report. |
| FOL-L046 | P1/P2 | 🟡 | a print window of its own with a sheet preview and the printers of the computer: ranges by number or label, odd/even, reverse, copies and collation, comments and fields, fit/actual/shrink/custom scale, paper by page, 2–16 pages per sheet, booklet, poster, two-sided order, colour; jobs sent directly (IPP with a vector print file on Linux and macOS, GDI pictures up to 600 dpi on Windows); Save as PDF; print permissions; portfolio printing of its own files | Verification on real printers (order, scale, margins, counts); vector output on Windows; booklets on both sides without a duplex printer. |

### Automation, AI and specialist functions

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L047 | P2 | ⬜ | page-mark macros only; a minimal command line (`--features-md`, `--version`) | Action sequences, batches over files, command line with structured results. |
| FOL-L048 | P2 | ⬜ | — | On-device AI engine decision. |
| FOL-L049 | P2 | ⬜ | — | 3D viewer and authoring. |
| FOL-L050 | P1/P2 | 🟡 | preferences, shortcuts, ribbon and classic interface, Quick Access Toolbar, command palette, settings export/import/reset, resizable panes and ribbon | Toolbox profiles, plug-ins, portable/administrative settings. |

### Integrity contracts and sharing

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L051 | P0 | 🟡 | undo/redo of every edit; every PDF save (Save, Save As, copies, protected copies, signed files) and the settings written to a temporary file and renamed over the target only when complete; recovery copies the same way, never for encrypted documents; signed documents saved as incremental updates | Versioned session format; exports of pictures/text/attachments still written directly (they never replace the document). |
| FOL-L052 | P0 | 🟡 | page tree normalized before every page edit, links and destinations kept by page operations, scripts never run (there is no engine), external actions not executed on opening | A relations test corpus (links, fields, tags, destinations) for reorder/copy/merge; a report of the relations that cannot be kept. |
| FOL-L053 | P1 | 🟡 | real attachments through the mail program (Simple MAPI, Mail, `xdg-email`), ZIP, EML drafts; the attachment dialog: current state or saved file, pages, comments kept/flattened/removed, form data and comments as files, sizes and previews, protected copies kept protected, failures reported in the dialog | A check with real mail programs; Extract Pages, Split and Delete Pages onto the same page copies (layers and XFA follow page subsets). |

### Remote services

| Id | P | State | Notes |
|---|---|---|---|
| FOL-C001 | P3 | ⬜ | Remote storage connectors. |
| FOL-C002 | P3 | ⬜ | Document management systems. |
| FOL-C003 | P3 | ⬜ | Shared review service. |
| FOL-C004 | P3 | ⬜ | Sharing by link, project spaces. |
| FOL-C005 | P3 | ⬜ | E-signature providers. |
| FOL-C006 | P1 | 🟡 | Time stamps from an RFC 3161 server set by the user (signatures and document time stamps, only a digest sent); OCSP answers and revocation lists fetched when turned on; public servers not tried yet, no long-term validation profile claimed. |
| FOL-C007 | P3 | ⬜ | Enterprise rights management. |
| FOL-C008 | P3 | ⬜ | Third-party DRM. |
| FOL-C009 | P3 | ⬜ | Direct mail delivery (preparation is FOL-L053). |
| FOL-C010 | P3 | ⬜ | Form submission endpoints. |
| FOL-C011 | P3 | ⬜ | Remote AI endpoints. |
| FOL-C012 | P3 | ⬜ | Remote OCR/conversion. |
| FOL-C013 | P3 | ⬜ | URL content, template catalogues, remote tools. |
| FOL-C014 | P3 | ⬜ | Central administration. |

## Interface requests to keep

- Tools show a **short name under the icon** (`Pencil`, `Loupe`, `Zoom +`, `Zoom −`), on by
  default; View › Tool Names hides them; the tooltip has the whole name and the shortcut. Short
  names live in `assets/i18n/<lang>/short.json`; a tool without one gets a shortened label.
- The edges between ribbon groups and of the panes must follow the pointer when dragged.
- Submenus must not run away: an open submenu waits while the pointer crosses other rows towards
  it.
- Tools are icons from Tabler Icons; every command keeps a tooltip and an accessible name.
