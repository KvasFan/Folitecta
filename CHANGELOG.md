# Changelog

All notable changes to Folitecta are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

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
