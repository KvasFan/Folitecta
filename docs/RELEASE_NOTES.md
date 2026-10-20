## Folitecta 0.2.1 — printing, protection and smoother pages

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release brings
a print window of its own, keeps protected documents encrypted through signing, removes more of
what lies under redaction marks, and makes scrolling and zoom smoother on large documents.

**Highlights**

- **Printing, redone**: a print window with a live preview of the sheets and the printers of the
  computer. Copies, ranges by number or page label, odd/even, reverse; fit, actual size, shrink or
  custom scale, paper by page size; 2 to 16 pages per sheet, booklets and posters; two-sided
  printing and colour. Jobs go straight to the printer: a vector print file through the local
  print server on Linux and macOS, sheets as pictures up to 600 dpi on Windows.
- **Smoother pages**: scrolling no longer waits for the program, only the pages near the window
  are kept, the visible ones are drawn first on several threads, and zoom scales the pictures at
  once and sharpens them when it stops. On a 1000-page document the longest pause while scrolling
  fell from about 0.4 s to about 40 ms, and six Ctrl+wheel steps are sharp after about 0.12 s
  instead of 0.5 s.
- **Protection that stays**: new passwords use AES-256 (AES-128 is still a choice). Signing,
  certifying and time-stamping a protected document keep it encrypted, and documents made from a
  protected one are protected like it. Passwords with accents or other non-ASCII characters open
  older AES-128 and RC4 files correctly.
- **Revocation checked**: signature certificates are checked against OCSP answers and revocation
  lists in the file, or fetched online once you turn that on; Add Validation Material stores them
  in the document so the signatures can be checked later.
- **Redaction removes more**: vector drawings under the marks and the content of reused drawings
  are removed instead of drawing the page as a picture, and a dialog offers to sanitize the
  document (metadata, attachments, hidden content, quotes of the removed words, scripts) before a
  checked new file is written.
- **Text beyond Western European letters**: text boxes, typewriter text, callouts and measurement
  labels write Polish, Czech, Greek, Cyrillic and many symbols with an embedded subset of GNU
  FreeFont FreeSans. What cannot be drawn yet is refused, never replaced by `?`.
- **E-mail attachment dialog**: choose for each document the current state or the saved file,
  some pages, comments kept, flattened or removed, form data and comments as separate files or one
  ZIP, with sizes and previews; save an unsent draft or open the mail program.
- **Drawing on layers**: what you draw or write goes to the layer selected in the Layers pane, and
  hiding a layer hides its comments everywhere.

**Not yet verified**: printing on real printers, sending through real mail programs, public time
stamp and revocation servers. The tests use stand-ins for all three; reports are welcome.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.2.1-windows-x64.zip` |
| Linux x64 | `folitecta-0.2.1-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.2.1-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Each package carries the `dictionaries`, `ocr` and `fonts` folders next to the program (in the
app bundle's resources on macOS): keep them with it. Linux needs the WebKitGTK 4.1 runtime
(`libwebkit2gtk-4.1-0`). macOS builds are not notarized: right-click → Open on first launch.
SHA-256 checksums are attached.

The full list of changes is in `CHANGELOG.md`; the status of every command is in
`docs/FEATURES.md`; known limitations are in `docs/ARCHITECTURE.md`.
