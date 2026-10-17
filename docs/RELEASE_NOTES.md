## Folitecta 0.0.11 — split window, field calculations

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- Window › Split: two views of a document, side by side or one above the other, each with its own
  page, zoom and layout. The outlined view is the one you work in; a click in the other one
  switches.
- Field calculations: a text field or combo box can be the sum, product, average, smallest or
  largest value of other fields, calculated whenever a value changes (in the same undo step) and
  written so other viewers calculate it too; Form › Set Field Calculation Order sets the order.
- Form › Highlight Fields by kind of field, or only the required ones.
- Links pane: select a link to see it on its page, copy or open its address from the pane menu,
  double-click to follow it. View › Expand All / Collapse All work on the visible pane.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.11-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.11-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.11-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
