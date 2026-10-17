# Changelog

All notable changes to Folitecta are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- History pane: documents opened before, grouped by day, pinned ones first, sorted by opening,
  closing or modification time or by name, with thumbnails, paths and a preview on hover; open
  at the page where a document was closed, pin, remove, copy the path, open the folder.
- Sessions: save the open documents as a session, open a session instead of the open documents
  or next to them or in a new window, rename, remove, sort, export and import sessions as files.

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
