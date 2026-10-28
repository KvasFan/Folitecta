# Changelog

All notable changes to Folitecta are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.5.0] - 2026-10-28

### Added
- The Pencil's quick panel: choosing the Pencil opens a small panel over the pages with the
  thickness of the stroke (0.5 to 8 points), four standard colours — black, green, blue and red —
  and a button that opens the colour palette for any other colour. They set the style of the
  next strokes, which the Pencil keeps; the panel goes with another tool.
- The same quick panel for the free highlighter (yellow, green, pink and blue, thicker lines)
  and the shape tools — rectangle, ellipse, line, arrow, polygon, polyline and cloud — with the
  fill of closed shapes (none, the four colours or any colour of the palette).
- The quick panel for the measuring tools — distance, perimeter, area, angle, radius and volume
  — with their thickness and colours and the scale of the measures, which a click opens; the
  measuring tools stay chosen, so several measures are taken one after the other.
- Pen pressure for the Pencil: with a pen that tells how hard it presses (a graphics tablet or a
  stylus), the stroke is thinner where it presses lightly and thicker where it presses hard,
  from 0.3 to 1.7 times the chosen thickness. The stroke shows so while it is drawn, is saved as
  an appearance every reader shows, and keeps its pressures when it is moved, restyled or
  partly erased. A button of the Pencil's panel turns it off; the mouse draws as before.

## [0.4.1] - 2026-10-26

### Changed
- The rectangle, ellipse, line, arrow, polygon, polyline and cloud tools stay chosen after a
  shape, like the pencil, so several shapes are drawn one after the other; Escape goes back to
  the hand. The other tools follow Tools › Keep Tool Selected as before.

### Fixed
- Export to Office Formats… wrote nothing: closing its dialog also ended the export before the
  save window opened. Word, RTF and web pages are written again.

## [0.4.0] - 2026-10-26

### Added
- Digital IDs on smart cards and tokens, in the certificate store of Windows and in the keychain
  of macOS: Digital IDs lists them (cards and tokens through their PKCS #11 module — on Linux the
  modules the system has registered, and modules added there), and the signing dialog offers
  them with the IDs kept by Folitecta. Their keys never leave the card or the system; the PIN is
  asked when signing (or by the system, or on the reader's own keypad) and never kept; every
  signature is checked against the certificate before it is written.
- Field scripts: the properties of a text field or combo box have its calculate, format and
  validate scripts, typed and applied one at a time. A calculate script sets the field's value
  as the others change (in the calculation order); a format script changes only the text shown
  (`AFNumber_Format`, `AFPercent_Format`, `AFSpecial_Format` and the other functions of forms are
  there); a validate script refuses a value with its alert. The scripts of opened documents run
  the same way.
- A sandboxed JavaScript engine runs the console, the scripts of fields and the scripts of
  opened forms: a fresh engine for every run, stopped after too many loop rounds or too deep a
  recursion, with no network, files, databases or programs in reach, and the documented API of
  the document, `event`, `app`, `console`, `util` and the `AF…` functions of forms.
- Edit Paths (Edit Content › Shapes, also in Edit Content of the ribbon): a click chooses a
  vector path the page draws — a line, a curve, a rectangle — and shows its nodes and a bar with
  its line and fill colours and line width; a node dragged, a colour or width changed or the path
  deleted changes the page content itself (one undo step each), so it stays when the file is
  opened again. Clipping paths and the paths of reused drawings are not offered yet.
- Export to Office Formats… (File, and Convert › Export): the text of all pages or of the current
  page as a Word document (DOCX), rich text (RTF) or a web page (HTML) — headings (as heading
  styles) and paragraphs in reading order, each page beginning on a new page when asked — written
  where you choose and never over the document; the report counts the paragraphs and headings
  and what is not exported yet (pictures, comments).
- Action Sequences (Tools › Action Sequences…): named sequences of steps — rotate pages, delete
  empty pages, recompress pictures, colours to grey, flatten or delete comments, remove scripts,
  watermark, page numbers, document information, PDF/A — kept in the settings, imported and
  exported as JSON, run on the open document as one undo step (a step that fails leaves the
  document as it was) or on files and folders: the outputs and collisions shown first, then a
  result and a log for every file, with Cancel. The same sequences run from the command line
  with `folitecta --run-actions`, one line of JSON per file.
- Convert Colours to Grey… (Document and Convert › Modify): the colours of the page content,
  drawings and comment appearances (RGB, CMYK and ICC-based), the pictures in those colours and
  the colours of comments and form fields become grey in one undo step; the dialog first tells
  what would change and what would be left (patterns, spot colours, Lab colours, shadings,
  pictures inline in the content). View › Grey Preview shows the pages in grey without changing
  the document.
- Recompress Pictures works on the whole document, the current page or the picture selected on
  the page (Recompress Image…), with presets — screen (96 dpi), e-book (150 dpi), print (300 dpi),
  archive (resolution kept) — or settings of one's own: each picture is scaled down to the
  resolution chosen where it is drawn largest, then kept as JPEG if that makes it smaller. An
  estimate lists, picture by picture, its page, resolution, size in pixels and bytes before and
  after, or why it stays (transparent, its colours, not smaller), before anything changes.
- Preflight PDF/A… and Convert to PDF/A… (Document and Convert › Modify): the document is checked
  against 18 rules of PDF/A-2b (encryption, file identifier, XMP metadata and their agreement
  with the document information, output intent, embedded fonts, forbidden actions and
  JavaScript, annotations, embedded files, LZW, XFA, interpolation, layer configurations,
  PostScript), each problem listed with its page; conversion fixes what it can in a copy opened
  in a new tab — with an sRGB output intent of Folitecta's own — and declares PDF/A-2b only when
  no rule checked is left broken. Fonts that are not embedded are not embedded yet.

### Fixed
- A heading directly above its text is a paragraph of its own (lines of a clearly different size
  are no longer joined), for the reflow view, reading aloud and exports.
- Saving keeps the XMP metadata in step with the document information (the modification date
  and the producer saving writes), so a PDF/A document stays conformant once saved again.

## [0.3.1] - 2026-10-26

### Changed
- Shorter menus: each menu of the menu bar keeps the commands used most at the top (at most 22
  rows; File and Edit had over 80 and 110) and puts the others in submenus by subject — File ›
  New, Save in Other Ways, History and Sessions, File on Disk, Send and Share, Places; Edit ›
  Paste Special, Select, Page Text, Arrange, Format, Pictures and Objects, Layers, Interface; and
  so on in View, Document, Comments, Tools and Window. The long submenus (Tools › Tool,
  Annotation, the macros, the Bookmarks and Layers pane menus, path editing) are split the same
  way, also in the ribbon dropdowns and the pane menus. Commands not placed yet are under More.

### Fixed
- On Linux the pencil pointer is no longer twice as large as the other pointers on HiDPI screens
  (WebKitGTK up to 2.54 enlarged SVG pointers twice): it is a 20-pixel picture.

## [0.3.0] - 2026-10-26

### Added
- Tables, calculator tapes, designed stamps and text stamps draw text beyond Western European
  characters (Polish, Czech, Greek, Cyrillic, symbols and more) with the embedded subset of GNU
  FreeFont FreeSans, like text boxes since 0.2.1: one subset for all the text of a drawing, bold
  text filled and stroked (FreeSans has no bold), shown, printed, copied and found after
  flattening. Text stamps no longer refuse such text. Text that cannot be drawn (right to left,
  joining or combining letters, characters the font lacks) is refused before any change; the
  table editor names it while it is typed and stays open with what was typed. WinAnsi text keeps
  the standard Helvetica, written as before.
- Watermarks, headers and footers, page numbers, Bates numbers, comment summaries and documents
  made from text files or e-mail messages draw text beyond Western European characters the same
  way: one subset for all the pages a mark is added to, one for a whole summary or document, bold
  headings and labels filled and stroked. Text they cannot draw is refused before any change (for
  marks after macros such as `{title}` are filled in), and the watermark, header and footer and
  numbering dialogs name it while it is typed. WinAnsi text keeps the standard Helvetica.
- Review of recognized text: words text recognition read with doubt (a letter it was less than
  80% sure of) are listed one by one beside the page, with a picture of the word and its line
  and the word marked on the page, to correct (Enter), keep or remove (specks and lines read as
  text), or skip; each check is one undo step. The doubt is kept in the page, so a saved document
  can be checked later with Review Recognized Text. After OCR Pages the review opens by
  itself when there are doubtful words (an option of the dialog), and the message counts them.
- Clean Up Scans: the pages that are a picture of paper are looked at in the background (with
  progress and Stop), and the blank sides (ink below a share you set, leaving out the edges of
  the sheet, specks and show-through) and the sheets fed askew (from an angle you set) are listed
  with their thumbnails; each can be left out or shown, and the clean-up deletes the blank pages
  and straightens the others in one undo step.
- Markdown and CSV files become documents (File › Open, dragged onto the window, or New › From
  Text File). Markdown is set with its headings (which become bookmarks), bold, italic, code,
  struck text, links that work, lists, task lists, quotes, code blocks, rules and tables;
  pictures are shown by their description. A CSV file becomes a table: its separator guessed
  (comma, semicolon, tab or bar), quoted fields kept whole, numbers aligned right in their local
  forms, the header repeated on every page and the pages turned sideways when the table is
  wide. Western European text is set in Helvetica, its bold and italic and Courier; other text in
  the embedded Unicode font.
- Compared documents side by side (Compare Documents › Show › Both): the new document on the
  right with every tool, the old one on the left with its differences marked, following the new
  one as it scrolls; each page of the new document is matched with the page of the old one that
  holds its first word both share, so pages inserted or removed do not put them out of step.
  Moving between the differences keeps the new document in front. Both again shows the new
  document alone.
- Bookmarks from Headings: the lines set clearly larger than the text are found (running heads
  left out, a heading on two lines joined), listed by level so any can be left out, and made into
  nested bookmarks that open their pages at the heading, in place of the bookmarks there are or
  after them. Insert Table of Contents: pages listing the bookmarks, indented by level, with
  dotted leaders to the numbers the pages have once the contents are in, every entry a link,
  inserted at the start or before the current page. Each is one undo step.
- Three measuring tools: Angle (three clicks: the end of one line, the corner, the end of the
  other; drawn with an arc and labelled in degrees), Radius (a circle dragged from its centre,
  labelled with its radius and diameter) and Volume (an area drawn, then the depth asked; labelled
  in cubic units). They use the measurement scale and its separators and are listed in the CSV
  export of the measures.
- Tick marks (Comment › Drawing › Tick Mark…): choose a check, a cross, a dot, or a number,
  letter or roman numeral plain or in a circle, its size and colour; then each click on a page
  places one, the numbers and letters going on by one (the next one shown in the status bar),
  until another tool is chosen. They are kept as data and drawn again when resized.
- Reflow (View › Reflow, `Ctrl+4`): the text of the document as it is read, in paragraphs and
  headings filling the width of the window at a size of one's choosing, each page's part marked
  with its number; running heads, page numbers and words broken at the end of a line are mended.
  The page at the top becomes the current page and a page gone to is scrolled to.
- Read Aloud (View › Read Aloud, `Ctrl+Shift+Y`): a bar that reads the paragraphs from the current
  page (or the text selected) with a voice of the system, marking the one being read on its page
  or in the reflow view; pause, previous and next paragraph, stop, the voice and the speed. On
  Windows and macOS the voices of the system are those of the web view; on Linux reading goes
  through speech-dispatcher (optional), spoken to over its socket and started when needed.
- Every release package carries `THIRD-PARTY-LICENSES.txt`: the libraries built into the program
  for that system, each with its licence and the address of its source, and their licence texts
  (written by `crates/notices` from the published sources; a library without a text stops the
  release). The macOS app also carries Folitecta's own `LICENSE`.
- New documents from templates (File › From Template…): ruled, squared, dotted and graph paper,
  music staves, Cornell notes and handwriting lines; a week, months with their days in the right
  columns (from the month chosen, weeks from Monday or Sunday) and a to-do list with boxes to
  tick; a letter, meeting notes and an invoice that works out its amounts, subtotal and total.
  The gallery shows the first page of each on the paper chosen (upright or sideways); the to-do
  list and the office papers are forms to fill in, sized to the paper; their words follow the
  language of the interface (text beyond Western European characters in the embedded font).

- Tab groups (Window › New Vertical / Horizontal Tab Group): the active document goes to a second
  group beside the others or below them, each group with its own tab strip and its document
  shown; a press in the other group's view or a click on its tabs works there. Tabs move to the
  other group, are shared out between two groups or gathered into one; documents opened join the
  group worked in. Move to New Window opens a saved document in another Folitecta window and
  closes its tab; New Blank Window opens an empty one.

- Scanning (File › From Scanner…): on Linux through SANE (`scanimage`), with the scanner, its
  source (flatbed or feeder), colours and resolution chosen in Folitecta and kept as named
  presets; on Windows through the system's scanning window. Each page is the size of the paper
  scanned; colour and grey scans are kept as JPEG. Sheets printed on both sides can be scanned
  as two stacks, the backs put between the fronts (also with the stack turned over), and Clean
  Up Scans can open on the new document.

### Changed
- Pictures made of the pages follow the layers of the view, as printing does: Export to Images,
  a page copied as a picture, the snapshot of an area and a stamp made of the page leave out what
  a layer hidden in the view hides, comments included (before, they followed the default
  visibility of the document).
- Messages that count say "1 page deleted", "1 comment" or "1 file" for one, in English and
  Italian, instead of "1 page(s)" or "1 files"; messages with several numbers are written as
  labels ("Pages: 6 · sides: 4 · sheets of paper: 2"), right for any number.
- The Windows program carries the C runtime itself, so it starts without the Visual C++
  Redistributable installed, and it has its icon, its version (shown in the file properties) and
  a manifest (the common controls of Windows 10 and 11 for the dialogs of the system).
- The macOS app is signed ad hoc as a whole bundle (program, Info.plist and resources sealed
  together); it is still not notarized.
- The release and CI workflows use the current versions of their actions (Node.js 24); a
  downloaded artifact whose digest does not match now stops the release.

### Fixed
- The table editor closed when its table could not be saved, and what was typed was lost; it now
  stays open with the reason shown.
- The Linux program starts without libxdo. It needed `libxdo.so.3` only because the menu library
  of the interface toolkit linked it (Folitecta never calls it), so it did not start on systems
  that have only libxdo 4 (Arch, Manjaro, CachyOS) or no libxdo at all (Ubuntu desktops); the
  library is now opened only when it is used, and the compatibility link of 0.2.1 is no longer
  needed. Building from source no longer needs `libxdo-dev`.

## [0.2.1] - 2026-10-20

### Added
- Printing has its own window with a live preview of the sheets and the printers of the
  computer: copies and collation; ranges by number or page label, odd or even pages, reverse
  order; comments and form fields; fit, actual size, shrink, custom scale, paper by page size,
  auto-rotate; 2 to 16 pages per sheet; booklets (binding, signatures, back cover); posters
  (overlap, cut marks, labels in a 3 mm strip under the tiles); orientation, paper, two-sided
  printing (blocked with an explanation when the printer cannot turn the paper that way) and
  colour. Jobs go straight to the printer: on Linux and macOS the vector print file goes to the
  local print server, on Windows the sheets go as pictures at up to 600 dpi; progress and cancel
  are in the status bar. Save as PDF and the system dialog remain as fallbacks. What prints
  follows the document: printable comments only, NoRotate notes upright, layers in their print
  state, visibility expressions included.
- Text boxes, typewriter text, callouts and measurement labels draw text beyond Western European
  characters (Polish, Czech, Greek, Cyrillic, symbols and more) with an embedded subset of GNU
  FreeFont FreeSans: it shows, prints, is copied and found, also after flattening. Text that
  cannot be drawn yet (right-to-left, joining or combining letters, characters the font lacks)
  is refused before any change, with the characters named while typing; no `?` is written.
  Comment imports name the comments they cannot draw and import the others on request.
- E-mail attachment dialog (File › Email Document…, Share › Send, Form › Forms › Email Form
  Data…): for each document the current state or the saved file, its form data (XFDF, FDF, CSV,
  text) and its comments (XFDF, FDF); PDF files cut to some pages, with their comments kept,
  flattened or removed, separately or in one ZIP archive. Every file is prepared in the
  background and shows its size, what it holds and its first pages. Save Draft… writes an unsent
  `.eml` where you choose; Open in Mail Program hands the files over. Nothing is sent.
- New password protection uses AES-256 by default; AES-128 stays a choice in the Password
  protection dialog and in security policies (older policies load as AES-256).
- Revocation of signature certificates is checked from the OCSP answers and revocation lists in
  the signature, its time stamp or the document security store, or fetched online; online checks
  are off until turned on in Signatures pane › Options. Add validation material… writes the
  certificates and revocation data to the document security store as an update that keeps the
  signatures valid (encrypted for protected documents).
- Redaction removes the vector paths under the marks, and the text, pictures and paths of
  reused drawings, inline pictures and Type 3 text, without drawing the page as a picture; text
  of hidden layers and clipped text go too. Applying redaction opens a dialog: what goes,
  warnings for blurred areas and signatures, and sanitizing on by default (metadata,
  attachments, hidden content, quotes of the removed words, scripts). By default the result is
  saved as a new file that is checked before it is written. The document security store and its
  declaration go with every redaction.
- What you draw or write goes to the selected layer: with one layer selected in the Layers pane
  (or chosen with Edit › Choose Layer), new comments are put on the layer and placed pictures and
  barcodes become content of the layer; the status bar shows the layer, Default Layer stops it.
  Hiding a layer hides its comments in the pages, thumbnails, printing, comment summaries and the
  Comments pane (Show Comments from Hidden Layers, off by default); pictures exported from the
  pages follow the document's default layer visibility, not the view. Flattened
  comments keep their layer; Choose Layer, the Layer field of the properties and Move to New
  Layer move comments and pictures between layers. Rasterized pages keep the comments of hidden
  layers as comments on their layer.

### Changed
- Scrolling and zoom are much smoother on large documents:
  - Scrolling no longer waits for the program: the wheel, the scroll bar and Ctrl+wheel are
    followed by a script in the page, and mouse moves and wheel events reach the program only
    where something listens to them. On a 1000-page document the longest pause while scrolling
    fell from about 0.4 s to about 40 ms.
  - A view holds only the pages near what it shows (the others are boxes of their exact size);
    pages are found without walking the page tree; comments, bookmarks, links and form fields are
    read once per revision. On a 1000-page document the first sharp pages show after under 50 ms
    instead of about 0.45 s, and text selection and drags answer in a few milliseconds instead of
    0.2–0.4 s.
  - Each document has one drawing queue (the pages shown first, then those ahead, thumbnails
    last); a picture asked for twice is drawn once and one no longer shown is not drawn; a page
    drawn alone uses several threads; recent pictures are kept (128 MB for all documents); a page
    shows its small picture until the sharp one comes.
  - Zoom scales the pictures shown at once and draws new ones when it has stayed the same for
    90 ms; at high zoom pages and the loupe are drawn in 512-pixel tiles. Six Ctrl+wheel steps
    are sharp after 0.11–0.13 s instead of 0.56–0.58 s; a dynamic zoom drag on a 50-page document
    is sharp after 0.2 s instead of 2.4 s.
  - An edit draws again only the pages it touched, and the text of the pages is kept when
    comments change.
  - Go to Page…, bookmarks, search results and thumbnails jump to a far page that is sharp after
    about 40 ms on a 1000-page document (0.2.0: about 0.4 s for a page not seen before).
  - Linux: the page view is composited on the graphics card where that is known to be safe (a
    graphics card with a render node, a driver other than NVIDIA's, not a virtual machine);
    elsewhere the setting of 0.2.0 stays. `FOLITECTA_GPU=off`, `on` or `no-shared-buffers`
    overrides the choice.
  - Linux: the memory the render threads free goes back to the system once the pages have been
    drawn (a second without drawing, and when a document closes), and the threads share four
    memory areas; the interface uses less memory than 0.2.0 (after a 1000-page document about
    470 MB instead of about 780 MB).
- Signing, certifying and time-stamping a protected document write a file encrypted like the
  document, and saving a signed protected document keeps its signatures valid (encrypted
  incremental update). Documents made from a protected document are protected like it; pages,
  layers and comments of protected files are refused by unprotected documents. Protected
  documents leave no recovery copies, history thumbnails or history titles.
- Print permissions of protected files apply to readers who opened them with the user password
  (or without one when the file has none); the owner password lifts them, at open or typed in
  the print window. Files protected by Folitecta with printing allowed now permit full-quality
  printing; files protected by 0.2.0 with the Print permission ticked print as 150 dpi pictures
  when opened with their user password (typing the owner password in the print window lifts
  this). Save as PDF keeps the protection.
- Later revisions that only add validation material keep a signature's integrity: they must be
  regular updates, read the same by a second reader; anything else counts as a change after
  signing. Signature checks run off the interface thread, and earlier results stay dimmed with
  "Checking the signatures…" while they run again. Clear All Signatures also removes the
  validation material.
- A missing mail program, one that cannot take the files, or a web browser handling e-mail links
  is reported in the attachment dialog, with Save Draft…, Open the Draft and Show the Files; no
  draft or `mailto:` message opens by itself. Files handed over go to a new folder only you can
  open; drafts fold long subjects and write non-ASCII file names the standard way.
- Flattening comments is much faster on documents with many comments (1,200 comments: 10.9 s →
  0.11 s).
- Text stamps refuse text beyond WinAnsi; a failed edit keeps the undo and redo history.
- Release packages carry a `fonts` folder next to the program (in the app bundle's resources on
  macOS).

### Fixed
- AES-128 and RC4 files opened with the owner password, or with a password with characters
  outside ASCII (also typed with separate accents, or written as UTF-8 by other programs), were
  decrypted into noise or opened as empty pages, and saving kept the noise; they now open
  correctly, and a key that cannot be verified is refused.
- Signatures in encrypted files did not validate; a byte range that does not leave out exactly
  the signature's own value now makes the signature invalid.
- Passwords AES-128 could not keep were silently weakened; they are now refused.
- A wrong password asks again; certificate-protected files are reported; Revert to Saved asks for
  the password of protected files. Protected copies always open in readers.
- Pictures and attachments handed to other programs go to a private folder of the user that is
  removed afterwards.
- Copies of some pages (e-mail, attachments) keep nothing of the pages left out: pictures, fonts,
  drawings, comments, form fields and their XFA data, layers, bookmarks, links, destinations,
  tags, threads.
- Redaction: text after kerned words keeps its place; strokes under skewed or stretched matrices
  and miter joins leave no ink inside the marks; patterns, Type 3 glyph procedures and picture
  metadata of removed content no longer stay in the file; patterns and fonts other pages keep with
  what was under the marks are reported. A signed version brought back by Undo after a later save
  is never written with a broken signature.
- Comments exported as FDF and imported again, or imported from another PDF, stay on a layer of
  the same name instead of a copy of the layer; re-importing XFDF keeps the layer of a comment;
  pasting a comment on a layer no longer copies the layer. Resources shared by several pages are
  copied before something is added to one of them. Documents that claim PDF/A-1 refuse new
  layers.
- With the Signatures pane or the details of a signature open (after signing, time-stamping,
  Validate All Signatures or Summary), the interface drew itself again without pause, keeping the
  processor busy, and dialogs that measure themselves (Split Pages, picture editor), the
  portfolio preview and menu searches stopped working.
- The interface no longer stops while its window is not drawn (minimized or hidden), and the
  results of page scripts are no longer lost. Sounds and videos play on Linux.
- With the side pane hidden, the page area could lose its width.
- Once a drag or an editing tool had been used, every mouse move in the window waited for the
  program.
- Portfolio › Print prints only the portfolio's own PDF files, with the portfolio's protection.

## [0.2.0] - 2026-10-18

### Added
- Time stamps (RFC 3161) from a time stamp server: a signature can get a time stamp of its value
  (Add a time stamp in the signing window), and Document › Signatures › Timestamp adds a document
  time stamp, as an incremental update that keeps the earlier signatures valid. Only a SHA-256
  digest is sent to the server set by the user; the answer is checked (status, data, nonce,
  signature and certificate of the authority) before it is written.
- The details of a signature show separate checks: integrity, signer (or authority), time (a
  valid time stamp of a trusted authority, or the signer's computer) and revocation (not checked
  yet); document time stamps are validated and shown in the signatures pane.
- E-mail with the files really attached: the document, every open document, both as a ZIP
  archive, and the form data (Simple MAPI on Windows, Mail on macOS, `xdg-email` on Linux; a
  `mailto:` message and the folder of the files when no mail program takes attachments).
- When no mail program takes attachments, an unsent e-mail draft (`.eml`) with the files attached
  is saved and opened with the program that reads e-mail files.
- PDF from e-mail messages: File › Open and Create › From E-mail open `.eml` files as documents
  with the subject, sender, recipients, date and text, and the attachments embedded.
- ZIP archives written and read by the core (`folitecta_core::zip`).
- Guides for AI agents in `ai/`: the target requirements, the state of each one, the planned
  releases, the workflow and the progress log.

### Changed
- Redaction takes the text and the pictures under the marks out of the page content and keeps
  the rest of the page as text and drawings (before, every marked page became a picture); the
  comments and form fields over the marks are removed with their values, and the next save
  rewrites the whole file so no earlier revision keeps the removed content. Pages whose content
  cannot be edited that way, or every marked page when chosen in the redaction settings, are
  drawn as pictures.
- The ribbon tools show a short name under their icons (Pencil, Loupe, Zoom +, Zoom −), on by
  default; View › Tool Names shows icons alone; the tooltips keep the whole name and the shortcut.
- Dragging the edge of a ribbon group follows the pointer (the number of tools comes from the
  real widths of the tools), and moves without a pressed button no longer disturb drags.

### Fixed
- A document opened from an encrypted file is saved encrypted again, with the same passwords and
  permissions; before, saving wrote it without protection. Save Unrestricted Copy removes the
  protection on purpose.
- Saving can no longer damage the original file: the document is written to a temporary file
  that replaces the original only once it is complete (the same for protected copies, signed
  files and the settings).
- Submenus no longer run away from the pointer: an open submenu waits while the pointer crosses
  other rows towards it, and clicking a row opens its submenu instead of closing it.

## [0.1.0] - 2026-10-18

### Added

- Text recognition (OCR), in pure Rust: OCR Pages reads all the pages, the current one or a range
  (leaving out the pages that already have text, if wanted), OCR Images only the pictures of the
  pages, the area OCR tool an area drawn on a page (its text shown to copy or to write on the
  page), and OCR Image makes a searchable document of picture files. Lines are found and read by
  PaddleOCR models (PP-OCRv4 detection, PP-OCRv5 Latin recognition: English, Italian and 30 more
  languages, slightly turned lines included) run by the tract inference library; the text is
  written invisibly over each word where the page shows it, also on turned pages, so the pages
  can be searched and their text selected and copied. Reading again replaces the text read
  before. Pages are read in the background, several lines at once, with a progress bar and Stop;
  the models are data files in `ocr/` with their licence, packaged next to the program.
- Spell checking with Hunspell dictionaries (English and Italian included as separate files in
  `dictionaries/`, each with its licence; any other Hunspell dictionary can be added). Spell
  Check and Replace (Review › Proofing) goes through the words of the page text, the comments
  and the text fields that the dictionaries of the languages chosen do not know, shows each one
  with the text around it and suggestions (the closest typing mistakes and the language of the
  text first), and changes it once or everywhere (one undo step), passes over it once or
  everywhere, or adds it to the user's own words. The words are marked on the page, as they are
  while the page text is edited with View › Spell Check on. Words in capitals, words with digits,
  addresses and file names are passed over; a word divided at the end of a line is checked whole.
- Quick Access Toolbar: the buttons next to the ribbon tabs (Open, Save, Undo, Redo) are now a
  toolbar of the user's own. A right click on a tool of the ribbon puts it there (or takes it
  away), and Customize Toolbars searches the commands to add, orders them, adds separators,
  shows each with its icon, its name or both, resets the toolbar, and puts it next to the tabs or
  in a row below the ribbon.
- Security policies (Protect › Security Policies): named sets of the permissions of password
  protection, two examples to start with, made, edited, cloned, renamed, deleted, imported and
  exported (JSON); applying one opens Password Protection with its permissions, and that dialog
  can choose a policy or save its permissions as a new one. Passwords are never kept.
- Items of combo boxes and list boxes edited in a list (Form › Items, or Edit Items in the field
  properties): added (Enter adds the item typed), deleted, moved up and down, or kept sorted
  (the field's Sort flag, items written in alphabetical order); written in one undo step.
- Pictures on push buttons: one for the normal, pressed and pointed-at states (the caption goes
  under the picture), chosen from picture files and removed in the field properties or with
  Form › Button › Up, Down and Roll Over Icon; and what the button does when clicked (nothing,
  invert the colours, invert the border, show it pressed).
- Groups of comments (Arrange › Group and Ungroup): the comments of a group are chosen with one
  click, moved, nudged and deleted together; groups are written the standard way (`/IRT` with
  `/RT /Group`), so groups made by other programs work too. Several comments chosen with Shift
  are also dragged, nudged and deleted together, in one undo step.

### Changed

- Libraries under the Mozilla Public License 2.0 are accepted next to the MIT, Apache-2.0 and
  GNU ones (`deny.toml`); option-ext, which Dioxus uses on Linux and macOS, is one.
- The full continuous integration runs for release tags, pull requests and on demand; the
  changes in between are checked locally before they are pushed.
- The release packages carry the `dictionaries` and `ocr` folders next to the program (in the
  resources of the macOS app bundle).
- The parts of the text recognition library that read the models are built for size.

## [0.0.12] - 2026-10-18

### Added

- Note icons: a note can be drawn as a comment bubble, a note, a key, help, a new paragraph, a
  paragraph, an insertion caret, a check mark, a circle, a cross or a star (Comments › Note Icon
  for the selected note, or for the new notes; also in the dialog of a new note). Notes read
  from other files keep their icon when they are drawn again.
- Arranging comments: Shift or Ctrl and a click adds a comment to the selected one (or takes it
  out), and the Arrange tab, shown while comments are selected, aligns them on an edge or a
  middle (with the selection, the page, the page as a group or the first comment chosen),
  spreads them evenly across or down (between the first and the last, or over the page), gives
  them the width, height or size of the first, and brings them to the front, to the back, or one
  step up or down. Turned pages are arranged as shown; every arrangement is one undo step.
- Thumbnails › Highlight Visible outlines, on the thumbnail of the current page, the part of the
  page the window shows, and follows it while the pages scroll.
- The loupe enlarges more or less (Zoom In and Zoom Out of its menu, from 1.5 to 8 times); the
  choice is kept.

### Changed

- The command modules of the interface and the modules of the core library are grouped in
  folders by area (documents, comments, text, forms, navigation, protection, window).
- The release workflow also publishes the release on GitLab when the repository has a
  `GITLAB_TOKEN` secret (packages of every platform, the same tag and notes).

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
