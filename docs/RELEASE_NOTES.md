## Folitecta 0.0.2 — interactive editing and an adaptive interface

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- The interface fits any window size: the ribbon mixes large and stacked small buttons and
  collapses groups only when needed; panes, status bar, menus and dialogs adapt when the window
  is resized. A built-in self-test checks every tab, pane, menu and dialog at several sizes.
- Text selection with the mouse, copy, highlighted search results with navigation, and text
  highlight / underline / strikeout tools.
- Comments can be selected, moved, resized, nudged, copied between documents and flattened.
- New tools: snapshot, stamps (presets, custom text, pictures), redaction, image placement,
  typewriter, loupe; rulers, grid and guides with snapping.
- Redaction that destroys the marked content, password protection (AES-128) with permissions,
  script removal.
- More page tools: replace, overlay/underlay, split into tiles, rasterize, crop to content,
  normalise sizes, duplicates, drag-and-drop reordering, page labels, transitions, backgrounds.
- New documents from text files and the clipboard; real printing of document pages.
- Complete Italian translation of the interface.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.2-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.2-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.2-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
