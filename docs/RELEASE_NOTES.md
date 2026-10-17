## Folitecta 0.0.9 — resizable interface, readable themes, Find and Replace, recovery

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- Sizes you choose: drag the edge of the left or right pane to change its width, the bottom
  edge of the ribbon for one to six rows of tools, the right edge of a ribbon group to show more
  of its tools (all of them, without the More menu). A double click restores the automatic size.
- Themes: light, white, dark and black; the dark theme is readable everywhere, and the chosen
  tool stands out clearly in every theme.
- New icons for every command (Tabler Icons, MIT).
- The first ribbon tab is called Edit, with Text (Edit Text, Insert, Replace, Delete Text, Find,
  Find and Replace) and Clipboard groups.
- Find and Replace in the text of the pages: one occurrence at a time or all of them in one undo
  step.
- Replace a picture with a file or the clipboard; links that go to a named destination, open a
  file or run a script; Populate Forms from a CSV table; separators for measurements.
- Document Recovery: documents with unsaved changes are copied every minute and offered again
  after the window ended without saving them.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.9-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.9-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.9-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
