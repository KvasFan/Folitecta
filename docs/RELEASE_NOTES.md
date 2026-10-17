## Folitecta 0.0.12 — arrange comments, note icons

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- Arrange comments: Shift- or Ctrl-click to choose several, then align them (with the selection,
  the page or the first one chosen), spread them evenly, give them the same size, or bring them
  to the front or the back — from the Arrange tab shown while comments are selected. Each
  arrangement is one undo step, also on turned pages.
- Note icons: a comment bubble, note, key, help, paragraph marks, caret, check mark, circle,
  cross or star, for the selected note or for new notes; notes from other files keep theirs.
- Thumbnails can outline the part of the page the window shows; the loupe enlarges more or less.
- The code is organized by area, in the interface and in the core library.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.12-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.12-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.12-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
