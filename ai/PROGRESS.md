# Progress log

Finished work, newest first. One entry per sub-task: requirement, commit, what changed, how it was
checked, what is still open. The history before October 2026 is in
[CHANGELOG.md](../CHANGELOG.md). Never rewrite an old entry: when a task is reopened, add a new one.

## 2026-10-26 — FOL-L009, FOL-L025: the office export from its dialog; shape tools stay chosen

- **Current state before:** Export to Office Formats… closed its dialog and then waited for the
  save window in a task of that dialog, which ended with it: nothing was written (the self-test
  called the writing directly and did not see it). Shape tools went back to the hand after one
  shape.
- **Changed:** `commands/document/office.rs` runs the save window and the writing with
  `spawn_forever`, and takes the self-test's answer to the save window (`answer_save`). The
  other dialogs of 0.3.0 and 0.4.0 close only after their work, without waiting after it.
  `ui/canvas/create.rs`: `stays_chosen` — the pencil, the free highlighter, the tick marks and
  the rectangle, ellipse, line, arrow, polygon, polyline and cloud tools.
- **Checked:** the office step now goes through the dialog (Word chosen, Export pressed); with
  the old code it found no Word file, with the fix the file holds the headings and text. The
  tools step draws two rectangles one after the other without choosing the tool again.
- **Open:** a test of every dialog through its own buttons.

## 2026-10-26 — FOL-L039/S02: digital IDs on cards, tokens and the system store

- **Current state before:** only PKCS #12 digital IDs kept by Folitecta could sign.
- **Changed:** core `digsig/external.rs`: `ExternalKey` (signs a digest: RSA PKCS #1 v1.5, or
  ECDSA as DER, `ecdsa_der` for devices that give `r ‖ s`), `ExternalKind` from the certificate,
  `rsa_digest_info`, `DigitalId::from_external`, `SoftKey` for tests and the stand-in card; `IdKey`
  gets an external variant (no PKCS #12 export, no issuing), and a signature made outside is
  checked against the certificate before it is used. App `keystore/`: `pkcs11.rs` with `cryptoki`
  (Apache-2.0, approved; modules loaded once, tokens listed without logging in, the key found
  by the certificate's id after `C_Login`), `windows.rs` (personal store, CNG) and `macos.rs`
  (keychain identities, Security framework, approved), a stand-in card for the self-test. Digital
  IDs lists the IDs outside and the added modules (add, remove, look again); the signing dialog
  offers them with a PIN field, or says the system asks.
- **Checked:** unit tests of `external` (RSA and P-256 keys outside sign and are checked; no
  export; a device signing with another key is caught; `DigestInfo` and `r ‖ s`) and of
  `keystore::pkcs11` (a file that is no module refused; the system's trust module loaded and
  listed twice); Windows and macOS builds linted from Linux; self-test step `cards` (the card
  listed, its PIN field, a wrong PIN refused in the dialog, the document signed by the card and
  valid).
- **Open:** tried with real cards and stores; the issuers' certificates of a token carried in
  the signature; batches with confirmed scope.

## 2026-10-26 — FOL-L034/S02: a sandboxed JavaScript engine and the scripts of fields

- **Current state before:** the console ran code in a Web Worker of the interface; the scripts
  of fields were kept in the file but only `AFSimple_Calculate` was understood.
- **Changed:** core `interactive/js.rs` with Boa (`boa_engine`, MIT/Unlicense, approved): `run_script`
  in a fresh context (loop limit 5 000 000, recursion limit 400), a prelude with the document,
  `event`, `app`, `console`, `util` and the `AF…` functions (simple calculations, number, percent
  and special formats and keystrokes, ranges), the outcome as `ScriptRun` (console lines, result,
  operations, event value and `rc`); `API` lists what scripts can use. `form_calc`:
  `field_script`, `run_field_script`, `set_field_script` (one undo step, `/CO` kept), custom
  calculate scripts run by `recalculate`; `set_field_value` asks the validate script; the
  appearance of a text field shows what its format script gives. App: `FieldScripts` in the field
  properties; the console runs `run_script` on a blocking thread and gives up after its time
  limit.
- **Checked:** unit tests of `js` (operations, `AF…` functions, errors and endless scripts);
  `tests/js.rs` (4: a calculate script of its own kept in the file, removed and undone; a format
  script changes only the text shown; a validate script refuses a value with its alert and with
  `AFRange_Validate`; endless loops and recursion stopped, nothing of the system in reach); the
  form, template and script tests unchanged; self-test step `field_scripts` (three scripts typed
  in the properties, the field calculated, formatted and a value refused) and the console step.
- **Open:** keystroke scripts while typing, page and document events, action chains, a database
  bridge.

## 2026-10-26 — FOL-L021/S01: editing the vector paths of a page

- **Current state before:** pictures and text of the page could be edited; vector paths could not.
- **Changed:** core `graphics/paths.rs`: `page_paths` (decoded content walked with `q`/`Q`, `cm`,
  `w` and device colours; every painted path with its nodes — each point of `m`, `l`, `c`, `v`,
  `y`, the origin and far corner of `re` — bounds, stroke and fill, colours, width and the
  inverse matrix; clipping paths and text left out), `path_at`, `move_path_node`,
  `set_path_style` (the path wrapped in `q` … `Q` with `RG`/`rg`/`w`), `delete_path`, all
  written back as one content stream in one undo step. App: `Tool::EditPaths` on Edit Content ›
  Shapes; the paths' boxes on the page, the chosen one's nodes (dragged over a layer that follows
  the pointer and drops the node on release) and `PathToolbar` (colour inputs, width, delete);
  the chosen path in the page stamps so the page draws again when it changes.
- **Checked:** `tests/paths.rs` (4: paths, nodes and colours found, the topmost under a point; a
  node moved and the far corner of a rectangle resized, kept after opening the file again, drawn,
  undone; colours, width and deletion; the lines of a template are paths found by a click); unit
  test of the graphics state; self-test step `paths` (a rectangle chosen by a press, its two
  nodes and bar shown, a node dragged 40 points, filled green, deleted, all undone).
- **Open:** reused drawings, adding and removing nodes, clipping, gradients, transparency.

## 2026-10-26 — FOL-L009/S02: export to Word, RTF and HTML

- **Current state before:** export to pictures, text, embedded images and attachments.
- **Changed:** core `organize/office.rs`: `OfficeFormat` (DOCX, RTF, HTML), `export_office`
  (the paragraphs and headings of `reading_blocks`; DOCX as a package of `[Content_Types].xml`,
  relationships, `word/document.xml` with Heading1–3 styles, `word/styles.xml` and
  `docProps/core.xml`; RTF with `\uN?` escapes; HTML with `h1`–`h3` and `p`; page breaks on
  request; pictures and comments counted as left out). `PageText::paragraphs` ends a paragraph
  where the line size changes by more than a fifth, so a small heading above its text is found
  as a heading (it was joined to the text). Export to Office Formats… with a dialog (format,
  pages, page breaks) writing where the user chooses, never over the document.
- **Checked:** `tests/office.rs` (2: a DOCX whose parts are well-formed XML with the headings
  styled, in order, and a page break, the report of a picture and a comment left out; RTF with
  Unicode escaped and no page breaks when not asked; HTML of the first page only; pages out of
  range and a page without text refused); unit tests of the RTF escapes; all core tests after the
  change of paragraphs; self-test step `office`.
- **Open:** pictures, tables to XLSX with their columns and types, PPTX, page fidelity.

## 2026-10-26 — FOL-L047/S01: action sequences, batches, command line

- **Current state before:** macros of page marks; `--features-md` and `--version`.
- **Changed:** core `organize/actions.rs`: `ActionSequence` and `Step` (11 kinds, JSON with an
  `action` tag and defaults), `run_actions` (one undo step; all undone when a step fails, with
  the log so far), `batch_inputs` (folders with their subfolders), `plan_batch` (outputs in a
  folder or next to the inputs with a suffix; `exists`, `same-file`, `twice`), `run_batch_item`
  (open, run, `save_as`; protected files fail with their reason, collisions skip unless
  overwriting); `make_pdfa` converts in place (one undo step) for the PDF/A step. App: Tools ›
  Action Sequences… (sequences in the settings, steps edited with their settings, import and
  export, run on the document, on files or a folder with the plan, results, Cancel) and
  `cli.rs` (`--run-actions`, JSON lines, exit status 0/1/2; on Windows the console of the
  parent is attached for the commands of the command line).
- **Checked:** `tests/actions.rs` (3: a sequence as one undo step; a failing step leaves the
  document as it was; a batch over a folder with a subfolder, a protected file and an existing
  output gives done, failed, skipped and leaves the inputs untouched; overwrite, same-file and
  twice); unit tests of the JSON; the command line run by hand on files; self-test step
  `actions`.
- **Open:** interactive steps, more steps, portfolios as inputs, scheduling.

## 2026-10-26 — FOL-L044/S01: colours to grey, grey preview

- **Current state before:** pictures could be made grey one by one in the picture editor.
- **Changed:** core `organize/greyscale.rs`: `convert_to_grey` and `estimate_grey` (on a copy):
  content streams decoded and re-encoded with the colour state followed through `q`/`Q`, `cs`/`CS`
  and the colour spaces of the resources (`rg`, `RG`, `k`, `K` and `sc`/`scn` in RGB, CMYK and
  ICC-based spaces become `g`/`G`, their spaces DeviceGray); pictures (8 bits, Flate or JPEG) and
  palettes made grey; `/C`, `/IC`, `/MK` colours and `/DA` of comments and fields;
  `GreyReport` with what was converted and what was left. Commands Convert Colours to Grey…
  (a dialog with the estimate) and View › Grey Preview (a `grayscale(1)` filter on the page
  pictures).
- **Checked:** `tests/greyscale.rs` (2: an invoice with pictures and a red note drawn with grey
  pixels only after the conversion, also opened again, coloured again after Undo, nothing left
  to convert a second time; a spot colour left and counted while an RGB fill becomes grey);
  unit tests of the grey of colours and of a content stream; self-test step `grey`.
- **Open:** ICC profiles and rendering intents, CMYK, spot colours, output preview of
  separations and overprint.

## 2026-10-26 — FOL-L043/S01: PDF/A-2b preflight and conversion

- **Current state before:** only Enable Editing (Discard PDF/A).
- **Changed:** core `organize/pdfa.rs`: `RULES` (18 rules with their clauses of ISO 19005-2,
  level B), `preflight_pdfa` (findings with page, object, detail and whether conversion fixes
  them), `convert_to_pdfa` (a copy: file identifier, XMP from the document information with
  dates as XMP dates, an sRGB output intent from `srgb_icc`, actions and JavaScript removed,
  annotation flags, rollover and down appearances, LZW streams re-encoded, XFA and
  NeedAppearances removed, Interpolate false, layer configurations named; the copy is checked
  again and declares `pdfaid:part 2`, `conformance B` only when nothing is left). Commands
  Preflight PDF/A… and Convert to PDF/A… with a dialog listing the rules, the problems and the
  conversion into a new tab.
- **Checked:** `tests/pdfa.rs` (3: a document in the embedded font with JavaScript, a hidden note,
  device colours and an interpolated picture becomes PDF/A-2b and still passes when opened
  again, and draws; a document with the standard Helvetica is converted but never declared
  PDF/A; a protected document refused); unit tests of PDF dates and of the sRGB profile, which
  LittleCMS also read and used (colours identical to its own sRGB); self-test step `preflight`.
- **Open:** embedding fonts, the other PDF/A rules, other profiles, an independent validation.
## 2026-10-26 — FOL-L045/S01: recompression of one picture, presets, estimate

- **Current state before:** the pictures of all pages or of the current page as JPEG at a quality
  and a largest side in pixels; one picture could not be chosen; no estimate.
- **Changed:** core `organize/recompress.rs`: `RecompressScope` (all, pages, one picture of
  `page_images`), `RecompressSettings` (quality, highest dpi, largest side), `RecompressPreset`
  (screen 96 dpi q60, e-book 150 q72, print 300 q85, archive q92), `estimate_recompress` and
  `recompress` sharing one plan; the resolution of each picture where it is drawn largest from
  the sides of its matrix; `RecompressItem` per picture (page, dpi, pixels and bytes before and
  after, `Kept` reason); `recompress_images` kept as a wrapper. The dialog: scope (the selected
  picture chosen by Recompress Image…), preset or own settings, Estimate on a copy away from the
  interface, a table of the pictures. Recompress Image… is no longer partial.
- **Checked:** `tests/recompress.rs` (4: an estimate changes nothing and scales to 96 dpi, archive
  keeps the pixels; one picture changes without the other, undone, a wrong index refused; pages
  recompressed open again with JPEG pictures of the new size and draw; transparent pictures and
  larger results left alone, the file unchanged); `tests/doctools.rs` unchanged; self-test step
  `recompress`.
- **Open:** fonts and other objects, MRC for scans, quality measured on the reopened file.

## 2026-10-26 — FOL-L025, FOL-L050: a smaller pencil pointer on Linux, shorter menus (0.3.1)

- **Asked after the 0.3.0 release:** the pencil symbol is too large on Linux; the menus (File and
  Edit first) are too long and want submenus.
- **Changed:** the pencil pointer was an SVG of 24 CSS pixels; WebKitGTK up to 2.54 draws an SVG
  pointer at the scale of a HiDPI screen without setting the scale of the cursor surface
  (WebKit bug 328226, fixed upstream on 21 October 2026), so on the user's screens (scale 1.5
  and 2, both 2 for GTK3) it came out twice as large. On Linux (`assets/styles/linux.css`,
  `theme::PLATFORM_CSS`) it is now a 20-pixel PNG, which keeps its size with every WebKitGTK.
  `ui/chrome/menu_layout.rs`: the layout of the menus of the menu bar and of long submenus of
  the registry (`Item::Cmd`, `Sep`, `Group`, `More`), resolved against the registry
  (`menu_nodes`, `submenu_nodes`); `ui::menu_nodes` draws them (submenus of the layout open like
  the registry's), also in the ribbon dropdowns and the pane menus. Top rows: File 21 (was 81),
  Edit 22 (111), View 19 (121), Document 21 (107), Comments 17 (65), Tools 10, Window 10.
- **Checked:** unit tests (every name of a layout is a command; every menu at most 22 rows, each
  submenu of a layout at most 20, More at most 16; every command a menu held is still in it,
  none twice; separators only between entries; every submenu labelled in English and Italian);
  self-test steps `layout` (File has 21 rows, its New submenu holds From Template…; the submenu
  delay) and `text` (the pencil pointer is a PNG on Linux).
- **Open:** the pencil's quick panel (0.5.0).

## 2026-10-26 — FOL-L011/S01: scanning

- **Current state before:** From Scanner… and Custom Scan… were planned.
- **Changed:** core `organize/scanning.rs`: `ScannedPicture`, `picture_dpi` (PNG `pHYs`, JPEG
  JFIF density, BMP), `duplex_order`, `Document::from_scans` and `append_scans` (pages the size
  of the paper through `insert_sized_image_pages`), `interleave_backs`. App `scanner/`: SANE's
  `scanimage` (devices, options of `-A`, flatbed to its output, feeder with `--batch`), WIA
  (`SelectDeviceDlgID` + `GetImageDlg` on an STA thread), a stand-in for the self-test; colour
  and grey kept as JPEG. `commands/document/scanning.rs` and the scan dialog: devices and
  capabilities asked in the background, presets (`scan.presets`), backs as a second stack
  (`scan.turn_over`, No Backs), Clean Up Scans afterwards. A Windows-only warning of
  FOL-L042 (`EngineKind::SpeechDispatcher` unused there) silenced.
- **Checked:** `tests/scanning.rs` (3: resolutions read from PNG, JPEG and BMP; page sizes from
  the resolution given, written or 300; backs interleaved both ways, undone); unit tests of the
  order and of the `scanimage` output; self-test step `scanning` (stand-in device listed, preset
  saved, three fronts and three backs made into six Letter pages, one flatbed page); clippy
  with `-D warnings` for the Windows (clang-cl, llvm-lib) and macOS targets. Not tried with a
  real scanner: none here, and SANE is not installed.
- **Open:** macOS (Image Capture), scanning into the open document, stopping a scan.

## 2026-10-26 — FOL-L002/S01: tab groups and windows

- **Current state before:** one strip of tabs, a split window of one document; New Blank Window
  made a blank document in the same window.
- **Changed:** `commands/window/tab_groups.rs`: `TabGroups` (the tabs of each group, the tab each
  shows, the group worked in) in `AppState::groups`; `sync` (called by `set_active` and when a
  tab closes) keeps groups and views in line: the active tab holds the other group's document as
  the other view of its split (`SplitView::source`), views going back to their tabs when the
  active tab changes, new tabs joining the group worked in, a group left empty ending the
  groups. `focus_view` of the other view makes its group's document the active one. Commands:
  New Vertical/Horizontal Tab Group, Move Tab to the Other Group, Distribute Tabs side by side or
  one above the other, Merge All Tabs into One Group, Move to New Window (a saved document's file
  opened by another process, its tab closed; unsaved documents are to be saved first), New Blank
  Window (another process). A split, Remove Split and a comparison side by side end the groups.
  `start_window` only notes windows during the self-test.
- **Checked:** self-test step `groups` (strips 3 and 1, focus by a press and by a tab, ended by
  moving the last tab back, stacked and gathered, the moved document noted with its file and its
  tab closed) and `split_view` unchanged.
- **Open:** dragging tabs between groups and windows; gathering the tabs of other windows.

## 2026-10-26 — FOL-L010/S01: template gallery

- **Current state before:** new documents were blank pages or made from files.
- **Changed:** `organize/templates.rs`: `Template` (13, in three groups), `TemplateOptions`
  (paper, pages or months, first month and year, first day of the week, words by key),
  `Document::from_template`: papers drawn as vector lines and dots; a week; months with the
  days in their columns; a to-do list, a letter, meeting notes and an invoice as forms whose
  fields and rows fit the paper; the invoice's amounts (`PRD`), subtotal and total (`SUM`)
  calculated. `place_form_field` adds a field without a commit (`add_form_field` = place +
  commit), so a form of hundreds of fields is one serialization. File › From Template…: the
  gallery with the picture of each template's first page (drawn in the background), the paper,
  sideways, pages or months, the first month and year, the first day of the week; double click
  creates. Words of the templates in `pages.json` (`templates.word.*`).
- **Checked:** `tests/templates.rs` (6: every template on Letter paper with fields only where
  meant; three months from November 2026 with 1 November under Sunday, also with weeks from
  Sunday; Italian and Cyrillic words; the invoice saved, opened, filled and summed; the to-do
  list and meeting fields on the page; A6 and a low wide paper keep every field on the page);
  unit tests of the calendar; self-test step `templates`.
- **Open:** the user's own templates (save as template, a folder shown in the gallery).
## 2026-10-22 — FOL-L042/S01: reflow and reading aloud

- **Current state before:** the pages could only be read as pages; nothing was read aloud.
- **Changed:** `page_text/reading.rs` (`reading_blocks`: the paragraphs of the pages in order with
  their characters, headings by size like `find_headings`, running heads and page numbers left
  out, words broken at a line end joined, compounds kept). The reflow view (a view option,
  `Ctrl+4`): the blocks page by page in a readable column, text sizes, the page at the top
  reported as the current page and a page gone to scrolled to. Read Aloud (`Ctrl+Shift+Y`): a bar
  and `commands/text/read_aloud.rs` reading paragraph by paragraph (the text selected, else from
  the current page), the paragraph marked on its page (`DocTab::reading`, kept in view) or in the
  reflow view, pause, previous, next, stop, voice and speed; `speech/`: the Web Speech API when
  the web view has voices, else speech-dispatcher over its socket (`speech/ssip.rs`, written
  from the protocol as the server implements it: CR LF, events at any time demultiplexed, the
  dummy module alone meaning no voice, `--spawn` once when the socket is missing; pause by
  cutting the paragraph), else the stand-in of the self-test. WebKitGTK here has no
  `speechSynthesis` and the machine has no speech-dispatcher, so the Linux path was tested
  against a fake service only.
- **Checked:** `tests/reading.rs` (headings, running foot left out, whole paragraphs in order with
  their characters, some pages); `reading.rs` unit tests (lines joined, broken words mended,
  compounds kept); `speech/ssip.rs` tests (blocks, messages and dot escaping, rates, voices and the
  dummy module, a whole session against a fake service: client name, notifications, voices,
  rate, language before voice, the other client's event ignored, the end told once, QUIT); self-
  test step `reading` (reflow: four pages, headings, a larger text, the current page followed
  both ways; reading with the stand-in: paragraphs in order, marked in the reflow view and on
  the page, a pause that holds, next, stop).
- **Open:** words marked as they are spoken; reading from a paragraph clicked; trying the Linux
  path with a real speech-dispatcher; a keyboard and screen reader pass.

## 2026-10-21 — FOL-L029/S01: tick marks

- **Current state before:** marks had to be drawn as shapes or stamps one by one, numbers typed.
- **Changed:** `markup/ticks.rs`: `TickMark` (shape, text, size, colour), `TickShape` (check,
  cross, dot, text, circled), `Sequence` (numbers, upper and lower letters like spreadsheet
  columns, roman numerals), `add_tick_mark` (a `/FolitectaTick` stamp keeping its mark as JSON,
  its rectangle centred on the click, wider for long texts) and `tick_mark_at`; the appearance
  draws it again from its mark when resized. Tool Tick Mark… (Comment › Drawing): a dialog for
  the mark, then each click places one and the sequence goes on; the tool stays chosen, the
  next mark is shown in the status bar.
- **Checked:** `tests/ticks.rs` (3: five kinds placed and listed, the mark kept, found after
  flattening; redrawn when resized; text beyond WinAnsi, undrawable and empty texts refused
  with nothing changed); unit tests of the sequences and sizes; marks rendered and looked at;
  self-test step `tools` (circled numbers 3 and 4 by two clicks, next 5) and the dialog state.
- **Open:** saved sets of tick styles; counting the marks of a document.

## 2026-10-21 — FOL-L028/S01: angles, radii and volumes

- **Current state before:** distances, perimeters and areas only.
- **Changed:** `add_angle_measure` (a polyline of three points, `/IT /FolitectaAngle`),
  `add_radius_measure` (a line from the centre, `/FolitectaRadius`; the comment's rectangle holds
  the circle), `add_volume_measure` (a polygon, `/FolitectaVolume`, `/FolitectaDepth` in the
  unit of the scale; refused without depth), sharing the measure dictionary of the others
  (`measure_annotation`). The appearance draws the arc of an angle and the circle of a radius
  (`measure_extra`) and their labels: degrees on the bisector (farther out for narrow angles),
  `R … · Ø …`, cubic units. `measurements()` lists them for the CSV export. Tools in the Measure
  group: Angle (finished by its third click, with its own status hint), Radius (a drag with the
  circle previewed), Volume (an area, then the depth asked, the last one offered again).
- **Checked:** `tests/markup_tools.rs` (a right and a 45 degree angle, a radius with its
  diameter and its circle inside the rectangle, a volume with comma decimals, the kinds listed,
  refusals); measurements rendered and looked at; self-test step `tools` (90.0° by three clicks,
  `R 12.7 mm · Ø 25.4 mm` by a drag, a volume with its depth asked).
- **Open:** snapping profile, a measure info pane, editing the depth afterwards.

## 2026-10-21 — FOL-L022/S01: bookmarks from headings, tables of contents

- **Current state before:** bookmarks were made by hand (or from the tags of a tagged
  document); no table of contents could be made.
- **Changed:** `navigation/contents.rs`: `find_headings` (each line with the size most of its
  letters have; the body size is the one most letters have; headings are at least 15% and a
  point larger, short, with letters, not on three pages and a quarter of them when their digits
  are left out; consecutive lines of one heading joined; levels by size, at most 4),
  `bookmarks_from_headings` (nested, opening the page at the heading, replacing or appending),
  `insert_contents` (set with `typeset`: title, entries indented by level, dotted leaders, the
  numbers the pages have once the contents are in, found in up to three passes; pages inserted,
  their fonts and a GoTo link per entry in one undo step). `Typeset` gains `put`, `cursor`,
  `link` and `into_pages`. Organize › Pages: Insert Table of Contents… and Bookmarks from
  Headings… (the headings listed by level, each can be left out or shown).
- **Checked:** `tests/contents.rs` (4: headings and levels with a heading on two lines and a
  running head; nested bookmarks, appended, undone; contents with the new page numbers, reading
  order, six links to the right pages, bookmarks still right, undone; no bookmarks refused);
  unit tests (page shift, running heads); contents page rendered and looked at; self-test step
  `contents` (five headings listed, one left out, nested bookmarks, a contents page with four
  links, both undone) and the two dialog states.
- **Open:** article threads; duplicate bookmarks; page labels in the contents; bold text of the
  body size as headings.

## 2026-10-21 — FOL-L035/S01: compared documents side by side

- **Current state before:** the differences were marked on both documents, but only one was
  shown at a time; Show › Both switched between them.
- **Changed:** `Comparison::page_map` (the old page beside each new page: where its first word
  both documents share is, pages without one after the page before them). `SplitView::source`:
  the other view of a split shows another tab's document (its sizes, pictures, labels and the
  pages its render service holds), with the difference marks of the page view (now
  `compare_marks`), and is not focused. Show › Both puts the new document on the right and the
  old one on the left; the canvas script scrolls the old view with the new one (the matched page
  at the top, as far into it); moving between differences stays in the new document; a new
  comparison, closing either document or a split of the window ends it; Both again ends it.
- **Checked:** unit test of the page matching (inserted pages, no shared words, empty);
  self-test step `panes`: seven pages against six with one inserted and one word changed, the
  map, the other view marked to follow with six pages and its marks, the old page 4 beside the
  new page 5 after scrolling, Both again back to one view; the render-write check.
- **Open:** geometry and picture differences; following in the horizontal layout, and the new
  document following the old one.

## 2026-10-21 — FOL-L005/S01: Markdown and CSV documents

- **Current state before:** text files became documents as plain text, so Markdown showed its
  marks and CSV its separators.
- **Changed:** `organize/typeset.rs`, a small typesetter (spans in five faces, lines broken by
  the widths of their faces, letters cut only when a word is longer than a line; paragraphs,
  markers, rules, code boxes and tables over pages; links and bookmarks), with the metrics of
  Helvetica-Bold (`util::helvetica_bold_width`); `markdown.rs` (`Document::from_markdown`,
  pulldown-cmark 0.13, MIT, without its HTML writer) and `csvdoc.rs` (`Document::from_csv`,
  `parse_csv`, `guess_separator`). File › Open, dropped files and New › From Text File make
  `.md`/`.markdown`/`.csv`/`.tsv` files into these documents.
- **Checked:** `tests/markdown_csv.rs` (6: styles, faces, bookmarks nested, the title, a link
  over its words, right-aligned prices; forty sections over many pages inside the margins; text
  beyond WinAnsi in FreeSans and CJK refused; a CSV of 150 records with its header on every page
  and amounts aligned; a wide CSV sideways; empty and undrawable CSV refused), 8 unit tests
  (blocks, task lists and pictures, wrapping, faces, nesting of headings, quoted fields,
  separators, local numbers); pages rendered and looked at; self-test step `email` opens a
  Markdown and a CSV file.
- **Open:** RTF (S02); Markdown pictures, CSS and formulas; options for CSV (separator, header,
  paper) in the interface; code in the Unicode font is not monospaced.

## 2026-10-21 — FOL-L012/S01: Clean Up Scans

- **Current state before:** blank pages could be deleted (pages that draw nothing) and a picture
  straightened one at a time; a blank scanned side is a picture, so it was never found, and every
  askew sheet had to be straightened by hand.
- **Changed:** `organize/scans.rs`: `Document::scan_picture` (one picture covering 85% of the
  page), `look_at_scan` (`ScanLook`: ink share away from the edges, specks and show-through left
  out; skew angle), `scan_fixes` with `ScanThresholds`, `clean_up_scans` (straighten, then
  delete, one undo step; every page cannot be deleted). The Clean Up Scans dialog (Convert,
  beside the page) looks at the pages away from the interface thread with progress and Stop,
  lists the blank and askew pages with thumbnails, thresholds and kinds to choose, each page to
  leave out or show, and refuses to apply once the document has changed (Look Again).
- **Checked:** `tests/scans.rs` (3: a blank sheet, a sheet turned 2 degrees and a page that is
  not a scan found as such; cleaned in one step and undone; every page refused, a missing page
  refused, nothing changed), 2 unit tests (ink without edges, specks and show-through; fixes by
  thresholds); self-test step `scans` (found with pictures, a stricter angle, applied, undone)
  and the dialog state.
- **Open:** dark borders, orientation, despeckle and black and white in the workflow; fronts and
  backs interleaved (with the scanner, FOL-L011).

## 2026-10-21 — FOL-L014/S01: review of the words read with doubt

- **Current state before:** text recognition wrote every word alike; nothing told which words
  might be wrong, and a wrong word could only be fixed by reading the page again.
- **Changed:** each word keeps the probability of its least sure letter (`OcrWord::confidence`);
  words below `DOUBTFUL_BELOW` (0.8) are written inside `/FolitectaDoubt <</Confidence c>> BDC
  ... EMC`, so the doubt survives saving and follows undo. `Document::doubtful_words` reads them
  back from the page (text, confidence, place from the text matrix and stretch, turned pages
  included); `Document::review_ocr_word` keeps, corrects (the run stretched to the same length)
  or removes one in one undo step. `Renderer::render_area_png` draws only an area of a page. The
  dialog Review Recognized Text (beside the page, the word marked on it) shows the picture of
  the word and its line, a text field (Enter corrects), Keep, Remove and Skip; it opens after
  OCR Pages when there are doubtful words (an option of the OCR dialog), and the message counts
  them.
- **Checked:** `tests/ocr.rs` (4 new: doubtful words listed where they were read, on a turned
  page too; corrected, kept and removed, one undo step each; kept doubtful in a saved file; the
  words a 45 dpi scan reads wrong are doubtful, a clean scan has none); a renderer test of an
  area against the whole page; self-test step `ocr` with a page of three doubtful words (one
  corrected with Enter, one kept, one skipped, shown again and removed) and the dialog state.
- **Open:** field recognition proposals (S02); no threshold to choose; the spell check does not
  look at doubtful words first.

## 2026-10-21 — FOL-L018/S03: Unicode in page marks, summaries, text and e-mail documents

- **Current state before:** watermarks, headers and footers, page and Bates numbers, comment
  summaries and documents made from text or e-mail wrote the standard Helvetica in WinAnsi, so
  other characters became `?`.
- **Changed:** page marks draw through `TextWriter` (`text_object` with a rotation matrix): the
  macros are filled in first, the font is chosen for all the texts of a mark before the edit
  begins, and the Unicode subset goes on every marked page under a name none of them uses yet
  (`FoliUni1`, `FoliUni2`, ...; `resource_name`, `use_resource_name`), so an earlier mark keeps
  its font when pages share resources; a failure after the edit begins abandons it. Summaries
  (all three layouts), text documents and e-mail documents use one writer for the whole
  document and one subset on all its text pages; their bold is stroked in the Unicode font. Text
  documents split pages at form feeds before the text is prepared. The watermark, header and
  footer and numbering dialogs name text the font cannot draw while it is typed.
- **Checked:** `tests/unicode_drawings.rs` (11 new: marks shared by every page, a later mark with
  its own font name and removal of one mark leaving the other, page and Bates numbers, WinAnsi
  marks kept, refusals before any change including a macro that expands to Hebrew; summaries in
  the three layouts with a reply, saved and reopened; WinAnsi summaries; refused summaries; text
  documents over two pages, WinAnsi kept, refused; e-mail documents with stroked bold, WinAnsi
  kept, refused); `fit` unit test with the Unicode font; self-test step `unicode` with a watermark
  typed in Polish and Greek (undone) and a refused one.
- **Open:** right-to-left text and shaping still refused; S04 (form fields) and S05 (OCR,
  barcodes, signatures, media, redaction text) still write WinAnsi.

## 2026-10-20 — FOL-L018/S02: Unicode in tables, calculator tapes and stamps

- **Current state before:** designed stamps, tables and calculator tapes wrote their text with the
  standard Helvetica in WinAnsi, so other characters became `?`; text stamps refused them.
- **Changed:** `TextWriter` draws a drawing of several pieces of text with one font
  (`for_pieces`, `piece`, `wrap_piece`, `text_object`); the bold of the Unicode font is filled
  and stroked inside `q`/`Q`; WinAnsi drawings keep their fonts and their bytes. Tables, tapes,
  designed stamps and text stamps (and their appearances drawn again when moved or resized) use
  it, with the subset of a drawing made again used again (`previous_font`, now in `fonts`). The
  table editor names text it cannot draw while it is typed and stays open when its table is
  refused.
- **Checked:** `tests/unicode_drawings.rs` (8: tables, tapes and designed stamps beyond WinAnsi,
  saved and reopened, found after flattening; WinAnsi kept; refusals leave the document, its
  bytes and its history unchanged; the subset used again and a new one for new characters, undo);
  the text stamp test of `unicode_text.rs` rewritten for the new behaviour; writer unit tests (3
  new); self-test step `unicode` with a table typed in Polish and Greek, a refused table (editor
  kept open, nothing changed) and a tape comment in Polish (9 interactions, 0 problems).
- **Open:** right-to-left text and shaping still refused; no real bold or italic face (bold is
  stroked); S03 (page marks, Bates numbers, summaries, text and e-mail documents), S04 (form
  fields) and S05 (OCR, barcodes, signatures, media, redaction text) still write WinAnsi.

## 2026-10-20 — release 0.2.1: check of the published packages

- **What:** the eight files of the v0.2.1 release downloaded and checked by four independent
  checks and a completeness critic, after the release (the release page itself was left as
  published).
- **Found right:** SHA-256 of every file equal to the `.sha256` files and to the digests GitHub
  shows (8 of 8); every package holds what `release.yml` puts in it, byte for byte with the tag
  (no `fonts/source`, no build or hidden files); Windows PE32+ x86-64, Linux ELF x86-64 (glibc
  2.35 or newer), macOS universal (x86_64 and arm64, Info.plist 0.2.1); no private data in the
  packages; the release body equals `docs/RELEASE_NOTES.md`. The CI of the tag passed on Linux
  (770 tests), macOS (769) and Windows (757).
- **Found wrong, fixed in the documents:** the Linux program links libxdo 3 (through the menu
  component of the interface toolkit, never called by Folitecta) and does not start without it:
  Arch, Manjaro and CachyOS have only libxdo 4, and Ubuntu desktops do not install libxdo3 (the
  same in 0.2.0). With a compatibility link the published package passed the full interface
  self-test on this PC (554 entries: 277 states and 277 interactions, 0 problems). The README and
  the release notes now give the libraries per system and the link; the program will start
  without libxdo in 0.3.0. Also corrected: exported pictures follow the default layer visibility
  (the notes said comments of hidden layers were hidden everywhere), the GitLab releases stop at
  0.0.12, the Visual C++ runtime of the Windows program, the first start on macOS 15, names of
  menus and dialogs, the zoom figures, the Windows checksum file (CR LF, refused by the checker
  of macOS; LF from the next release) and a visible warning when the GitLab release is skipped.
- **Open (planned for 0.3.0 in ROADMAP.md):** libxdo not needed at start, licence notices of the
  libraries in the packages, the Visual C++ runtime built into the Windows program with its icon
  and version, an ad hoc signature of the macOS app, singular forms of counted messages, exported
  pictures following the view. Not verified at all: the Windows and macOS packages on a real
  machine, real printers, real mail programs, servers on the internet.

## 2026-10-20 — release 0.2.1

- **Tag:** `v0.2.1` on `main` (full CI on Windows, macOS and Linux; release packages).
- **Contents:** FOL-L018/S01, FOL-L053/S01, FOL-L036/S02, FOL-L040/S02 with FOL-C006/S02,
  FOL-L037/S02 (the 0.2.1 plan), and also FOL-L046/S01 (printing), FOL-L023/S02 (drawing on the
  selected layer), the speed of the interface and the interface self-test on Linux.
- **Checked:** 769 tests in 67 suites, clippy without warnings, licences, and the full interface
  self-test in a nested compositor on Linux (554 states, 277 interactions, 0 problems) after the
  last merge; three adversarial reviews of the speed work;
  the packages of the release downloaded and their SHA-256 compared.

## 2026-10-20 — FOL-L001: speed of the interface (scrolling, zoom, long documents)

- **Commits:** merge 43fc7f1 "Merge the speed of the interface (FOL-L001) after drawing on
  layers" (branch commits 926df43…e923296) and f56d855 (pages changed by a redaction applied in
  the background are drawn again).
- **Current state before:** every scroll, wheel event and mouse move was a synchronous request to
  the interface thread; every page and thumbnail was drawn again at each change, each page
  walking the page tree (about 350 ms per change at 1000 pages); each render thread had its own
  first-in-first-out queue without priority, merging or cancellation, and every zoom step asked
  for every visible page again; pictures were compressed as PNG; every page stayed in the page
  view and every edit redrew every page.
- **Changed:** a page script follows scrolling, the current page and Ctrl+wheel, and mouse and
  wheel events reach the program only where listened to; page ids and the comments, outline,
  links, destinations and fields of a revision kept; per-page revisions (`changes`) so an edit
  redraws the pages it touched and keeps the text of the others; `render_service`: one queue per
  document ranked by the window, duplicates merged, stale requests refused, a picture cache
  shared by the documents (256 MB, lowered to 128 MB in the memory round below), thumbnails
  from the page pixels, several threads for a page
  drawn alone, uncompressed pictures; a page window with placeholders and small previews; zoom
  settled after 90 ms; 512-pixel tiles at high zoom and for the loupe; GPU composition on Linux
  (`FOLITECTA_GPU`).
- **Checked:** `render_bench` (generated documents, medians of 10); core tests `page_lookup`,
  `page_revisions` and unit tests of the render service; self-test step `perf` comparing with
  0.2.0 in a nested compositor (1280×800, load 1.2–3.2, medians of 3, builds alternated). On the
  1000-page document: sharp after opening 17–46 ms (0.2.0: 416–462 ms); longest pause while
  scrolling 41–44 ms (401–409 ms); six Ctrl+wheel steps sharp after 107–124 ms (556–577 ms);
  a text selection press 3 ms (203–208 ms). On the 50-page document: dynamic zoom sharp after
  0.20–0.22 s (2.36–2.38 s). Synchronous event requests while scrolling or hovering: 0 (91–241).
- **Review fixes:** 275e0ae (a render thread no longer loops when the document changes twice in
  a row), c0a4c14 (tiles of large pages at high zoom drawn at their scale), 66f90d0 (one picture
  store of 256 MB for every document, freed on close; 128 MB after the memory round), e55722d
  (each view tells the render threads
  which pages it holds before asking their pictures; failed pictures asked again at most 3 times;
  self-test step `jumps`), e965678 (bookmark rows keep their element on press, so clicks reach
  them), d90276e (picture addresses only grow; numbers hashed by value, so an undo no longer
  redraws untouched pages), 3860324 (GPU composition only with a render node, no NVIDIA driver and
  no virtual machine), 7925709 (the `perf` step measures Go To Page). Each with a test that failed
  before. Go To Page to a far page of the 1000-page document is sharp after 42–44 ms (0.2.0:
  394–400 ms). Memory round (merges 1d58434 and 53cf5aa): glibc kept the pictures' memory in 26
  per-thread arenas (999 of 1224 MB at the peak). 4ce4145 lowers the picture store to 128 MB;
  7941c16 asks glibc to give freed memory back (`malloc_trim`, on a thread of its own, after a
  second without drawing and when a document closes) and ca943a9 limits it to four arenas, both on
  Linux only. A C allocator (mimalloc, f6a191a) was tried and removed (8002c53): it kept more
  memory than this and would have been the one dependency not written in Rust. Interface process,
  0.2.0 → final: peak 926–936 → 810–834 MB; end of the probe 877–918 → 247–252 MB; after the
  50-page document 472–479 → 422–425 MB; after the 1000-page document 775–786 → 466–474 MB; speed
  unchanged. Second review fixes: 0bf05b0 (integers hashed as
  integers), 767c52e (a failed picture is drawn once per address), 9cb36c3 (a thumbnail asked
  with a preview set aside is no longer dropped), 19d242f (virtual machines found by their
  graphics driver), 2a2bad0 and f2f857f (real clicks with increasing times; `jumps` covers the
  split window and tab switching), 5a4a955 (stress tests of the render service). Third review
  fixes: 5aadf11 (failed pictures tried again with new bytes), c1b0a3c (a render thread reads the
  stop with its last decision, so no answer is lost on close), dcffac7 (`jumps` waits for the
  pictures of the active document).
- **Open:** pinch zoom and real HiDPI screens not measured (the nested compositor gives a pixel
  ratio of 1); decoded JPEG pictures not kept between renders; the document is serialized on the
  interface thread at each commit (about 10 ms at 1000 pages); slower than 0.2.0: a jump with the
  scroll bar (33–48 ms instead of 0–11 ms for pages 0.2.0 kept in the page), thumbnails of heavy
  drawings (0.36 s instead of 0.18 s), 400 % zoom on the 50-page document (0.19–0.26 s instead of
  0.17 s); memory measured on Linux only (Windows and macOS keep their allocators as in 0.2.0);
  the 128 MB picture store holds about 85 text pages at 100 % but about 5 at
  scale 4.

## 2026-10-19 — FOL-L023/S02: drawing and writing on the selected layer

- **Commits:** merge "Merge drawing on the selected layer (FOL-L023/S02) after the effects
  check" (branch commits 4745cfb, dbddba0, 60c8a55, review fixes 44c3db3…dd7502c).
- **Current state before:** layers could only be shown, hidden, merged, flattened and imported;
  new comments had no layer; comments of hidden layers were still drawn and listed; pasting a
  comment copied its layer object.
- **Changed:** `layer_drawing`: the drawing layer, `/OC` on drawn comments, marked content of the
  layer around placed pictures and barcodes, comments and pictures moved between layers,
  comments of hidden layers hidden in the bytes the renderer reads (views cached per revision and
  written as small updates: 1,000 pages × 20 comments × 20 layers went from 19/82 ms to 2/4 ms);
  visibility expressions evaluated as printing does; flattening keeps the layer; exchanges carry
  the layer by name; resources other pages share are copied before something is added;
  rasterizing, summaries and file thumbnails follow the layers; PDF/A-1 refuses layers.
  Interface: pane mark, status chip, Choose Layer, Default Layer, Move to New Layer, the Layer
  field of the properties, the Comments pane option.
- **Checked:** `tests/layer_drawing.rs` (27); self-test step `layer_drawing` (14 interactions);
  629 workspace tests and a full self-test (275 states, 177 interactions, 0 problems) on the
  branch.
- **Open:** comments whose layer is a membership dictionary lose it on export and paste; XFDF
  carries no layer; rasterizing a rotated page drops its comments; exported pictures follow the
  default visibility, not the view; the drawing layer is not remembered between sessions; not on
  layers: redaction marks, form fields, links, signatures.

## 2026-10-19 — FOL-L046/S01: print sheets, print window, direct printing

- **Commits:** merge 8df021a "Merge the print overhaul (FOL-L046): own print window, sheets,
  direct printing" (branch commits b0e24cb…12ac5ec, review fixes e26bf16…cfa4399, bd281ca,
  41f9125, 33acf3a).
- **Current state before:** pictures of the pages through the dialog of the web view only; no
  pages per sheet, booklet, poster, two-sided options, printer choice or permission check;
  Portfolio › Print printed every open document.
- **Changed:** a planner of sheets in the core and a vector print file (checked, protected when
  saved), a band renderer for pictures of sheets; IPP to the local print server (Linux, macOS),
  GDI (Windows) and a fake backend for tests; the print window with its sheet preview, the job
  runner and the status chip; print permissions (user and owner password); self-test step
  `print`.
- **Checked:** 80 print tests (1 ignored); the IPP transport against an in-test server; the
  Windows code type-checked against the `windows` crate on Linux; full self-test on the branch
  (270 states, 165 interactions, 0 problems).
- **Open:** real printers on Linux, macOS and Windows (order, scale, margins, counts); vector
  output on Windows; about 40 lines of Windows-only window code not type-checked; listing the
  printers opens each one (slow with many network printers); a booklet printed on both sides
  needs a printer that turns the paper itself.

## 2026-10-19 — FOL-L037/S02: vector paths, reused drawings and sanitizing in redaction

- **Commits:** merge 1374d9e "Merge redaction of vectors, reused drawings and sanitizing
  (FOL-L037) after printing" (branch commits e915b1f…6b9ad74 and three rounds of review fixes).
- **Current state before:** paths under the marks were covered, not removed; text and pictures
  inside reused drawings forced the page to a picture; no sanitizing.
- **Changed:** fills and clips cut into bands around the marks, strokes against the marks
  widened by half the line width and cut in user space with their joins; copies of reused
  drawings (a list of allowed keys, structure retargeted); undrawn originals purged from shared
  and inherited resources, the structure tree and comments; Type 3 glyph procedures, patterns
  and picture metadata of removed content removed; XFA always removed; sanitizing (metadata with
  PDF/A and PDF/UA claims kept, attachments and media, hidden layers, comments and fields,
  quotes of the removed words in bookmarks, links, destinations, tags and document information,
  scripts and risky actions); the document security store goes; checks in memory and on the
  bytes before the new file is written; the apply dialog.
- **Checked:** `redaction_vectors` (25+), `redaction_sanitize` (19+), `redaction_reuse` (9), a
  2,000-page timing test; self-test step `redaction`; 617 workspace tests and a full self-test
  (431 states, 0 problems) on the branch.
- **Open:** rectangles only; nothing removed outside the crop box; the moves that replace removed
  glyphs keep their widths (the length of a word can be measured); patterns and drawings other
  pages draw stay for them (reported); quotes in page text, comments and field values are
  reported, not changed; the quote search reads at most 100 pages of long documents.

## 2026-10-19 — FOL-L040/S02, FOL-C006/S02: revocation and validation material

- **Commits:** merges eb6e5b1, 003814c and 6400d9f (branch commits 2fbbe5d, ed6e156, 0d44817,
  e5989e5, fc26766, 88f499a, 1b4c00c, e2a5210, 973b35c, b2de1ba).
- **Current state before:** revocation was always "not checked"; no document security store;
  any later revision counted as a change after signing.
- **Changed:** OCSP answers and revocation lists checked for the time the signature is proven to
  exist, then trust and expiry rules; data from the signature, its time stamp, the document
  security store, or fetched online when turned on; Add Validation Material writes DSS and VRI as
  an incremental update (encrypted for protected files); revisions that only add validation
  material are recognised when they are regular updates and a second reader (the renderer's)
  reads every object the same; validation runs in a task off the interface thread.
- **Checked:** `revocation` (27), `material_revisions`, `xref_shadowing`, irregular update
  shapes; online fetching only against 127.0.0.1; self-test step `revocation` (5); full
  self-test (415 states, 0 problems) on the branch.
- **Open:** indirect, delta and reason-partitioned lists, points named relative to the issuer,
  AIA, policy checks, OCSP nonce; not tried against public servers or another validator; no
  PAdES/LTV claim; protected files of other producers that keep objects in object streams show
  their material revisions as changes.

## 2026-10-19 — FOL-L036/S02: AES-256, protected documents signed and saved encrypted

- **Commits:** merges 0eb7894, bf86a01 and e39f19f (branch commits dc57801…245a1d3, e62921d,
  48ce0e5, 576515e, 4d16828, e368f6b, 955a4f0).
- **Current state before:** AES-128 only; signed and time-stamped outputs of encrypted documents
  were written plain; saving a signed encrypted document broke its signatures; owner passwords of
  revisions 2–4 and passwords outside ASCII decrypted into noise; documents made from protected
  ones were plain.
- **Changed:** `EncryptionProfile` (AES-256 default, AES-128), `Protection`,
  `protection::encrypted` (encrypted incremental updates, signature values left unencrypted,
  check of `/Encrypt`); passwords tried as typed and composed, in PDFDocEncoding and UTF-8, and
  SASLprep for AES-256; a key is used only once the user password it is made from is verified
  (Algorithms 6 and 7); gates on derived documents, imports, recovery, history and files handed
  to other programs (private folder); Dialog::Password targets.
- **Checked:** `protection_profiles`, `protected_signatures`, `protection_gates`, owner and
  composed/decomposed password tests (R2, R3, R4 RC4 and AES-128, AES-256); self-test step
  `protection`; 527 workspace tests and a full self-test (427 states, 159 interactions,
  0 problems) on the branch.
- **Open:** certificate encryption (S03); changing or removing the protection needs only the
  password to open (kept as it is by choice); permissions not enforced in Folitecta beyond
  printing; strings in stream dictionaries not encrypted; no independent validator tried on
  encrypted signatures.

## 2026-10-19 — FOL-L053/S01: e-mail attachment dialog

- **Commits:** merge b038555 "Merge the e-mail attachment dialog (FOL-L053) after Unicode
  generated text" (branch commits 06d1490, a3ea3e7, 870f18f, 8bec9b0).
- **Current state before:** no dialog; documents with changes copied on the interface thread;
  form data as XFDF only; drafts opened by themselves and a `mailto:` fallback; a shared
  temporary folder; copies of some pages kept data of the other pages.
- **Changed:** the attachment dialog (versions, pages, comments, form data and comments as files,
  ZIP, sizes and previews, Save Draft…, Open in Mail Program); `keep_only_pages` for page
  copies; failures reported in the dialog; private folder for the files handed over; RFC 2231
  file names and folded subjects in drafts; one-pass flattening.
- **Checked:** `tests/delivery.rs` (17), `mail.rs`, `mail_draft.rs`; stand-in `xdg-email`
  programs; self-test step `email`; full self-test (262 states, 142 interactions, 0 problems).
- **Open:** not tried with real mail programs (Simple MAPI, xdg-email, Mail); Extract Pages,
  Split and Delete Pages onto `keep_only_pages`; document-level data (XMP, embedded files,
  scripts) stays in every copy.

## 2026-10-19 — FOL-L018/S01: Unicode in generated text

- **Commits:** f8d163b, 839e2de, cd0f20a, 5e69af0, e8385ac, db2c7d5, 6139ab8, 9121860, d39d8fe.
- **Current state before:** `?` for every character WinAnsi lacks.
- **Changed:** `fonts`: GNU FreeFont FreeSans in `fonts/` with its licence and signed source
  (`fonts/source`, signature checked with key A015 6C13 9D2D AA3B 352E 42CD 5063 61DB A36F DD52);
  text boxes, typewriter text, callouts and measurement labels embed a subset with ToUnicode;
  refusals named before the change and while typing; comment imports choose; text stamps refuse;
  release packages carry `fonts`.
- **Checked:** 378 tests in 52 suites; self-test step `unicode`; full self-test (257 states,
  145 interactions, 0 problems).
- **Open:** right-to-left text and shaping refused; FreeSans is about 2% narrower than Helvetica
  for Western text it replaces; other producers still write `?`: designed stamps, tables, tapes
  (S02), page marks, Bates, summaries, text and e-mail documents (S03), form fields (S04), OCR,
  barcodes, signatures, media, redaction text (S05).

## 2026-10-19 — interface self-test on Linux

- **Commits:** 18941ab, 24287b7, 55c1884, 6e2de25, 8af6985, fa45a4b, 3eda722; a32d07c (effects
  check).
- **Current state before:** the self-test was run on Windows only; on Linux under Wayland it
  hung; the time stamp tests failed after noon UTC.
- **Changed:** progress log, time limits on page scripts and steps, a watchdog; the interface
  keeps running while its window is not drawn; results of page scripts no longer lost; window
  sizes applied and checked on Wayland; sounds and videos play on Linux; the time stamp tests use
  a time after the moment they run; a detector of state written while a component renders and
  the `effects` step (Split Pages, picture editor, portfolio preview, search, with the signatures
  pane and the details dialog open); several steps in one run (`step:a,b`).
- **Checked:** full self-test in a nested compositor on Linux (KWin, Wayland), 0 problems; the
  guide in ai/WORKFLOW.md.
- **Open:** screenshots of the states are taken on Windows only; writes to global signals are not
  instrumented by the detector.

## 2026-10-18 — release 0.2.0

- **Tag:** `v0.2.0` on `main` (full CI on Windows, macOS and Linux; release packages).
- **Contents:** FOL-L051/S01, FOL-L036/S01, FOL-L037/S01, FOL-L006/S01, FOL-C006/S01 with
  FOL-L040/S01, the e-mail work of FOL-L053 and the ribbon changes below. Steps 4 (attachment
  dialog) and 7 (Unicode generated text) of the 0.2.0 plan moved to 0.2.1.
- **Checked:** 324 tests in 50 suites; full interface self-test before tagging (see the entry
  below); the packages of the release downloaded and their SHA-256 compared.

## 2026-10-18 — FOL-C006/S01, FOL-L040/S01: time stamps and separate checks

- **Commit:** "Time stamps (RFC 3161) and separate checks of signatures; Folitecta 0.2.0".
- **Current state before:** no time stamps at all; validation gave one state per signature
  (valid, changed, trusted) and the signing time came from the signer's computer only.
- **Changed:** `digsig::timestamp` builds RFC 3161 requests (SHA-256 imprint, random nonce,
  certificate requested), checks the answer (status, imprint, nonce, signature of the authority,
  time stamping usage of its certificate, validity at the stamp time; genTime with fractions
  accepted) and holds `respond`, a local authority for tests. `cms_sig::sign_content` signs
  detached data or embedded content and puts the token of the signature value in the unsigned
  attribute `id-aa-timeStampToken`; `Document::sign_with` signs with a time stamp,
  `Document::time_stamp` adds a document time stamp (`ETSI.RFC3161`, incremental on signed
  files); `DigitalId::create_time_stamp_authority`. Validation reports `checks` (integrity,
  identity, time, revocation: not checked), `time_stamp`, `document_time_stamp` and
  `later_revisions_time_stamped` (later revisions that all end with valid document time stamps;
  what else they change is not analysed, so integrity stays a warning). Interface: an HTTP/1.1
  client (`http.rs`, TLS of the system through native-tls, redirections, chunked answers), the
  time stamp option and server address in the signing window, Document › Signatures › Timestamp,
  the checks and the time stamp in the signature details, a clock chip in the pane.
- **Checked:** core tests `timestamp` (tokens answer their request, other data and changed
  tokens fail, other nonce and refusals are errors; authorities without the usage are reported;
  document time stamps valid and trusted once the authority is; a signature with a time stamp
  passes every check, without one the time is a warning; a document time stamp after a signature
  keeps it valid; a later edit is not counted as a time stamp; failing transports leave no file);
  HTTP client tests against a local server (redirection, chunked answer, server error); self-test
  step `timestamp` (local authority over HTTP: a signature with a time stamp and a document time
  stamp, pane, four checks in the details).
- **Open:** not tried against a public time stamp server; revocation (OCSP/CRL) and long-term
  validation material; analysis of the changes of later revisions; time-stamping an encrypted
  document writes a plain file (FOL-L036/S02).

## 2026-10-18 — FOL-L037/S01: redaction by content

- **Commit:** "Saves are atomic and keep the encryption of protected files; redaction by content" on `main`.
- **Current state before:** applying redaction drew every marked page as a JPEG picture: the
  data under the marks was gone but so was the text of the whole page; comments and form fields
  over the marks stayed (with their contents and values); a signed document was then saved as an
  incremental update, keeping the earlier revision with the content in the file.
- **Changed:** the text probe of `textedit` records the advance of every glyph (its width with
  the spacing, in thousandths of the font size) and the bounds of every picture drawn;
  `remove_text_in` takes the glyphs under the marks out of their text operations, each replaced
  by a `TJ` move of its advance, so the rest of the line keeps its place; pictures the page draws
  itself get their pixels under the marks covered (`put_page_image`, no undo step of its own);
  comments over the marks and form fields with a widget over them are removed with their values
  (`remove_form_field`); the covers and overlay texts are drawn upright for the reader on rotated
  pages; unreferenced objects are pruned and the document stops being a base for incremental
  updates. Pages with text or pictures inside reused drawings, pictures written in the content or
  fonts the editor cannot handle fall back to the picture method; the redaction settings offer
  the picture method for every page. One undo step for the whole operation.
- **Checked:** core tests `redaction_content` — a word under a mark leaves every stream of the
  saved file (as text and as hex codes) and the next word keeps its position; half of a picture
  becomes black and the other half stays white; a note and a text field over the mark go, their
  texts absent from the saved bytes, a note elsewhere stays; on a page turned by 90° the code is
  upright inside the mark; a signed document is saved with one revision only. Updated
  `document_ops_2` (text outside the marks stays; the picture method still makes the page a
  picture).
- **Open:** vector paths under the marks; reused drawings; metadata and other sanitizing.

## 2026-10-18 — FOL-L036/S01: protected files stay protected

- **Commit:** same as above.
- **Current state before:** opening an encrypted file decrypted it and dropped the encryption;
  Save, Save As and Save Copy then wrote plain files (confirmed in `document.rs`, `load_inner`).
- **Changed:** the lopdf `EncryptionState` of the opened file (file key, O/U values, permissions,
  crypt filters) is kept and applied again to a copy of the document when it is written, the old
  encryption dictionary removed; the `/ID` is kept, so the same user and owner passwords open the
  saved file with the same permissions. `Document::save_unprotected_copy` and Save Unrestricted
  Copy write a plain copy on purpose. Undo snapshots stay plain in memory.
- **Checked:** core tests `keep_protection` — a file with user and owner passwords stays
  encrypted after Save As, Save and Save Copy, both passwords open it, `/P` unchanged, the edit is
  kept; a file with restrictions but no open password keeps its encryption dictionary and `/P`;
  an unrestricted copy opens without password while the document stays protected; plain files
  stay plain.
- **Open:** signing or time-stamping an encrypted document writes a plain signed file; AES-256
  for new protection; certificate encryption.

## 2026-10-18 — FOL-L051/S01: atomic saving

- **Commit:** same as above.
- **Current state before:** `Document::save_as` and `save_copy` wrote the target with
  `std::fs::write`, so an error in the middle (disk full, a locked file) could leave a truncated
  original.
- **Changed:** `util::write_atomic` (temporary file in the same folder, `sync_all`, rename over
  the target, temporary file removed on failure, symbolic links followed) and
  `document::write_pdf_file` (refuses bytes that are not a whole PDF) are used by Save, Save As,
  Save Copy, protected copies, the signed file written after signing and the settings file.
- **Checked:** core tests `atomic_save` — replacement leaves no temporary file; a failing rename
  (target is a folder) and a missing folder leave everything as it was; incomplete PDF bytes are
  refused and the original kept; a failed Save As leaves the document unsaved; on Windows a file
  locked by another program keeps its content and the document stays unsaved.
- **Open:** a versioned session format; exports (pictures, text, attachments) still write
  directly, they never replace the document.

## 2026-10-18 — interface: short tool names, ribbon group edges, submenus

- **Requirements:** FOL-L050 (consistent ribbon/menus), FOL-L042 (readable interface).
- **Commit:** `c7d3599`.
- **Changed:** the ribbon tools are tiles with a short name under the icon (`Pencil`, `Loupe`,
  `Zoom +`, `Zoom −`; 275 names per language in `assets/i18n/<lang>/short.json`, others shortened
  from their label), on by default; View › Tool Names shows icons alone; tooltips keep the whole
  name and the shortcut. Rows of tiles and rows of icons are saved separately. Dragging the edge
  of a ribbon group now picks the number of tools whose edge is nearest to the pointer (a model of
  the real widths) instead of a fixed number of pixels per column; moves without a pressed button
  are ignored during drags. An open submenu waits 300 ms before another row replaces or closes
  it, so the pointer can cross rows on its way; entering the submenu cancels the change; a click
  opens a submenu instead of toggling it, and its position is taken from the row.
- **Checked:** unit tests (short names, nearest-edge drags, tile widths); self-test step `layout`
  7/7 (pane edge +90 px, ribbon rows, group edge follows a 90 px drag, all tools shown at the
  end, short names and tooltips, names hidden and shown, submenu kept while crossing and closed
  after resting); full interface self-test: 255 states, 136 interactions, 0 problems.
- **Open:** short names for the extra tools of widened groups are derived automatically.

## 2026-10-18 — PDF from e-mail messages

- **Requirement:** FOL-L006 (EML profile).
- **Commit:** `c7d3599`.
- **Changed:** `folitecta_core::email` reads messages with `mail-parser` (any charset, encoded
  words, quoted-printable, HTML made into text) and `Document::from_email` typesets the subject,
  the header lines and the text on pages, embedding the attachments; File › Open and
  Create › From E-mail open `.eml` files as new documents.
- **Checked:** core tests (a multipart message with an encoded subject, quoted-printable text and a
  PDF attachment; a long message on several pages; save and reopen with the attachment); self-test
  step `email` (an `.eml` opened from disk: new tab, text on the page, attachment bytes).
- **Open:** pictures of the message on pages; mail archives.

## 2026-10-18 — e-mail with attachments, EML drafts and ZIP archives

- **Requirement:** FOL-L053 (partial).
- **Commit:** `c7d3599`.
- **Changed:** `sendByEmail`, `sendByEmailZip`, `sendByEmailOpenAll`, `sendByEmailOpenAllZip`,
  `forms.emailData`: the files (the saved file when it holds every change, a copy otherwise) are
  attached through Simple MAPI on Windows, Mail on macOS or `xdg-email` on Linux; when no program
  takes attachments, an unsent EML draft (`X-Unsent: 1`) with the files attached is saved and
  opened with the program that reads e-mail files; only then a `mailto:` message opens with the
  folder of the files. `folitecta_core::zip` writes and reads ZIP archives (deflate through
  flate2, CRC checks).
- **Checked:** ZIP round trip and damaged archives, EML draft round trip (core tests); names,
  outbox, numbering of duplicate names, drafts holding the files (interface tests); the `email`
  self-test step reads back a draft written by the mail code. **Not tried with real mail
  programs** (it would open the user's mail program).
- **Open:** a dialog to choose and inspect the attachments.

## 2026-10-18 — smaller modules (no change of behaviour)

- **Commit:** `705cff8`.
- **Changed:** `files.rs` → `files/`; canvas previews and comment creation in their own files;
  `markup/appearance/` (style, shapes, note icons, measurement labels, drawing); comment exchange
  split into XFDF, FDF and helpers; form field drawing in `interactive/form_appearance.rs`.
- **Checked:** 287 tests; full interface self-test: 255 states, 134 interactions, 0 problems.

## 2026-10-18 — release 0.1.0

- **Commit and tag:** `08f685b`, `v0.1.0` (full CI on Windows, macOS and Linux; release
  packages with SHA-256 files).
- **Requirements advanced:** FOL-L013 (local OCR of pages, areas and pictures), FOL-L019 (spell
  checking with corrections), FOL-L050 (Quick Access Toolbar), FOL-L031 (items of choice fields,
  pictures on push buttons), FOL-L036 (security policies), FOL-L025 (grouping comments).
- **Checked:** the Windows package downloaded from the release, SHA-256 verified, `ocr` and
  `spelling` self-test steps passed with the packaged executable and its data folders.
