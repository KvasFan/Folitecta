# Changelog

All notable changes to Folitecta are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Note icons: a note can be drawn as a comment bubble, a note, a key, help, a new paragraph, a
  paragraph, an insertion caret, a check mark, a circle, a cross or a star (Comments › Note Icon
  for the selected note, or for the new notes; also in the dialog of a new note). Notes read
  from other files keep their icon when they are drawn again.
- Thumbnails › Highlight Visible outlines, on the thumbnail of the current page, the part of the
  page the window shows, and follows it while the pages scroll.
- The loupe enlarges more or less (Zoom In and Zoom Out of its menu, from 1.5 to 8 times); the
  choice is kept.

### Changed

- The command modules of the interface and the modules of the core library are grouped in
  folders by area (documents, comments, text, forms, navigation, protection, window).

## [0.0.11] - 2026-10-17

### Added

- Field calculations: in the properties of a text field or combo box, its value can be the sum,
  product, average, smallest or largest value of other fields (a group stands for every field
  in it). Values are calculated again whenever a field changes, reset or imported, in the same
  undo step, and the calculation is written as the standard calculate action so other viewers
  compute it too. Form › Set Field Calculation Order puts the calculated fields in the order in
  which they are calculated.
- Form › Highlight Fields highlights the fields of chosen kinds (text boxes, check boxes, radio
  buttons, combo and list boxes, buttons, signatures, barcodes) or only the required ones.
- Window › Split shows the document in two views, side by side or one above the other, each with
  its own page, zoom, layout and turn: the outlined view is the one the tools and commands work
  on, and a click in the other view makes it that one. Remove Split keeps the outlined view.
- Links pane: a click selects a link and shows it on its page, a double click follows it; the
  pane menu copies the address of the selected link, opens it in the browser and wraps long
  addresses. View › Expand All, Collapse All and Expand All Descendants work on the bookmarks,
  form fields or comments pane that is shown.

### Changed

- A click on a link in the links pane no longer opens it at once (a double click does).

### Fixed

- The Highlight Fields entry Only Required Fields had a garbled English name.

## [0.0.10] - 2026-10-17

### Added

- Barcodes: Add Barcode draws a QR Code, Data Matrix, PDF417, Aztec, Code 128, Code 39, Code 93,
  EAN-13, EAN-8, UPC-A, UPC-E, ITF or Codabar where it is dragged (the text of a linear barcode
  can be written below it). Detect Barcodes reads every barcode of the document, or of an area,
  with its kind, content and page (Show goes to it, Copy copies it), and Create Web Links from
  Barcodes puts a link over each barcode that holds a web address. Barcodes are written and read
  with rxing, the Rust port of ZXing (Apache-2.0).

### Changed

- The ribbon shows the tools as icons, with their names in the tooltip, so more than twice as
  many tools fit in the same space. View › Tool Names, or the preferences, puts the names next
  to the icons again.
- Pages appear sooner: each document draws its pages on up to three threads at once, and the
  picture of a page is encoded about four times faster. Rasterizing, redacting, cropping to the
  white margins, finding duplicate pages and exporting pages as pictures no longer encode and
  read back a picture of every page.

### Fixed

- Create Web Links from URLs adds all its links in one undo step, and writes the document once
  instead of once for every link.

## [0.0.9] - 2026-10-17

### Added

- The width of the left and right panes changes by dragging their edge; the ribbon gets taller or
  shorter by dragging its bottom edge (one to six rows of tools in each column); each ribbon group
  gets wider by dragging its right edge, up to every tool of the group without its More menu. The
  ribbon scrolls when the groups are wider than the window. A double click on an edge restores the
  automatic size; the sizes are kept between sessions.
- White and black themes next to light and dark (View › Theme and the preferences).
- Find and Replace (`Ctrl+H`) in the text of the pages: the next occurrence selected, one
  replaced at a time or all of them (on every page or the current one) in one undo step, with
  match case and whole words. Occurrences that cannot be changed are counted and left as they
  were. Annotate Results highlights the results of every search.
- Replace Image: the selected picture (or the largest one of the page) replaced with a picture
  file or the picture on the clipboard, keeping its place on the page.
- Links that go to a named destination, open a file or run a script, next to web and page links
  (Add 'Go To' Link, Add 'Open File' Link, Add Custom Link).
- Populate Forms: one filled copy of the document for every row of a CSV or tab-separated table
  (the first row names the fields; a File column names the copies), saved in a folder.
- Document Recovery: documents with unsaved changes are copied every minute (in the local data
  folder); the copies of a window that ended without saving are offered at the next start (open
  or delete them), and View › Document Recovery lists them at any time. Documents opened from
  encrypted files are not copied; signed documents keep their signatures in the copy.
- Measurements: decimal and thousands separators next to the precision, in the scale dialog;
  measurements made elsewhere show their own separators.
- Page marks: Edit and Remove list the page and Bates numbers to change or remove them.

### Changed

- The project lives at [github.com/KvasFan/Folitecta](https://github.com/KvasFan/Folitecta), with
  releases also on [gitlab.com/tirael8308/folitecta](https://gitlab.com/tirael8308/folitecta).
- The first ribbon tab is called Edit, with a Text group (Edit Text, Insert, Replace and Delete
  Text, Find, Find and Replace) and a Clipboard group (undo, redo, cut, copy, paste, delete,
  select all).
- New icons for every command, drawn from the Tabler Icons set (MIT), with a chosen icon for the
  tools used most and no command left on a generic picture.
- The chosen tool, toggle or mode stands out clearly in every theme.
- Changing the text of a page reads again only the text of that page, not of the whole document.

### Fixed

- Enter on a focused button of a dialog presses only that button, not also the default one.
- Dark theme: the menu bar, the Open and Save buttons, pane titles and other text inherited the
  light colours and could not be read; native lists and scroll bars are dark too.
- BMP, GIF, TIFF and WebP pictures can be read (combining files, new documents from pictures,
  replaced pictures) and written (pictures edited in another app as BMP or TIFF).

## [0.0.8] - 2026-10-17

### Added

- Editing the text of a page, in the Edit group of Home (and the Edit menu): the Edit Text tool
  places a caret in the text, Delete and Backspace remove the selected characters from the page,
  typing opens Insert Text with what was typed; Insert Text, Replace Text and Delete Text change
  the text drawn on the page (the rest of the line moves to make room). Characters missing from an
  embedded font are written with a standard font of the same family, and a message says so.
- The toolbar of a text selection replaces or deletes the selected text in the page.
- Text Correction tool: insertions and replacements suggested with comments. Insert Text and
  Replace Text of the Comments tab keep suggesting changes with comments; their dialog can switch
  to changing the page instead.
- Rectangles, ellipses, highlights, text boxes, lines and arrows are drawn as they will look while
  the pointer moves, with the style of the tool, instead of a selection frame. With Shift held,
  rectangles and ellipses become squares and circles, and lines go in steps of 45 degrees.
- The pencil shows a pencil cursor, stays chosen after each stroke, and draws smooth curves: the
  stroke follows curves through the points of the pointer, with at least twice as many pieces,
  both while drawing and in the comment.
- Redaction overlays: areas filled with a colour, blurred or pixelated (normal or strong) when the
  redactions are applied; overlay text with its font, size and colour, fitted to the area or
  repeated; the look of new redactions or of the selected one.
- Redaction codes: sets of codes (added, chosen) whose codes become the overlay text.
- Comment Styles pane: named styles per kind of comment, cloned from the default or the selected
  comment, renamed, edited, deleted, made the default of the tool and applied to the selected
  comment; default styles reset. A styles palette over the page picks a style for the tool.

### Changed

- Pencil comments, including those made elsewhere, are drawn as smooth curves through their points.
- The Edit Text tool moved from Objects to the Edit group of Home.

## [0.0.7] - 2026-10-17

### Added

- Explorer pane: the folders and files of the computer, back, forward, up and the folder of the
  current document; places of your own (added, moved, removed) next to home, desktop, documents,
  downloads and the drives; thumbnails of PDF files; new folders; files moved to the recycle bin
  after a confirmation; double click opens a PDF file or enters a folder.
- Folder filters: PDF files, every file that can be combined, all files, and filters of your own
  (name and extensions) added, edited and removed; lists of the files shown or selected saved as
  text, and the PDF files of such a list opened.
- Source files of a combined document: PDF files, pictures and text files added one by one, from a
  folder or from a list; moved up, down, to the top or the bottom (several together); the pages
  to take from each PDF file; a details view with kind, pages and folder.
- Pictures of the pages: the Edit Content › Images tool outlines the pictures a page draws; a
  click selects one and shows its toolbar, a double click opens it in the picture editor.
- Picture editor: quarter turns (the picture keeps its centre on the page), mirroring, gray,
  cropping by dragging (the kept part stays where it was), straightening of tilted scans, filters
  (smooth, Gaussian blur, sharpen, mean removal, despeckle, edge detection, four embosses), pixel
  formats (24 and 32 bits, gray with or without transparency, 256 colours from a palette, black
  and white), zoom and undo. Turns, mirroring, gray, straightening, filters and pixel formats also
  work on the selected picture without the editor.
- Pictures edited in another app (the system app for pictures or one chosen), as PNG, JPEG, BMP
  or TIFF, and put back on the page with Update once saved there. Apply to All changes a picture drawn in
  several places everywhere; otherwise only the selected drawing changes.
- Split pages editor: horizontal and vertical cut lines over a page, dragged or typed in
  millimetres, centimetres, inches or points; a grid of equal parts; lines removed from the list
  or the preview, or all of them after a confirmation; rulers, line colour and zoom; every page,
  odd or even pages, or the page shown. It replaces the dialog that cut pages into tiles.

### Changed

- Comparing documents finds the common pages and words with a diff that keeps memory linear, with
  a time limit on very different documents.
- The tags tree is read once per change of the document for the panes that show it.
- The code is organized in smaller modules (canvas, dialogs, panes, state, interface self-test,
  tags); rectangles share one set of geometry helpers.

## [0.0.6] - 2026-10-17

### Added

- Accounting calculator: tapes of amounts placed on the page as comments, typed on a keypad or
  the keyboard (Enter adds), with running totals, comments on each line, lines added, deleted
  and edited, decimals (none to six, or floating), rounding (half, up, down), date and time,
  background, border and text look, undo and redo in the pane, a Calculator ribbon tab and
  settings for new tapes. Moving a tape keeps it editable.
- Metadata templates: keep the title, author, subject and keywords of a document and give them
  to others; the XMP properties of the document with name and value columns; save the XMP
  packet (one made from the document information when there is none).
- Guides: export them to a file, import them, copy the guides of the current page to every page.
- The document file: rename it, move it to another folder, move it to the recycle bin; Save To.
- Continuous zoom tool: drag up to zoom in, down to zoom out.
- Size of the current page and of the file; links exported as a text list.
- JavaScript console: run scripts or the selected part of them against the open document
  (`this.numPages`, `this.pageNum`, `this.getField(name).value`, `this.info`, `app.alert`,
  `console.println`, `util.printf`); the field values, page and document information they change
  are applied to the document. Scripts run in a worker apart from the window and stop after a
  time limit. Snippets saved as files in a folder of their own, cloned, deleted and edited.
- Named document scripts: add, edit, rename, delete and run them; JavaScript options (time limit,
  changes allowed, alerts as messages).
- Sound and video comments: sounds made from WAV files, audio and video files (MP4, WebM, Ogg,
  MP3, M4A, WAV, FLAC) embedded in screen comments; a media player docked or floating, with play,
  pause, stop, mute, controls, volume, position and full screen; a list of the media of the
  document; titles and descriptions; media saved as files; playing turned off and on. Sounds and
  media of other documents (sampled sound, renditions, rich media) play too.

### Fixed

- Text files are offered with the type of their extension in the save dialog (web pages, JSON,
  XMP), not always as text or CSV.

## [0.0.5] - 2026-10-17

### Added

- History pane: documents opened before, grouped by day, pinned ones first, sorted by opening,
  closing or modification time or by name, with thumbnails, paths and a preview on hover; open
  at the page where a document was closed, pin, remove, copy the path, open the folder.
- Sessions: save the open documents as a session, open a session instead of the open documents
  or next to them or in a new window, rename, remove, sort, export and import sessions as files.
- Stamps pane: standard stamps, your own collections, favorites and recently used stamps, text
  stamps, pictures from the clipboard, duplicate, recolor, sort, zoom.
- Stamp designer: text, rectangles, ovals, lines, polylines, polygons, curves and pictures on a
  sized background, templates, cropping to the content, and fields filled in when the stamp is
  placed (date with a chosen format, name, e-mail, company, login, counter, values asked at
  placement, page-mark macros).
- Document tools: stamps from the current page or from an area, next and previous bookmark,
  pages split at the guides, pages cut to their crop box, management of watermarks, headers and
  footers and page numbers, a space usage audit, recompression of pictures as JPEG (with a size
  limit), pages of typed text or of an RTF file, highlights for the phrases of a text file,
  leaving PDF/A, a signature repeated on several pages, scripts for document events.
- Compare documents: the text of two documents compared word by word, changes, insertions and
  deletions highlighted on both (words or whole rows), a list of the differences with previous,
  next, first and last, the pages that differ, and a comparison report document.
- Tables: drawn on the page with the table tool, with a header row; an editor for the cell text,
  rows and columns inserted above, below, left or right, deleted rows and columns, merged and
  split cells, cell fill, bold text and alignment, line and text colour, line width and text
  size. Double-click a table to edit it again.
- Form editor: copies of a field in rows and columns, and the tab order and calculation order
  numbers shown on the fields.
- Links: open the web address of the selected link in the browser.
- Loupe: follow the mouse (on by default).
- Accessibility: automatic tagging of a document from what its pages draw (headings by text
  size, paragraphs, figures, links and other annotations, artifacts for drawings and for running
  headers, footers and page numbers) and of its form fields only.
- Tags pane: the structure tree with its marked content and annotations; new tags, children and
  siblings, properties (type, title, alternate text, actual text, language), moving tags,
  deleting them, turning them into artifacts, tags made from the selected text or annotation,
  finding the tag of a selection, highlighting a tag's content on the page, copying its text,
  deleting empty tags, role map and class map editors, applying the role map, marking the
  document as tagged, removing the whole structure.
- Order pane: the reading order of each page, numbered or with the tag types on the page, items
  moved up and down, tagged again (headings, paragraph, figure, table, cell, list item, field,
  formula, note, reference, caption) or made artifacts, neighbouring items joined, table cells
  one by one, tables and figures only.
- Content pane: the marked content of each page, artifacts and content without tags, shown on
  the page and in the tags pane.
- Accessibility check: 32 rules about the document, the page content, forms, alternate text,
  tables, lists and headings, with the items found on the pages, automatic fixes (tags, title,
  bookmarks, tagged annotations and fields, tab order, field descriptions), the language and
  the alternate text of the figures asked in dialogs, rules passed, failed or skipped by hand,
  explanations, and a report that can be saved as a web page.

### Changed

- Marking or redacting many search results is a single undo step.
- New watermarks, headers, footers and page numbers are marked as artifacts, so screen readers
  leave them out.

### Fixed

- Moving or resizing a stamp made with the stamp designer no longer redraws it as a plain text
  label.
- Messages shown by a dialog now close by themselves like the others, and jumps started from a
  dialog finish their scroll and highlight.
- A macro chosen from the menu while a dialog is open goes into the dialog's field instead of
  the clipboard when the last focused field is no longer known.

## [0.0.4] - 2026-10-17

### Added

- Comment exchange and review: export and import comments as XFDF, FDF or PDF; comment
  summaries (comments only, separate pages or connector lines, sorted by page, author, date or
  type); replies, review states with their history, check marks, filters, pop-up notes on the
  page and comments pane options.
- Bookmarks: editing of the bookmark tree with drag and drop, styles and colours, destinations
  with their zoom, web addresses, import and export as text, bookmarks from the page text;
  editing of named destinations.
- Interface: classic toolbars, more page layouts, a pan and zoom window, touch mode, a menu bar
  that can be hidden, zoom levels.
- Digital signatures: sign and certify with digital IDs (PKCS#12, RSA and ECDSA), create
  self-signed IDs, validate signatures against trusted certificates, clear signatures. Signed
  documents are saved as incremental updates, so their signatures stay valid.
- Handwritten signatures and initials (drawn, typed or pictures) kept in a library and placed on
  pages, plus the signer's name, date, e-mail and company as page text.
- Macros in watermarks, headers and footers, page numbers and Bates numbers: labels, dates and
  times, document information, file and folder names, bookmark titles, computer and user names,
  environment variables; a macro picker and guide; Bates numbering that continues across runs.
- An external application launched with the details of the view.
- Layers: tree with groups and sublayers, view-only visibility that can become the default,
  new layers and groups, locks, sorting, merging, flattening, extraction of visible or selected
  content, pages of another file imported as a layer.
- Attachments pane with columns, open, save, edit (name, description, relationship, contents)
  and automatic showing; fields pane with name groups, sorting, selection by type and a manual
  tab order.
- Search: accents ignored on request, words near each other (adjacent, paragraph, page,
  document) in separate colours, comments, bookmarks, form fields, document information,
  attachments (including PDF attachments) and links, grouped results, search through the PDF
  files of a folder.
- Portfolios: create portfolios from files, files view with folders, details or tiles, columns
  and sort order, preview, file information and custom columns, initial file, cover page.

### Changed

- Text files used to create documents start a new page at each form feed.
- Marking or redacting search results uses the results shown.

## [0.0.3] - 2026-10-16

### Added

- Comment styles: a style editor in the properties pane for colour, fill, opacity, thickness,
  dashes, line endings, text size, text colour and alignment; default styles per tool; copy and
  paste of styles; the Format tab commands work on the selected comment.
- Appearances are generated from the comment entries for every comment type Folitecta draws, so
  moving, resizing and restyling keep strokes and text crisp in any viewer.
- New comment tools: polygon, polyline, cloud, callout, free highlight, eraser for pencil strokes,
  insert-text and replace-text marks, file attachments.
- Measurement tools: distance, perimeter and area with a calibrated scale (units and ratio),
  labels on the page and CSV export of all measurements.
- Form design: create text, check box, radio button (groups), combo box, list box, button and
  signature fields by dragging; edit name, tooltip, required, read-only, multi-line, password,
  comb, maximum length, options, default and export values, caption, font size, colours, border
  and alignment; delete fields; tab order by rows, columns or structure.
- Form filling directly on the page; import and export of field values as XFDF, FDF, CSV or
  tab-separated text; reset of one field or of the whole form.

### Changed

- Changing a field value always redraws the field appearance (check boxes and radio buttons
  switch state, text fields show the new text).
- The feature matrix counts a submenu as available when all of its commands work and as partial
  when some of them do.

### Fixed

- The self-test now also detects squeezed buttons; the document properties button no longer
  shrinks in narrow panes.

## [0.0.2] - 2026-10-16

### Added

- Text layer from the renderer: mouse text selection with a floating action bar, copy,
  select all, text highlight / underline / strikeout tools, highlighted search results with
  previous/next navigation, markup or redaction of every search result, links created from
  web addresses found in the text.
- Comment editing on the page: select, move and resize with handles, arrow-key nudging,
  copy / cut / paste between documents, flatten a single comment, localized comment types.
- New tools: snapshot to the clipboard, stamps (presets, custom text, pictures), redaction
  marks, image placement, typewriter, loupe.
- View options: real view rotation, rulers with units, grid, guides with snapping, link and
  field highlighting, transparency grid, dark and high-contrast page modes, info bar.
- Redaction (areas, text, search results) with permanent application; password protection
  (AES-128, permissions); document script listing and removal.
- Pages: replace with pages of another file, overlay / underlay, split into tiles, rasterize,
  crop to visible content, normalise sizes, find duplicates, reorder by dragging thumbnails,
  page labels, transitions, backgrounds, pictures as pages.
- New documents from text files and from the clipboard; paste special; printing with page
  range, resolution, comments on/off and fit to paper.
- Properties pane redesign, pane option menus, Italian translation of every command.
- Interface self-test (`--ui-selftest`) with layout audit, screenshots and simulated input.

### Changed

- The ribbon fits any window width: large buttons for the main commands, stacked small buttons
  for the others, groups collapsing from the right only when needed. The pane strip, status
  bar and dialogs adapt to the window size; menus show check marks for on/off options.
- Generated text uses real Helvetica metrics and WinAnsi encoding (better centring of
  watermarks, headers, numbers and stamps; accented and typographic characters).

### Fixed

- Overlapping and clipped interface elements at small window sizes and after resizing.
- "Remove all page marks" no longer deletes flattened comments or applied redactions.
- Fit-width zoom with rulers and in the two-page layout.

## [0.0.1] - 2026-10-16

### Added

- Core library (`folitecta-core`): undo-aware document model on lopdf, page operations,
  comments with appearance streams, bookmarks, forms, attachments, layers, links and named
  destinations, text extraction and search, watermarks / headers and footers / page and Bates
  numbering, hayro render service, exports, command registry of 1,977 commands.
- Desktop application (`folitecta`): Dioxus interface with ribbon and classic menus generated
  from the registry, 27 panes, document canvas with drawing tools, dialogs for every
  implemented operation, command palette, toasts, light/dark themes, English and Italian.
- Continuous integration (format, clippy, tests on Windows, Linux and macOS) and a release
  workflow that publishes binaries for all three platforms on `v*` tags.
