# Folitecta roadmap and progress

State of every requirement of [REQUIREMENTS.md](REQUIREMENTS.md) on the `main` branch at
Folitecta 0.13.0, reviewed on **1 November 2026** against the code (not only the command
registry). Update the row of a requirement whenever a sub-task changes it, and log the work in
[PROGRESS.md](PROGRESS.md).

| Mark | Meaning |
|---|---|
| ✅ | Available for the profile described in the row, with tests; keep it working. |
| 🟡 | Partial: a concrete part works, the gaps are listed. |
| 🔵 | In progress on `main` (some parts committed, not finished). |
| ⬜ | Not started. |

Command-level detail (1992 registry commands: 1465 available, 59 partial, 468 planned) is in
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

### 0.3.0 — daily work (released 26 October 2026)

Released as planned, each item at its first sub-task: FOL-L014 (OCR review), FOL-L012 (scan
clean-up workflow), FOL-L005 (CSV, Markdown, RTF), FOL-L035 (synchronized side-by-side comparison), FOL-L018/S02–S03 (Unicode in stamps, tables, tapes and
page marks), packaging found by the 0.2.1 release check (the Linux program starts without libxdo,
licence notices of the libraries in every package, the Visual C++ runtime built into the Windows
program with its icon and version, an ad hoc signature of the macOS app, LF in the Windows
checksum file, current versions of the release actions), pictures exported from the pages
following the layers of the view, singular forms of counted messages, FOL-L042 (local text to
speech with the system voices, reading reflow), FOL-L022 (table of contents from bookmarks or
headings, article threads), FOL-L028 (angle, radius, volume), FOL-L029 (tick marks), FOL-L002
(tab groups and windows), FOL-L010 (template gallery), FOL-L011 (scanner through the system
imaging API). Left for later: RTF with styles (FOL-L005), article threads (FOL-L022), scanning
on macOS (FOL-L011).

### 0.3.1 — interface (released 26 October 2026)

Asked after the 0.3.0 release:

1. ~~**FOL-L025 — a smaller pencil pointer on Linux.**~~ Done: WebKitGTK up to 2.54 enlarged
   SVG pointers twice on HiDPI screens; on Linux the pencil pointer is a 20-pixel picture.
2. ~~**FOL-L050 — shorter menus.**~~ Done: every menu of the menu bar keeps at most 22 rows,
   the commands used most at the top and the others in submenus by subject; long submenus of
   the registry are split the same way (`ui/chrome/menu_layout.rs`).

### 0.4.0 — professional tools (released 26 October 2026)

FOL-L045 (recompress one picture, presets), FOL-L043 (PDF/A-2b preflight and conversion of a
copy), FOL-L044 (grey conversion, grey preview), FOL-L021 (vector path editing), FOL-L047 (action
sequences, batch, command line), FOL-L034 (sandboxed JavaScript engine, field scripts, console),
FOL-L039 (smart cards and tokens through PKCS #11, the Windows certificate store, the macOS
keychain), FOL-L009 (Word, RTF and HTML exports). Left for later: embedding fonts in PDF/A
conversion (FOL-L043), colour profiles beyond grey (FOL-L044), clipping and reused drawings in
path editing (FOL-L021), keystroke and page scripts (FOL-L034), real cards and stores tried
(FOL-L039).

### 0.5.0 — drawing at hand (released 28 October 2026)

1. ~~**FOL-L025 — the pencil's quick panel.**~~ Done: choosing the Pencil opens a small panel
   over the pages with six buttons — the thickness of the stroke (0.5 to 8 points), four standard
   colours (black, green, blue and red) and one that opens the colour palette for any other
   colour; they set the style of the next strokes (`ui/features/quick_panel.rs`).
2. ~~**FOL-L025 — the same panel for the other drawing tools.**~~ Done: the free highlighter
   (its own colours and thicknesses) and the rectangle, ellipse, line, arrow, polygon, polyline
   and cloud tools, with the fill of closed shapes.
3. ~~**FOL-L025, FOL-L028 — the panel for the measuring tools.**~~ Done: distance, perimeter,
   area, angle, radius and volume, with the scale of the measures shown and opened from the
   panel; the measuring tools stay chosen.
4. ~~**FOL-L025 — a pressure-sensitive pen.**~~ Done: the Pencil follows the pressure of a pen
   (pointer events of type pen): its width changes along the stroke, drawn as a filled outline
   and kept with the comment (`FolitectaPressure`); a button of the Pencil's panel turns it off.

### 0.6.0 — safe edits (released 28 October 2026)

Page operations keep or update what ties a document together, and say before committing what
they cannot keep (FOL-L052, FOL-L015, FOL-L051).

1. ~~**FOL-L052, FOL-L015 — pages that leave take their relations.**~~ Done: deleting pages,
   extracting them and splitting a document drop the bookmarks, named destinations and links
   leading to the pages that go, their form fields and tags, the opening view and article threads
   through them; nothing of those pages stays in the file (before, bookmarks, destinations and
   tags kept them in it). Delete Pages names what goes with the pages and asks first.
2. ~~**FOL-L015, FOL-L052 — pages that arrive bring their relations.**~~ Done: Insert Pages from a
   file and Combine Files bring the form fields of those pages into the form (a name already used
   gets `_2`, listed before OK), their named destinations, bookmarks and layers, and their tags
   when the document is tagged (counted as left out otherwise); Combine Files puts each file
   under a bookmark of its name (`organize/incoming.rs`).
3. ~~**FOL-L015 — duplicated pages.**~~ Done: the comments of a copy keep their pop-ups and
   replies among themselves; the fields of a copy are widgets of the same fields.
4. ~~**FOL-L052 — a relations corpus.**~~ Done: `testdocs::relations_doc` (bookmarks, named
   destinations, links by destination, name and action, fields over several pages, radio groups,
   comments with pop-ups and replies, labels, the opening view), plain and tagged, through every
   page operation one by one and in a row, saved and reopened, with undo
   (`tests/relations_corpus.rs`).
5. ~~**FOL-L051 — exports through a temporary file.**~~ Done: pictures, text, attachments, form
   data, comments, Office documents, digital IDs and settings go through `util::write_atomic`.

### 0.7.0 — forms that check themselves (released 30 October 2026)

Fields that know what they hold: dates, times, numbers and ranges chosen, not written as
scripts, and checked when a value is committed (FOL-L031, FOL-L032), and the user's own
templates (FOL-L010).

1. ~~**FOL-L031, FOL-L032 — dates and times in the script engine.**~~ Done: `AFDate_*` and
   `AFTime_*`, `util.printd` with every format letter and `util.scand` (English and Italian
   month names, week days left out, two-digit years, only dates that exist); the keystroke script
   runs when a value is committed, so a date that does not exist or a word in a number field is
   refused, naming the format (`interactive/js.rs`).
2. ~~**FOL-L031 — a date field.**~~ Done: the designer's Date tool makes a text field with the
   date format of the language; filling it offers a calendar, and a refused value stays in the
   editor (`ui/features/calendar.rs`).
3. ~~**FOL-L032 — formats and checks chosen.**~~ Done: `FieldFormat` and `FieldCheck`
   (`interactive/form_format.rs`), properties of a text field written as the standard scripts and
   read back from those other programs write; hand-written scripts stay as they are.
4. ~~**FOL-L010 — the user's own templates.**~~ Done: File › Save as Template…; My templates in the
   gallery with origin, date, version and the fonts a template needs, new documents from them,
   rename and remove (`organize/user_templates.rs`).

### 0.8.0 — documents you can trust (released 31 October 2026)

A protected file keeps everything it holds encrypted and Folitecta keeps to what its security
settings allow; actions that reach outside the document ask first; redaction removes what lies
outside the visible page too (FOL-L036, FOL-L052, FOL-L037).

1. ~~**FOL-L036 — strings of stream dictionaries encrypted.**~~ Done: the dates and other strings
   in the dictionary of a stream (an embedded file's `/Params`, for one) are encrypted when a
   protected file is written and decrypted when it is read; files written before, with these
   strings in plain text, still read (`protection/encrypted.rs`).
2. ~~**FOL-L036 — permissions kept in Folitecta.**~~ Done: every change of a file opened with the
   user password is sorted object by object into changing, commenting, filling in and
   assembling (`protection/permissions.rs`); a kind the settings forbid is taken back before it
   is kept, and the owner password is asked with the setting named, which lifts the
   restrictions until the document is closed. Copying text, a page or an area is refused the
   same way.
3. ~~**FOL-L052 — a safe reading mode.**~~ Done: the links of a page and the push buttons of a
   form are followed (they did nothing before): a page, a destination, a named action and
   clearing the form run at once; a web address, a file or another document is named and asked
   first, with a choice to trust the document until it is closed; programs are never started
   and form data is never sent (`commands/navigation/follow.rs`). Scripts of opening and page
   events are not run, as before.
4. ~~**FOL-L037 — what lies outside the crop box.**~~ Done: applying the marks of a page whose
   sheet reaches beyond its crop box takes the content there too (glyphs, pictures, paths, as
   under a mark); Apply Redactions names those pages with a choice, on by default, to leave it
   (`RedactionOptions::beyond_crop`).

### 0.9.0 — text in every field (released 31 October 2026)

The text people type into forms stays what they typed, and files of sessions say what they are
(FOL-L018, FOL-L051).

1. ~~**FOL-L018 — form fields (S04).**~~ Done: values, list items and button captions beyond
   WinAnsi are drawn with the embedded font; a new text that cannot be drawn is refused before
   anything changes, named, and stays in the editor; what a file already holds never fails
   (`interactive/form_appearance.rs`).
2. ~~**FOL-L051 — a versioned session format.**~~ Done: files of sessions carry their format and
   version; what earlier versions wrote still reads, a later version is refused with a message,
   and other JSON files are no longer read as empty sessions
   (`commands/navigation/sessions.rs`).

### 0.10.0 — names in every alphabet (released 1 November 2026)

What Folitecta draws from a person's name keeps its letters (FOL-L018/S05).

1. ~~**FOL-L018 — the visible part of a digital signature.**~~ Done: the name, reason and location
   beyond WinAnsi use the embedded font; a name it has no letters for never stops the signature.
2. ~~**FOL-L018 — typed signatures.**~~ Done: letters Times Italic lacks use the embedded font,
   upright; text that cannot be drawn is refused before any change.
3. ~~**FOL-L018 — the poster of a media comment.**~~ Done: its file name, with the same fallback
   as the signature.
4. ~~**FOL-L018 — redaction overlay text.**~~ Done: codes beyond WinAnsi over the covers, by content
   and as pictures, with a font name free on the page; new marks are checked before they are
   made.

### 0.11.0 — right to left (released 1 November 2026)

Hebrew is drawn and read right to left; values that cannot be drawn are kept (FOL-L018).

1. ~~**FOL-L018 — Hebrew in generated text.**~~ Done: Hebrew letters and punctuation are drawn in
   visual order wherever generated text is drawn (`fonts/writer.rs`), brackets turned, fields
   aligned to the right. Its points stay refused: they need placing over the letters.
2. ~~**FOL-L001 — right-to-left text read in its order.**~~ Done: the lines of a page with Hebrew or
   Arabic letters are put in reading order in the text layer, with the map to their glyphs, so
   search, copying and marks work on any PDF (`page_text/textlayer.rs`).
3. ~~**FOL-L031 — values that cannot be drawn.**~~ Done: the refusal of 0.9.0 goes: a value the
   embedded font cannot draw (Arabic has no letters in it) stays as typed, Folitecta warns and
   shows question marks, and the form asks other readers to draw its fields
   (`/NeedAppearances`).

### 0.12.0 — nothing left behind (released 1 November 2026)

What a command promises, it does whole (FOL-L016, FOL-L010, FOL-L015).

1. ~~**FOL-L016 — Remove Cropped Content removes it.**~~ Done: the command only clipped, so the text
   stayed in the file; it now takes it out as redaction does, asks first, and stays one undo step.
2. ~~**FOL-L010 — templates exchanged.**~~ Done: import of a template or of any PDF file, export
   of a template, from the gallery.
3. ~~**FOL-L015 — attached files of an inserted document.**~~ Done: they come with all its pages,
   renamed when their name is used; taking some pages leaves them, and the window says so.

### 0.13.0 — buttons that work (released 1 November 2026)

Links and push buttons do all their document says, scripts included, in the sandbox (FOL-L034).

1. ~~**FOL-L034 — scripts of buttons and links.**~~ Done: a press runs the script in the sandbox of
   the console, with no files and no network, within its time limit; alerts and failures are
   messages, changes reach the fields and pages (`follow::run_actions`).
2. ~~**FOL-L034 — chains of actions.**~~ Done: `/Next` is followed in order, a chain that comes
   back on itself ends (`Document::button_actions`, `link_actions`).

### 0.14.0 — search that finds more (released 2 November 2026)

Search by pattern, and results that leave the program (FOL-L003).

1. ~~**FOL-L003 — regular expressions.**~~ Done: an option of the search pane, with case and whole
   words; an invalid expression is explained; the library keeps to a time in proportion to the
   text, whatever the pattern.
2. ~~**FOL-L003 — ready patterns.**~~ Done: e-mail addresses, phone numbers, dates, web addresses
   and bank accounts (`SEARCH_PATTERNS`).
3. ~~**FOL-L003 — results exported.**~~ Done: CSV of page, match and context; cells a spreadsheet
   would run as formulas are written as text (`search_hits_csv`).

### 0.15.0 — what the file forbids (released 2 November 2026)

Files that forbid copying keep their text and pictures, and the protection stays (FOL-L036).

1. ~~**FOL-L036 — exports under the copy permission.**~~ Done: pictures of pages, text, embedded
   pictures, Office documents and search results are refused with the password to open, as copying
   is, and the owner password asked; pages taken out into a tab are restricted like the document;
   the text of the reflow view cannot be selected. Reading aloud stays.
2. ~~**FOL-L036 — the protection itself.**~~ Done: Save Unrestricted Copy and the Security window
   need the owner password as soon as the settings forbid anything, printing included
   (`Restricted::Protection`).

### 0.16.0 — what the file holds (released 2 November 2026)

What is in a file, in sight or not (FOL-L037, FOL-L004).

1. ~~**FOL-L037 — hidden content of the pages without marks.**~~ Done: Apply Redactions offers to take
   out what lies beyond the crop box of the other pages too, chosen by default; what shows there does
   not change. Not when pages are drawn as pictures.
2. ~~**FOL-L004 — Fonts pane.**~~ Done: the fonts of pages, forms, patterns and comments, with their
   kind, whether they are embedded and their pages; those other computers will replace are said.

### 0.17.0 — lists that leave the program (released 2 November 2026)

What a pane lists can be taken away (FOL-L003, FOL-L026, FOL-L002).

1. ~~**FOL-L003 — all the results in the export.**~~ Done: the CSV of a search has what was found in
   comments, bookmarks, fields, links, attachments and document information too, each with its place.
2. ~~**FOL-L026 — comments as CSV.**~~ Done: Export to CSV in the menu of the Comments pane: page,
   type, author, subject, date, reply and contents (`util::csv_table`).
3. ~~**FOL-L002 — files of a session that are gone.**~~ Done: named one by one, not only counted.

### 0.18.0 — forms that say so (released 2 November 2026)

Forms tell what they could not take (FOL-L032, FOL-L031).

1. ~~**FOL-L032 — values from a file are checked.**~~ Done: imported values and copies filled from a
   table go through the formats and checks of their fields; those refused stay out and are named,
   where they used to enter as they were.
2. ~~**FOL-L031 — values too long.**~~ Done: the value is kept and said, typed or imported
   (`field_overflows`).

### 0.19.0 — boxes and reading (released 2 November 2026)

Pages for the printer, reading from where one looks (FOL-L016, FOL-L042).

1. ~~**FOL-L016 — page boxes.**~~ Done: Page Boxes… lists the sheet, visible page, bleed, trim and art
   boxes of a page and sets or removes the last three on a range of pages; Show All Boxes draws them.
2. ~~**FOL-L042 — reading from a paragraph.**~~ Done: a double click in the reflow view reads from
   there, also while reading.

### 0.20.0 — cleaner scans (released 2 November 2026)

What a scanner leaves, and how sure a reading must be (FOL-L012, FOL-L014).

1. ~~**FOL-L012 — dark borders.**~~ Done: Clean Up Scans finds the dark edges of a scan, told from
   lines of text and from pages dark all over, and paints them the colour of the paper; a page can
   be whitened and straightened in the same step.
2. ~~**FOL-L014 — the threshold of doubt.**~~ Done: chosen in the recognition window, as a percentage.

### 0.21.0 — fonts for PDF/A (released 2 November 2026)

What PDF/A needs most: every font in the file (FOL-L043).

1. ~~**FOL-L043 — simple fonts embedded.**~~ Done: a Type 1 or TrueType font that is not embedded,
   the standard Helvetica, Times and Courier included, becomes a subset of FreeSans, FreeSerif or
   FreeMono of the same style, with the widths the text is laid out with (those of the Adobe
   metrics for the standard fonts), its encoding kept or written as WinAnsi with differences.
2. ~~**FOL-L043 — what an independent validator finds.**~~ Done: veraPDF 1.30.3 on seventeen kinds
   of document found what the preflight missed — text painted in the initial black (DeviceGray),
   fonts written inside resources, comments without appearances, the actions and single
   appearances of buttons — all now checked and fixed; every copy declared PDF/A-2b passes it.
### 0.22.0 — scans and recognition in bulk (released in 0.23.0, 3 November 2026)

Scanning where the document is, and text recognition on many files (FOL-L011, FOL-L013, FOL-L047).

1. ~~**FOL-L011 — scanning into the open document.**~~ Done: a choice of the scanning window puts
   the pages after the current page instead of a new tab, backs and clean-up as before.
2. ~~**FOL-L011 — stopping a scan.**~~ Done: Stop ends `scanimage`; the sheets the feeder gave
   before stay.
3. ~~**FOL-L013, FOL-L047 — recognition as a step.**~~ Done: action sequences have a Recognize Text
   step (pages that already have text left out unless asked), so folders of files and the command
   line (`--run-actions`) get their text read; one engine shared by the core and the window.

### 0.23.0 — choices of one's own (released 3 November 2026)

Tables and templates as the user wants them (FOL-L005, FOL-L010).

1. ~~**FOL-L005 — Table Options.**~~ Done: a table made from a CSV file is made again in its tab with
   the separator, the first row (header or data), the paper, the turn of the pages and the size of
   the text chosen (`CsvOptions`, `from_csv_with`, `csv_shape`).
2. ~~**FOL-L010 — groups and search of templates.**~~ Done: a template of the user is put in a group
   of the gallery of its own (kept in its file), and a search box finds templates by name, group and
   file of origin.

### 0.24.0 — labels and bookmarks in order (released in 0.43.0, 4 November 2026)

Numbers that stay right when pages come and go, bookmarks without repeats (FOL-L015, FOL-L022).

1. ~~**FOL-L015 — page labels follow the pages.**~~ Done: new pages join the range of labels where
   they land and the rest of the range moves on; moved pages keep their labels
   (`labels_in_order`, in `set_page_order` and in the pages taken from another file).
2. ~~**FOL-L022 — page labels in the contents.**~~ Done: the entries of an inserted table of
   contents carry the labels the pages have once it is in.
3. ~~**FOL-L022 — duplicate cleanup.**~~ Done: Delete Duplicates in the Bookmarks pane deletes the
   bookmarks that repeat one before them (same title, same page and view within a point, or the
   same other action), their children kept.

### 0.25.0 — labels in every page selector (released in 0.43.0, 4 November 2026)

Pages chosen by the numbers printed on them (FOL-L017).

1. ~~**FOL-L017 — logical page labels in every page selector.**~~ Done: the page windows, the page
   box, the mail window and the pages of the other file in Replace Pages read page labels as Print
   did (`parse_pages_labelled`, `range_labels`), behind the preference Use Page Labels.

### 0.26.0 — scans the right way up (released in 0.43.0, 4 November 2026)

Sheets fed the wrong way round (FOL-L012).

1. ~~**FOL-L012 — pages upside down or sideways.**~~ Done: Clean Up Scans tells the direction of the
   lines in strips of the page and, along them, the ink above and below the core of each line
   (`imaging::upright_turn`); the page is turned by its rotation, counting the placement of the
   picture and the rotation it has (`ScanFix::Turn`, `ScansCleaned`).

### 0.27.0 — a history of changes (released in 0.43.0, 4 November 2026)

Undo steps with names, and a way back to any of them (FOL-L004).

1. ~~**FOL-L004 — history pane of changes.**~~ Done: every undo step is numbered as made and named
   after the command under way (until its window closes) or the tool that drew it
   (`Document::step_count`, `name_new_step`, `undo_steps`, `redo_steps`, named in
   `AppState::with_active`); the History of Changes pane lists them and goes back or forward to
   any with a click.

### 0.28.0 — tables that read right (released in 0.43.0, 4 November 2026)

Tables of the tags that a screen reader can follow (FOL-L041).

1. ~~**FOL-L041 — table headers.**~~ Done: Fix of the rule Table headers makes the cells of the
   first row of each table without headers `TH` with the scope of their column.
2. ~~**FOL-L041 — table spans.**~~ Done: Fix of the rule Regularity gives the cells of the shorter
   rows `/ColSpan`: the columns their text covers, measured against the widest row, or the last
   cell the rest when the page does not tell.

### 0.29.0 — words read with doubt, found (released in 0.43.0, 4 November 2026)

The doubtful words of text recognition where words are looked for (FOL-L014).

1. ~~**FOL-L014 — doubtful words in the spell check.**~~ Done: listed with the words the dictionaries
   do not know (`SpellScope::doubtful`, `Misspelling::doubtful`), the word found again among the
   doubtful words when it is checked (`keep_doubtful_word`, `correct_spelling` through
   `review_ocr_word`).
2. ~~**FOL-L014 — doubtful words in the search.**~~ Done: Only Words Read with Doubt keeps the
   results on them (`hits_on_doubtful`).

### 0.30.0 — measures and marks kept (released in 0.43.0, 4 November 2026)

What a measurement and a tick mark were set up with, changed or kept (FOL-L028, FOL-L029).

1. ~~**FOL-L028 — the depth of a volume.**~~ Done: in the Properties pane of a selected volume
   measurement (`volume_depth`, `set_volume_depth`), its label drawn again.
2. ~~**FOL-L029 — saved sets of tick styles.**~~ Done: named sets of shape, sequence, size and colour
   in the tick marks window (`tick.sets`).

### 0.31.0 — presentation (released in 0.43.0, 4 November 2026)

Documents shown as slides (FOL-L001).

1. ~~**FOL-L001 — presentation mode with transitions.**~~ Done: View › Presentation shows the
   page pictures fitted to the screen, with the transitions of Page Transitions played as
   animations and the display time of each page (`page_display_time`) moving on by itself.

### 0.32.0 — tabs dragged (released in 0.43.0, 4 November 2026)

Tabs arranged with the mouse (FOL-L002).

1. ~~**FOL-L002 — tabs dragged between groups.**~~ Done: a tab dropped on another goes before it
   (`reorder_tab`, the groups keeping the order of the tabs); dropped on the strip of the other
   group, it moves there (`move_tab_to_group`).

### 0.33.0 — table summaries (released in 0.43.0, 4 November 2026)

What a table holds, said before it is read (FOL-L041).

1. ~~**FOL-L041 — a summary asked for each table.**~~ Done: the rule Table summary is fixed by
   asking (`Fix::Ask`): Table Summaries shows every table with its picture
   (`table_summaries`, `set_table_summaries`; an empty summary taken away).

### 0.34.0 — articles (released in 0.43.0, 4 November 2026)

Stories that run across columns and pages (FOL-L022).

1. ~~**FOL-L022 — article threads read.**~~ Done: the Articles pane lists the threads (`articles`,
   their beads followed round to the first) and shows the area of a bead with a click;
   `add_article` makes a thread in one undo step.

### 0.35.0 — articles that come along (released in 0.43.0, 4 November 2026)

The threads of the pages taken from another file (FOL-L015).

1. ~~**FOL-L015 — article threads of an inserted file.**~~ Done: the threads of the source join the
   `/Threads` of the document with the pages they run on.

### 0.36.0 — the Article tool (released in 0.43.0, 4 November 2026)

Article threads drawn on the pages (FOL-L022).

1. ~~**FOL-L022 — an article tool.**~~ Done: `Tool::Article`, its parts kept until Enter
   (`commands::articles`, `Dialog::ArticleTitle`) and made with `add_article`.

### 0.37.0 — articles seen on the page (released in 0.43.0, 4 November 2026)

Where the parts of an article are (FOL-L022).

1. ~~**FOL-L022 — parts of articles shown.**~~ Done: boxes numbered by thread and part
   (`View::article_parts` of the page), dashed for the parts being drawn.

### 0.38.0 — articles edited (released in 0.43.0, 4 November 2026)

Threads changed after they are made (FOL-L022).

1. ~~**FOL-L022 — threads edited.**~~ Done: `rename_article`, `delete_article`,
   `delete_article_part` (the beads around joined, the first replaced, the beads taken off the
   lists of their pages), with their buttons in the Articles pane.

### 0.39.0 — articles rearranged (released in 0.43.0, 4 November 2026)

Parts added to a thread and moved in it (FOL-L022).

1. ~~**FOL-L022 — parts added and moved.**~~ Done: `add_article_parts` (the Article tool drawing for
   a chosen article, `TARGET`), `move_article_part`; the beads linked round again (`link_beads`).

### 0.40.0 — chosen flattening, reading where clicked (released in 0.43.0, 4 November 2026)

What is merged into the page, and where reading begins (FOL-L016, FOL-L042).

1. ~~**FOL-L016 — selective flattening.**~~ Done: `comment_kinds_and_authors`,
   `flatten_comments_of` (`flatten_where` sees each dictionary), the kinds and authors of the
   Flatten Comments window.
2. ~~**FOL-L042 — reading from a paragraph clicked on the page.**~~ Done: a double click with the
   reading bar open reads from the paragraph under the pointer.

### 0.41.0 — scans cleaner still (released in 0.43.0, 4 November 2026)

Specks and black and white in the clean-up of scans (FOL-L012).

1. ~~**FOL-L012 — despeckle and black and white in the same workflow.**~~ Done: `ScanThresholds`
   `despeckle` and `black_white`, `ScanFix::Despeckle` and `ScanFix::BlackWhite` for every kept
   scan (black and white last), counted in `ScansCleaned`; two boxes of the window.

### 0.42.0 — fonts that are used (released in 0.43.0, 4 November 2026)

The Fonts pane without the fonts nothing draws with (FOL-L004).

1. ~~**FOL-L004 — fonts listed but not used.**~~ Done: the content that owns a resource dictionary
   is read for the names it uses (`Tf`, `gs`, `Do`); what it does not name does not count, and
   everything counts when the content cannot be read.

### 0.43.0 — fronts and backs (released 4 November 2026)

Two stacks of scans put in order after scanning (FOL-L012).

1. ~~**FOL-L012 — fronts and backs interleaved.**~~ Done: Interleave Fronts and Backs
   (`document.interleavePages`) with the number of fronts and the order of the backs, through
   `interleave_backs`.

### 0.44.0 — words marked as they are read (released in 0.73.0, 9 November 2026)

Following the voice word by word (FOL-L042).

1. ~~**FOL-L042 — words marked as they are read.**~~ Done: the engines tell where each word begins
   (`Spoken::Word`: the `boundary` events of the web view's speech, the index marks of
   speech-dispatcher with a mark before every word in SSML), the word found on the page through
   the characters the read text comes from (`read_word`, `joined_map`) and marked.

### 0.45.0 — pictures in the office exports (released in 0.73.0, 9 November 2026)

Word, rich text and web pages with the pictures of the pages (FOL-L009).

1. ~~**FOL-L009 — pictures exported.**~~ Done: the pictures of each page go among its paragraphs by
   their place (before the first paragraph below their top), at their size on the page (no wider
   than the text): inline drawings with `word/media` in DOCX, `\pngblip` and `\jpegblip` in RTF,
   data addresses in HTML; JPEG data kept as it is, other pictures as PNG, those turned or
   mirrored drawn from the page; specks and the scan of a page with text left out and counted.

### 0.46.0 — tables out of the pages (released in 0.73.0, 9 November 2026)

The tables of the pages taken out as spreadsheets (FOL-L009, FOL-L030).

1. ~~**FOL-L030, FOL-L009 — tables found and exported.**~~ Done: `page_tables` finds rows by the
   lines glyphs share across the page (not by the order they are drawn in), cells by gaps wider
   than a word space and columns by the gaps the rows leave free one under the other (a row
   wrapped in a cell joined again, running text in columns refused); `export_tables` writes a
   workbook (`sheets.rs`: inline strings, numbers and percentages with their formats, a bold head)
   or CSV; Export Tables lists the tables found to choose.

### 0.47.0 — tables in the office exports (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L009 — tables as tables.**~~ Done: the tables `find_tables` finds on a page go into the
   DOCX (`w:tbl` with borders, a repeated head row, numbers right), RTF (`\trowd`, `\trhdr`) and
   HTML (`table`, `th`, `td.n`) exports in place of the paragraphs inside them, before the first
   paragraph below their top; `OfficeExport::tables` in the report.

### 0.48.0 — picture fields (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L031 — picture fields.**~~ Done: `NewField::Picture` (a push button showing its picture
   alone, `/TP 1`, whose script is `event.target.buttonImportIcon();` as other programs write it),
   `is_picture_field`; `buttonImportIcon()` of the script API asks the program for a picture;
   button scripts know their button as `event.target`; the Picture Field tool
   (`tool.editFields.mode.image`).

### 0.49.0 — repeated fonts and pictures kept once (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L045 — fonts and other objects.**~~ Done: `merge_duplicates` (`duplicates.rs`): pictures,
   fonts with their descriptors, programs, maps and widths, and colour profiles that are the same
   object for object become one, in rounds until fonts match once their parts do; then what
   nothing uses is removed. Pages, comments, fields and content streams are never merged; signed
   documents are refused; one undo step, none when there is nothing to merge.

### 0.50.0 — form fields found (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L014 — field recognition proposals.**~~ Done: `find_form_fields` (`field_finder.rs`):
   underscores in the text, lines drawn with nothing written above them, empty rectangles and
   small squares (`PagePath::rectangles`: each `re`, or four corners joined), named by the words
   before, after or above them, none where a widget is; `add_found_fields` in one undo step with
   names made unique; Find Form Fields (`forms.findFields`) lists them to choose.

### 0.51.0 — pictures in Markdown (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L005 — Markdown pictures.**~~ Done: `Block::Picture` for a paragraph that is only a
   picture, `from_markdown_in` with the folder of the file, `Typeset::picture` (placed on the
   page, drawn when the document is made); the others shown by their description.

### 0.52.0 — hidden information removed, more steps (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L037 — sanitizing on its own.**~~ Done: `Document::sanitize` (metadata, attachments,
   hidden content, scripts; one undo step; signed documents refused) and Remove Hidden
   Information (`document.sanitize`) with the preview of each part.
2. ~~**FOL-L047 — more steps.**~~ Done: `Step::Sanitize`, `Step::MergeDuplicates`,
   `Step::FindFormFields`, with their settings in the Action Sequences window.

### 0.53.0 — pictures compared (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L035 — picture differences.**~~ Done: `picture_differences` pairs the pictures of each
   old page with those of the new pages beside it (`page_map`) by a digest of their data and
   size: moved, replaced where one stood, added, removed; `Difference::picture`, listed and marked
   with the text differences.

### 0.54.0 — object inspector (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L004 — object inspector.**~~ Done: `Document::inspect` (`inspect.rs`) at an
   `InspectPath` (the trailer or an object, then keys and indices): entries with their kind, a
   summary and where they lead, the data of streams (text, or hexadecimal); the Object Inspector
   window (`document.inspectObjects`).

### 0.55.0 — recognition that leaves out what is there (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L013 — exclusions.**~~ Done: `ocr_exclusions` (the letters of the text of the page,
   without the earlier recognition when it is replaced, and the rectangles of comments and fields)
   and `without_places` before the text is written; nothing new: no change, no undo step.

### 0.56.0 — settings that travel (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L050 — portable settings.**~~ Done: `portable.rs`: `--settings <folder>`, or a
   `portable` marker beside the program for `settings` beside it, through
   `FOLITECTA_CONFIG_DIR` before anything starts; the folder in About.

### 0.57.0 — nodes added and removed (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L021 — nodes added and removed.**~~ Done: `add_path_node` (a straight segment after a
   node), `remove_path_node` (its segment goes; the first node hands the start to the next line;
   rectangles refused; two nodes kept); double clicks in the shape editor.

### 0.58.0 — sites opened without asking (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L052 — the answer to a link kept.**~~ Done: `web_site`, `allowed_sites`,
   `allow_site`, `forget_sites`, `needs_asking` (`follow.rs`), the choice in the question of a web
   link, the list in Preferences.

### 0.59.0 — combs found (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L014 — combs.**~~ Done: `combs` (`field_finder.rs`): squares alike and side by side
   (at most 2 points apart), three or more, become one comb text field (`FieldProposal::comb`,
   `max_len`).

### 0.60.0 — the history of a comment (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L026 — history of a comment.**~~ Done: `comment_history` (made, replies, states,
   last change; `HistoryEntry`, `HistoryEvent`), shown in the Review History window.

### 0.61.0 — measurements pane (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L028 — measure info pane.**~~ Done: `measurement_list` (`Measurement`),
   `measure_parts` and `measure_totals`; the Measurements pane (`view.measurements`).

### 0.62.0 — cells over several rows (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L041 — row spans.**~~ Done: `fix_table_spans` keeps the cell of each column row after
   row; the columns a short row leaves before its first cell go to the cells above (`/RowSpan`).

### 0.64.0 — sideways sheets fed askew (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L012 — sideways pages straightened when askew.**~~ Done: `way_and_skew` (`scans.rs`):
   lines running down measured on the picture turned a quarter; when the way up cannot be told, it
   is told on a copy straightened by the skew, as it lies and turned a quarter.

### 0.65.0 — chosen pages to grey (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L044 — selective conversion.**~~ Done: `convert_pages_to_grey` and
   `estimate_pages_grey` (what only those pages reach changes; shared objects counted as kept);
   the pages of the Convert Colours to Grey window.

### 0.66.0 — snapping to what the page draws (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L028 — snapping profile.**~~ Done: `snap_points` (path nodes, rectangle corners) and the
   planned `view.snap.toObjects`, before the grid and guides, kept per tab, page and revision.

### 0.67.0 — office exports checked on a whole report (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L009, FOL-L005 — pictures and tables with the text.**~~ Done: a report made from
   Markdown with pictures, two tables over two pages and a list, exported to DOCX and HTML and
   read back: pictures of Markdown placed from the bottom as `add_image` measures (they were
   mirrored), list items started as paragraphs of their own (`list_item`).

### 0.68.0 — lists in the office exports (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L009 — lists as lists.**~~ Done: `list_mark`; `ul` and `ol` in HTML, items with a
   hanging indent in DOCX (`w:ind`, a tab after the mark) and RTF (`\li360\fi-360`).

### 0.69.0 — table columns by their contents (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L009 — column widths.**~~ Done: `table_widths` (the longest cell of each column, up to
   40 characters; scaled down to the text width) in DOCX and RTF.

### 0.70.0 — form values in the office exports (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L009 — filled forms exported.**~~ Done: the export reads a copy whose fields are
   flattened; the text inside the fields' areas goes by its place on the page.

### 0.71.0 — tables of filled forms (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L030 — field values in found tables.**~~ Done: `page_tables` reads a copy whose form
   fields are flattened (`tables_of_text`).

### 0.72.0 — wrapped cells of the last row (released in 0.73.0, 9 November 2026)

1. ~~**FOL-L030 — bug.**~~ Fixed: `table_of` takes the rows of one cell after the last row of two
   cells or more, close under it and not in the first column, as words wrapped in its cells.

### 0.73.0 — pages as a presentation (released 9 November 2026)

1. ~~**FOL-L009 — PPTX.**~~ Done: `export_slides` (`slides.rs`): presentation, master, layout,
   theme and a slide for each page with its picture at 144 dpi, centred and as large as the slide
   (the size of the first page) holds, the page's text as its description; `OfficeFormat::Pptx`.

### 0.74.0 — ballot marks in the office exports (on `main`, untagged)

1. ~~**FOL-L009 — check boxes and radio buttons.**~~ Done: the export reads them from their state,
   not from their appearance (hidden in the flattened copy): an invisible mark that reads ☑ or ☐
   is written just before the words after the box on its line, or where the box stands, and a
   block that is only a mark goes before its label (`join_ballots`); in tables the mark is the
   cell.

### 0.75.0 — lists within lists (on `main`, untagged)

1. ~~**FOL-L009 — nested lists.**~~ Done: each text piece keeps where its first line starts;
   `list_depths` gives an item further in than the one above it one level more and one back at
   the place of an outer item that level again; the web page nests `<ul>`/`<ol>` in the item above,
   Word and RTF indent each level by a quarter inch more.

### 0.76.0 — Word lists (on `main`, untagged)

1. ~~**FOL-L009 — lists that Word numbers.**~~ Done: list items in DOCX are paragraphs of Word lists
   (`w:numPr` with their level) defined in `word/numbering.xml` (`word_lists`): each list at one
   level under one item is a list of its own, with the format of its first item (bullet with its
   mark, decimal, lower or upper letter, `.` or `)`) and the number it starts from.

### 0.77.0 — notes of the slides (on `main`, untagged)

1. ~~**FOL-L009 — notes in PPTX.**~~ Done: a notes master (with its theme) and a notes page for
   each slide, the paragraphs of its page as the notes (up to 20 000 characters).

### 0.78.0 — nodes made smooth, corner or straight (on `main`, untagged)

1. ~~**FOL-L021 — Convert and Anchors › Add.**~~ Done: `convert_path_node` (`NodeKind`: straight
   lines on both sides, a sharp corner with the node's own handles drawn into it, or a smooth node
   with its handles on one line through it, straight sides made curves a third of their length
   out); a click on a node of the chosen path chooses it, and the bar over the path or Edit ›
   Path Editing converts it or adds a node halfway to the next one.

### 0.79.0 — shapes through three points (on `main`, untagged)

1. ~~**FOL-L021 — 3 Point › Arc, Circle, Rect.**~~ Done: `add_three_point_shape` (`ThreePoint`):
   the circle through three points (four quarter curves), the arc from the first through the
   second to the third (the way that passes the second, in quarter-turn curves) and the rectangle
   with one side from the first to the second as high as the third is from it, stroked with the
   colour and width of the Line tool, in the page content; a tool finished by its third click.

### 0.80.0 — snapping to middles and crossings, fill rule (on `main`, untagged)

1. ~~**FOL-L028 — snapping to the middles of lines and to intersections.**~~ Done: `snap_points`
   adds the middle of every straight segment (lines, the edges that close subpaths, the sides of
   rectangles) and where two of them cross (the first 400 segments of a page).
2. ~~**FOL-L021 — Fill Rule.**~~ Done: `set_path_fill_rule` and `PagePath::even_odd`; Fill Rule
   fills the chosen path by the other rule and says which.

### 0.81.0 — every drawing of a picture replaced (on `main`, untagged)

1. ~~**FOL-L020 — Replace All Image Instances.**~~ Done: the picture chosen replaced with one
   from a file wherever the document draws it, the number of drawings said
   (`image_drawings`, `replace_image_everywhere`).

### 0.82.0 — ends, corners, blending and fill opacity of comments (on `main`, untagged)

1. ~~**FOL-L026 — Line Cap, Line Join, Blend Mode, Stroke Opacity.**~~ Done: `AnnotStyle` gains
   `line_cap`, `line_join` (Folitecta's entries, drawn as `J`/`j` in the appearance), `blend`
   (`/BM`, twelve modes; highlights still multiply when none is chosen) and `fill_opacity`
   (`/ca`, apart from `/CA` of the lines); the style editor has a row for each, kept in named
   styles too, and the four commands open it at their row.

### 0.83.0 — wavy underlines, link highlights, format rows (on `main`, untagged)

1. ~~**FOL-L026 — Underline Style, Highlight Mode, Text Fill Color, Border Width Text.**~~ Done:
   `set_underline_wavy` (an underline made `Squiggly` or back, its appearance drawn again),
   `link_highlight`/`set_link_highlight` (`/H` of a link: nothing, invert, outline, push); an
   Underline row in the style editor and an On click row for a chosen link in the properties
   pane; the four commands open the pane at their row (text colour and thickness included).

### 0.84.0 — pages selected by shape and by conditions (on `main`, untagged)

1. ~~**FOL-L004 — Select › Shape, Advanced Selection.**~~ Done: the pages that draw shapes; a
   dialog choosing pages odd or even, within a range, upright or lying, holding text, pictures,
   shapes, comments, links or fields (all of them or one), or nothing, added to the selection or
   in place of it (`commands/navigation/page_select.rs`).

### 0.85.0 — pages copied and pasted (on `main`, untagged)

1. ~~**FOL-L050 — Paste Pages, Paste Pages Special.**~~ Done: Copy with pages chosen in the
   thumbnails keeps them as a document of their own (`Clip::Pages`); Paste Pages puts them after
   the current page (a picture of the clipboard when no pages were copied), Paste Pages Special
   before or after it, at the start or the end, up to 100 times; one undo step
   (`insert_document_copies`). Plain Paste leaves copied pages alone.

### 0.86.0 — pictures pasted as stamps, loupe snapshots (on `main`, untagged)

1. ~~**FOL-L027, FOL-L001 — Paste as Stamp, Loupe › Snapshot.**~~ Done: the picture of the
   clipboard as a movable stamp in the middle of the current page (96 to the inch, at most 60% of
   the page); what the loupe shows, or showed last, copied as a picture as enlarged as it shows it.

### 0.87.0 — scanned pages inserted, the window's view copied (on `main`, untagged)

1. ~~**FOL-L011, FOL-L001 — Insert Scanned Pages, Pan and Zoom › Snapshot.**~~ Done: the scanning
   window opened with its pages going into the open document after the current page; the part of
   the current page the window shows copied as a picture.

### 0.88.0 — fonts replaced (on `main`, untagged)

1. ~~**FOL-L019 — Replace Fonts.**~~ Done: `replace_font` draws every simple font of a name with
   a subset of a GNU FreeFont face chosen (sans, serif, mono; bold, italic), embedded, with the
   widths the text is laid out with, the fonts written inside page resources lifted first; one
   undo step; composite, Type 3 and symbol fonts and codes of their own refused.

### 0.89.0 — problem details (on `main`, untagged)

1. ~~**FOL-L004 — Show Problem Details.**~~ Done: `Document::problems` (`organize/problems.rs`)
   lists references to objects the file does not hold, page contents that cannot be read, fonts
   the text uses that the page does not name, comments without an area or a kind, and a page
   count that disagrees with the pages found, each with its page; a window shows them.

### 0.90.0 — letters of text boxes outlined (on `main`, untagged)

1. ~~**FOL-L026 — Text Stroke Color.**~~ Done: `AnnotStyle::text_stroke`
   (`/FolitectaTextStroke`), the letters of text boxes filled and outlined (`2 Tr`) in that
   colour, a 25th of the text size wide; a Letter outline row in the style editor for text boxes,
   typewriter text and callouts.

### 0.91.0 — document actions cleared (on `main`, untagged)

1. ~~**FOL-L034 — Document Actions, Document Actions › Clear.**~~ Done: the menu's Document
   Actions opens the editor of the document's event scripts; Clear removes all of them after
   asking, in one undo step (`clear_document_actions`); scripts of pages, links and fields stay.

### 0.92.0 — the actions of a link (on `main`, untagged)

1. ~~**FOL-L022 — Actions › Add, Add from Bookmark, Edit, Up, Down, Remove.**~~ Done: the
   actions of the chosen link listed in the properties pane, a row chosen with a click; added or
   edited in a window (a page, a web address, a script, a page turn of the reader, the page of a
   bookmark), moved and removed, the actions as written kept (named destinations, form
   submissions and other actions are not rewritten); one undo step each, one action always stays.

### 0.93.0 — zoom controls reached from the menus (on `main`, untagged)

1. ~~**FOL-L001 — Zoom Slider, Zoom Tools, Edit › Position.**~~ Done: View › Zoom Slider puts
   the keyboard on the zoom slider of the status bar; Tools › Zoom Tools and the ribbon's group
   choose the zoom tool; Edit › Position reaches the position slider of the media player.

### 0.94.0 — the commands of a group (on `main`, untagged)

1. ~~**FOL-L050 — group commands.**~~ Done: the 21 commands named after a group of commands open
   a window listing the commands of the group that run by themselves (`commands/window/groups.rs`,
   `Dialog::CommandGroup`); File and Form open their menus, No Properties the properties pane.

### 0.95.0 — search, loupe and pane options (on `main`, untagged)

1. ~~**FOL-L003, L001, L004 — search, loupe and pane options.**~~ Done: Annotate Result, the
   search toggles shared with Find, Stop; the boxes and guides in the loupe and its enlargement;
   Interactive Visible of the thumbnails; the options windows of the thumbnails, links and
   recovery (`Dialog::PaneOptions`), recovery copies every 1–15 minutes or none.

### 0.96.0 — Bates numbering of files listed, page mark previews (on `main`, untagged)

1. ~~**FOL-L017 — Bates numbering of files listed, previews of page marks.**~~ Done: the
   list of files (`commands/document/bates_files.rs`, `Dialog::BatesFiles`), Edit › List and
   the eight columns; the preview of the page mark windows (`ui/features/mark_preview.rs`,
   `/markprev/`) with Page Marks › Zoom In and Zoom Out.

### 0.97.0 — presets of one's own, e-mail methods (on `main`, untagged)

1. ~~**FOL-L050, L045, L011, L028, L040, L053 — presets and e-mail methods.**~~ Done: the
   Presets window (`commands/document/presets.rs`, `ui/features/presets.rs`) for four kinds,
   with Edit › Presets, Measures › Manage, Configure Scan Presets, Recompress Images › Edit,
   Digital Signatures and Measurement Options; Email Methods › Manage.

### 0.98.0 — toolbars and tabs of one's own, locks (on `main`, untagged)

1. ~~**FOL-L050 — toolbars and tabs of one's own, locks.**~~ Done: `qat::UserBar` (toolbar or
   ribbon tab, shown, entries), Customize Toolbars edits the Quick Access Toolbar or one of them;
   the ribbon draws the tabs of one's own, the classic interface the toolbars; Make Default;
   View › Lock/Unlock All Toolbars and Panes (`layout::locked`); Reset Menu.

### 0.99.0 — guides, fields and files through the clipboard (on `main`, untagged)

1. ~~**FOL-L050, L031, L023 — guides, fields and files through the clipboard.**~~ Done:
   `Document::field_copy`/`paste_fields` and `paste_files` (core), `clipboard::set_files`/
   `get_files` on the three systems, `commands/window/clip_more.rs` (Guides Select All / on
   Page, Paste Guides, Paste Fields, Paste Files, Copy File); Show Both and Remove Cropped Content
   no longer partial.

### 0.100.0 — comment labels, notes on text, rows of data (on `main`, untagged)

1. ~~**FOL-L026, L017, L028 — comment labels, notes on text, rows of data.**~~ Done:
   `set_annotation_subject`, `marks::RowData`, `set_row_data`, `set_region` (core);
   `commands/markup/comment_more.rs`, `CommentLabel`, `StyleList`, `RowData` windows; Region no
   longer partial.

### Planned releases after 0.73.0 (prepared 9 November 2026)

What is still open, from the requirements below and from the commands of
[docs/FEATURES.md](../docs/FEATURES.md) that are not available yet (276 planned and 12 partial
commands once submenus and the same command in menus, ribbon and panes are counted once; about 45
of them wait for an engine or a remote service decided below). 0.74.0 to 0.77.0 (office export
fidelity) are done and go out with the first of these releases.

1. **0.80.0 — editing that goes further** (FOL-L019, L020, L021, L028): path nodes made smooth,
   corner or straight, three-point arcs, circles and rectangles, clipping removed, fill rule; text
   reflowed inside its area, blocks joined and split; Edit Object and object groups; every use of
   a picture replaced at once; snapping to the middles of lines and to intersections; the content
   filters (text, pictures, shapes) of Edit Content.
2. **0.85.0 — clipboard, presets and the interface of one's own** (FOL-L050, L002, L004): Paste
   Pages and Paste Pages Special, fields, guides, files, rich text and as a stamp, Copy File; the
   presets managers (add, edit, clone, order, import, export) and Measurement Options; toolbars
   and ribbon tabs of one's own (new, rename, make default, lock and unlock); tabs moved between
   windows, all tabs gathered into one window, tabs distributed; thumbnails selected by shape and
   by advanced selection; the snapshot of the loupe and of pan and zoom.
3. **0.90.0 — actions and comments complete** (FOL-L022, L026, L034, L047): the editors of
   actions (go-to views, action lists of bookmarks, links, pages and layers, document actions),
   Batch Link and Web Links, the links pane's import and options; comment labels, line caps and
   joins, blend modes, stroke opacity, text fill and stroke colours, underline style, highlight
   mode, the styles list of the tools, editing a measurement and copying its information, the row
   data of comment macros; keystroke scripts while typing and page and document events.
4. **0.95.0 — creation, conversion and standards** (FOL-L005, L011, L043, L044, L045, L041,
   L035, L009): new documents from rich text with its styles and from a selection, Insert Scanned
   Pages, Replace Fonts, Show Problem Details, Bates numbering of files with its columns; PDF/A
   with composite fonts without a ToUnicode map, PDF/A-1 and -3 and a PDF/UA check; colour
   profiles and conversion to CMYK, separations preview; font subsets merged; the manual checks of
   the accessibility report; drawings compared; slides with editable text.

**To be decided by the user before they are planned** (engines or the network, against the
principle that Folitecta connects to nothing it is not asked to): XFA forms (FOL-L033), on-device
AI to summarize and translate (FOL-L048), 3D models (FOL-L049, about 15 commands), Office and web
page import and XPS, metafiles, CAD (FOL-L006, L008), file associations and a virtual printer
(FOL-L007), and the remote services FOL-C001–C014 (WebDAV, e-signature services, opening from a
web address, stamps and search providers from the web).

**To be checked by hand** (the self-test uses stand-ins): real printers, mail programs, time
stamp and OCSP servers, smart cards and system certificate stores, HiDPI and pinch zoom, a
keyboard-only and screen reader pass, and the macOS packages of 0.73.0 once GitHub Actions run.

### Later

The rest of the fonts for PDF/A (FOL-L043: composite fonts without a ToUnicode map or with
other encodings, characters no FreeFont face has, such as Chinese), FOL-L033 (XFA engine), FOL-L048 (on-device AI), FOL-L049 (3D), FOL-L006 (office and HTML import),
FOL-L007 (virtual printer, shell integration), FOL-L008 (XPS, metafiles, CAD), remote services
FOL-C001–C005 and C007–C014 (R4).

## State of every requirement

### Reading, workspace and search

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L001 | P1 | 🟡 | hayro rendering on worker threads with one queue per document ranked by the window, pages near the view only, small previews, tiles at high zoom, pages redrawn only when they change, scrolling and Ctrl+wheel without waiting for the program; thumbnails, single/continuous/two-page layouts, right-to-left option, loupe, pan and zoom, split window, view rotation, full screen, password-protected files (decrypted in memory) | Certificate-encrypted files; a test corpus (transparency, scans, rotated pages); HiDPI and pinch zoom measurements. |
| FOL-L002 | P1 | 🟡 | document tabs, split window, sessions (save/open), recovery of unsaved documents at the next start; two tab groups (side by side or stacked, tabs moved, shared out, gathered); a saved document moved to a new window, a new empty window | Tabs dragged to other windows; all tabs of other windows gathered into one. |
| FOL-L003 | P1 | 🟡 | search pane, Find and Replace, search in folders, annotate results, regular expressions, results as CSV | Embedded or local indexes. |
| FOL-L004 | P1 | 🟡 | panes for thumbnails, bookmarks, comments, attachments, layers, fields, links, destinations, signatures, fonts, tags, reading order, content, the history of changes; properties pane; document properties and XMP | Editing objects in the inspector; names for the changes made outside commands and tools (they are called Change). |

### Creation, import, export and templates

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L005 | P1 | 🟡 | blank pages, from images, from text files, from the clipboard, combine files; Markdown (CommonMark, tables, task lists; headings as bookmarks, links, code, quotes) and CSV (separator guessed, local numbers right-aligned, header repeated, wide tables sideways) set by a small typesetter | RTF with styles; Markdown pictures on the web, CSS and formulas. |
| FOL-L006 | P2 | 🟡 | e-mail messages (EML) made into documents: header lines, text, attachments embedded | Mail archives; office formats and HTML need a local engine decision. |
| FOL-L007 | P2 | ⬜ | — | File associations (`help.fileAssoc`), virtual printer, office add-ins. |
| FOL-L008 | P2 | ⬜ | — | XPS/OXPS, metafiles, CAD: engine decision first. |
| FOL-L009 | P1/P2 | 🟡 | export to pictures (PNG, JPEG, TIFF, …), text, embedded pictures, attachments, split and extracted documents; the text as editable Word (DOCX), rich text (RTF) and web page (HTML) documents, headings and paragraphs in reading order, page breaks on request, a report of what is left out; the tables found in the text of the pages as a workbook (XLSX, numbers kept as numbers) or CSV, and as tables in the Word, rich text and web page exports; check boxes and radio buttons as ☑ or ☐ | Slides with editable text (PPTX holds pictures of the pages, the text as notes), page fidelity layouts, accessible text export. |
| FOL-L010 | P1 | 🟡 | stamps have categories and favourites; a gallery of document templates (papers, week and month planners, a to-do list, a letter, meeting notes, an invoice that adds itself up) on any paper, in the language of the interface; the user's own templates (Save as Template…, My templates in the gallery with origin, date, version and the fonts they need; new documents from them, rename, remove) | — |

### Scanning and OCR

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L011 | P1 | 🟡 | scanning into a new document: SANE (`scanimage`) on Linux with its devices, sources, colour modes and resolutions and presets of them; the WIA dialogs on Windows; pages the size of the paper; backs scanned as a second stack put between the fronts; clean-up offered afterwards | Scanning on macOS (Image Capture); stopping the system's dialog on Windows. |
| FOL-L012 | P1 | 🟡 | picture tools: turn, crop, straighten, filters, black and white; delete blank pages; Clean Up Scans: scanned pages (a picture covering 85% of the page) looked at in the background, blank sides (share of ink away from the edges), askew sheets and sheets upside down or sideways listed with thumbnails and thresholds, deleted, straightened and turned in one undo step, also sheets both sideways and askew | Pages of other scripts than Latin letters (their way up is not told). |
| FOL-L013 | P1 | 🟡 | local OCR (PaddleOCR models run by tract): pages, a selected area, pictures; invisible text tagged `OCR`, removed or replaced on demand; Latin-script languages | Editable text output; logos left out; more scripts (models). |
| FOL-L014 | P1 | 🟡 | words read with doubt (a letter below 0.8 probability) kept in the page as marked content and reviewed one by one: picture of the word, its place marked on the page, correct, keep, remove or skip, one undo step each; the review opens after recognition; the fields a flat form leaves room for proposed and added | Proposals for scans (from the recognised words and the lines of the picture). |

### Pages and composition

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L015 | P1 | ✅ | insert, delete, extract, duplicate, move, swap, reverse, replace, insert pictures, split documents, split/merge pages, combine files (a bookmark per file), find duplicate pages, delete empty pages; bookmarks, named destinations, links, fields, tags and layers follow the pages in and out: same-named fields and destinations of an inserted file get a suffix and are listed before OK, what deleting pages takes is named before OK, duplicated pages keep their comments to themselves and share their fields; page labels follow the pages, new pages joining the range where they land; the article threads of an inserted file come with it | — |
| FOL-L016 | P1/P2 | ✅ | rotate, crop, crop to white margins, resize, margins, normalize, overlay, rasterize pages | — |
| FOL-L017 | P1 | ✅ | watermarks, header and footer, Bates numbering (several files), background, page numbering, macros, Folitecta's marks tagged so they can be updated or removed; page labels read in every page range | — |

### Native editing

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L018 | P0 | 🟡 | text boxes, typewriter text, callouts, measurement labels, text stamps, designed stamps, tables, calculator tapes, page marks (watermarks, headers and footers, page and Bates numbers), comment summaries, text and e-mail documents, form fields, digital and typed signatures, media posters, redaction overlay text: an embedded subset of GNU FreeFont FreeSans with ToUnicode for text beyond WinAnsi (bold stroked), refusals named before the change and while typing; other generated text uses the standard Helvetica with WinAnsi | Arabic and shaping (no letters in the embedded font), Hebrew points; a choice of substitute font; the `/DA` font; font inspection and replacement; the other producers: OCR and barcodes. |
| FOL-L019 | P1 | 🟡 | Edit Text tool (content stream editing), insert/replace/delete page text, find and replace in page text, spell checking with corrections | Paragraph reflow inside an area, joining/splitting blocks, scope for shared form objects. |
| FOL-L020 | P1/P2 | 🟡 | pictures pane and editor: replace, crop, turn, filters, export | Inline and nested pictures, colour profile preservation report, raster editor mode. |
| FOL-L021 | P2 | 🟡 | the vector paths a page draws chosen with a click (Edit Content › Shapes), their nodes dragged, their stroke and fill colours and line width changed, deleted — in the page content, one undo step each | Paths inside reused drawings, nodes of curves made smooth or corner, clipping paths, gradients and shadings, transparency, numeric transforms of several paths. |

### Navigation, containers and embedded content

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L022 | P1 | 🟡 | bookmarks (hierarchy, editing, export), links, named destinations, web links from text and barcodes; bookmarks from the headings found by text size (levels, running heads left out); a table of contents inserted from the bookmarks (leaders, page numbers or labels, links); duplicate bookmarks deleted; article threads listed and followed | Headings found by font weight (the renderer gives no weight for Type 1 fonts, the standard ones included). |
| FOL-L023 | P1/P2 | ✅ | attachments, layers (visibility, properties, drawing and writing on the selected layer, comments and pictures moved between layers, comments of hidden layers hidden in the view, thumbnails, printing, summaries, the Comments pane and the pictures made of the pages (export, copy, snapshot), visibility expressions), audio/video with playback | Codec list and unsupported media report; layers given by membership dictionaries lost on export and paste; XFDF carries no layer; pictures inside reused drawings on layers. |
| FOL-L024 | P2 | 🟡 | PDF portfolios: create, view, extract, print | Binder project with sources and regeneration. |

### Comments and technical tools

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L025 | P1 | ✅ | every markup and drawing comment, styles and named styles, lock, grouping, align/distribute/order, eraser, clouds, the quick panel of thickness, colours and fill of the drawing and measuring tools, pen pressure for the Pencil | Pressure tried with real tablets on Windows, macOS and Linux (stand-in pen events so far); tilt of the pen; check appearances in a second reader. |
| FOL-L026 | P1 | ✅ | comments pane with filters, replies, review states, XFDF/FDF/PDF exchange, comment summaries, comments as CSV, the history of each comment | The earlier texts of a changed comment (PDF keeps none). |
| FOL-L027 | P1/P2 | 🟡 | text stamps, stamp designer with dynamic values, favourites and recent stamps | Symbol libraries with fields asked when placed. |
| FOL-L028 | P2 | ✅ | distance, perimeter, area, angle, radius and diameter, volume (area by a depth), calibration, CSV export of measures, a quick panel with the scale; the tools stay chosen; snapping to the nodes, corners, middles and crossings of what the page draws | — |
| FOL-L029 | P2 | ✅ | calculator with an editable tape placed as a comment; tick marks (check, cross, dot, numbers, letters, roman numerals, plain or circled) placed in series with a click and counted in their window, with saved sets of their styles | — |
| FOL-L030 | P1/P2 | ✅ | barcodes written (vector) and read (rxing), tables as generated objects; tables found in the text of the pages (rows, cells and columns by the gaps between them) and exported | Cells spanning several columns are put in the first; tables drawn as pictures need text recognition first. |

### Forms and scripts

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L031 | P1 | 🟡 | form designer for every field type and a date field (format of the language, a calendar while filling, dates that do not exist refused), items of choice fields, pictures of push buttons, tab order, align/duplicate, fill & sign; duplicated pages share their fields, inserted ones get renamed (FOL-L015); picture fields that take a picture when clicked while filling | — |
| FOL-L032 | P1 | 🟡 | XFDF/FDF/CSV/TXT import and export, filling copies from table rows, calculation order, simple calculations computed in Rust; number, percentage, date, time and special formats and number ranges chosen in the properties and checked when a value is committed (the standard `AF…` scripts) | Named XML profiles, page templates. |
| FOL-L033 | P2 | ⬜ | — | XFA engine decision. |
| FOL-L034 | P2 | 🟡 | document scripts and actions listed and edited; a sandboxed engine (Boa, loop and recursion limits, nothing of the system in reach) with a documented API for the console, the calculate, format and validate scripts of fields (typed in their properties) and the `AF…` functions of forms; console time limit | Keystroke scripts while typing, page and document events, a database bridge. |

### Comparison, encryption, redaction and signatures

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L035 | P1 | 🟡 | text comparison of two documents with a report; side by side, the old document following the new one page for page (pages matched by the words they share), differences marked on both | Differences of drawings (paths) and of pictures encoded differently; following in the horizontal layout and from the old document. |
| FOL-L036 | P0 | 🟡 | passwords and permissions with AES-256 (default) or AES-128, security policies; every string and stream encrypted, the strings of stream dictionaries included; files opened encrypted are saved with the same encryption, passwords and permissions; protected documents signed, certified and time-stamped encrypted (encrypted updates keep the signatures); derived documents keep the protection, imports from protected files refused; print permissions applied; the other permissions kept when the file is opened with the user password (changes of a forbidden kind taken back, exports of text and pictures refused, the owner password asked); Save Unrestricted Copy and new passwords need the owner password when anything is forbidden | Certificate (public-key) encryption; the sorting of changes leans to allowing (an object of unknown kind counts as changing the document; what only follows a change is no change); the copy permission covers page text and pictures, not comments, form data or bookmarks. |
| FOL-L037 | P0 | 🟡 | mark areas and search results for redaction; apply by content (glyphs, picture pixels, vector paths, inline pictures, Type 3 text and the content of reused drawings under the marks removed, the rest stays text, comments and fields over the marks removed with their values, overlay codes upright on rotated pages, whole-file rewrite so earlier revisions go) or as pictures (chosen, or for pages that cannot be edited); sanitizing chosen when applying (metadata, attachments, hidden content, quotes of the removed words, scripts); the new file checked before it is written | Quadrilateral marks (a slanted line is removed by its bounding rectangle); content outside the crop box of pages without marks when pages are drawn as pictures; quotes in page text, comments and field values reported, not changed; the width of a removed word can be measured; patterns and drawings other pages draw stay for them (reported). |
| FOL-L038 | P1 | ✅ | handwritten signatures and initials, placed on several pages, kept apart from digital signatures | — |
| FOL-L039 | P1/P2 | 🟡 | PKCS#12 digital IDs (import, create, export), sign and certify (visible or invisible), appearance with picture, incremental updates keep earlier signatures; IDs of smart cards and tokens (PKCS #11 modules: the system's through p11-kit on Linux, and added ones), of the Windows certificate store and of the macOS keychain, keys kept on the device, PIN never stored, each signature checked against its certificate | Tried with real cards and stores, the issuers' certificates of a token in the signature, batches. |
| FOL-L040 | P1 | 🟡 | validation of every signature (digest, value, chain, trusted certificates), trusted list, signed revision opened in a tab; separate checks of integrity, signer, time and revocation; time stamps of signatures and document time stamps validated (imprint, nonce, authority signature and usage, trust); revocation from OCSP answers and revocation lists at hand or fetched (opt-in); validation material written to the document security store; revisions that only add it recognised when they are regular updates read the same by a second reader | Indirect, delta and reason-partitioned lists, points named relative to the issuer, AIA, policy checks; other changes of later revisions are not analysed (they count as changes); not tried against public servers. |

### Accessibility, standards, optimization and printing

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L041 | P1 | 🟡 | accessibility check and report, auto-tagging of the document and of form fields, tags pane, reading order, alternate text | Cells spanning rows in the middle of a row; manual checks listed in the report. |
| FOL-L042 | P1 | 🟡 | English and Italian interface, keyboard shortcuts, themes (light, white, dark, black), short tool names with full tooltips; a reflow view; reading aloud with the voices of the system (web view, speech-dispatcher on Linux) | A keyboard-only and screen reader pass; words marked tried with real voices on each system. |
| FOL-L043 | P2 | 🟡 | enable editing of PDF/A files; a PDF/A-2b preflight of 18 rules with findings by page and object, and conversion of a copy that fixes 14 of them (simple fonts that are not embedded become subsets of FreeSans, FreeSerif or FreeMono with the widths of the document) and declares PDF/A-2b only when nothing is left | Composite fonts without a ToUnicode map, characters no FreeFont face has; transparency and the other PDF/A rules; PDF/A-1, -3, PDF/X and PDF/UA profiles; a validator run as part of the checks (veraPDF was run by hand). |
| FOL-L044 | P2 | 🟡 | conversion of the colours of a document to grey (content, drawings, pictures, comments and fields; what is left reported), a grey preview of the pages that changes nothing | ICC profiles and rendering intents, conversion to CMYK, spot colours, separations and overprint preview. |
| FOL-L045 | P2 | 🟡 | space usage audit; recompression of the pictures of the document, of pages or of one selected picture, with presets (screen, e-book, print, archive) scaling pictures down to a resolution where they are drawn largest, an estimate picture by picture and a size report; never a larger picture; fonts, pictures and colour profiles held several times kept once, unused objects removed | Font subsets merged; MRC for scans; quality checks on the reopened file. |
| FOL-L046 | P1/P2 | 🟡 | a print window of its own with a sheet preview and the printers of the computer: ranges by number or label, odd/even, reverse, copies and collation, comments and fields, fit/actual/shrink/custom scale, paper by page, 2–16 pages per sheet, booklet, poster, two-sided order, colour; jobs sent directly (IPP with a vector print file on Linux and macOS, GDI pictures up to 600 dpi on Windows); Save as PDF; print permissions; portfolio printing of its own files | Verification on real printers (order, scale, margins, counts); vector output on Windows; booklets on both sides without a duplex printer. |

### Automation, AI and specialist functions

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L047 | P2 | 🟡 | page-mark macros; action sequences of 11 kinds of steps kept as JSON, run on the open document as one undo step or on files and folders (a plan with collisions, a result and a log per file, cancel), and from the command line (`--run-actions`, a line of JSON per file, exit status) with the same code | Interactive steps, portfolios as inputs, steps that split or protect, scheduling through the system. |
| FOL-L048 | P2 | ⬜ | — | On-device AI engine decision. |
| FOL-L049 | P2 | ⬜ | — | 3D viewer and authoring. |
| FOL-L050 | P1/P2 | 🟡 | preferences, shortcuts, ribbon and classic interface with short menus (submenus by subject), Quick Access Toolbar, command palette, settings export/import/reset, resizable panes and ribbon | Toolbox profiles, plug-ins, administrative settings. |

### Integrity contracts and sharing

| Id | P | State | What exists | Gaps and next sub-task |
|---|---|---|---|---|
| FOL-L051 | P0 | 🟡 | undo/redo of every edit; every PDF save (Save, Save As, copies, protected copies, signed files), the exports (pictures, text, attachments, form data, comments, Office documents, digital IDs) and the settings written to a temporary file and renamed over the target only when complete; recovery copies the same way, never for encrypted documents; signed documents saved as incremental updates; files of sessions with their format and version | — |
| FOL-L052 | P0 | 🟡 | page tree normalized before every page edit; pages that leave take the bookmarks, destinations, links, fields, tags and opening view tied to them (nothing of them stays in the file), pages that arrive bring theirs, and what cannot be kept is reported before OK; a relations corpus (`relations_corpus.rs`: bookmarks, destinations, links by array, name and action, fields over several pages, radio groups, comments with pop-ups and replies, tags with their parent tree) through every page operation, saved and reopened; external actions not executed on opening; links and buttons followed with a question before anything outside the document opens, programs never started | Relations of portfolio and attachment documents; files and other documents named by links are asked every time (only web sites can be allowed). |
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
