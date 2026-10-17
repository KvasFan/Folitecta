## Folitecta 0.0.6 — calculator, JavaScript console and media

A free PDF editor written entirely in Rust with a Dioxus desktop interface.

**Highlights**

- Accounting calculator: tapes of amounts placed on the page as comments, typed on a keypad,
  with running totals, comments on each line, decimals, rounding and their own look.
- JavaScript console: run scripts against the open document (`this.getField(name).value`,
  `this.pageNum`, `this.info`, `app.alert`, `console.println`) in a worker apart from the window
  with a time limit; snippets kept as files; named document scripts edited and run.
- Sound and video: sounds from WAV files and embedded audio or video files played in a docked or
  floating media player; media of other documents plays too.
- Files and metadata: rename, move or recycle the document file; metadata templates; the XMP
  properties of a document and XMP export.
- Guides exported, imported and copied to every page; a continuous zoom tool.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.0.6-windows-x64.zip` |
| Linux x64 | `folitecta-0.0.6-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.0.6-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Linux needs the WebKitGTK 4.1 runtime (`libwebkit2gtk-4.1-0`). macOS builds are not notarized:
right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
