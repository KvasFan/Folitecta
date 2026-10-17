## Folitecta 0.0.8 — page text editing, redaction overlays and comment styles

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- Editing the text of the page, in the Edit group of Home: the Edit Text tool puts a caret in the
  text; typing, Delete and Backspace, Insert Text, Replace Text and Delete Text change the text
  drawn on the page and move the rest of the line to make room.
- The comment tools stay comments: Insert Text, Replace Text and Text Correction in the Comments
  tab suggest the change with a comment and leave the page as it is.
- Shapes are shown as they will look while they are drawn (rectangles, ellipses, highlights, text
  boxes, lines, arrows) with the style of their tool.
- Pencil: a pencil cursor, smooth curves with at least twice as many pieces, and the tool stays
  chosen for the next stroke.
- Redaction overlays: a colour, blur or pixelation over the redacted areas, with overlay text and
  sets of redaction codes.
- Comment Styles pane: named styles for each kind of comment, a palette over the page to pick them,
  and default styles reset.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.8-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.8-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.8-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
