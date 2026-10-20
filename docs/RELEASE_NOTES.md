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
  instead of about 0.57 s. On Linux the interface also uses less memory than 0.2.0.
- **Protection that stays**: new passwords use AES-256 (AES-128 is still a choice). Signing,
  certifying and time-stamping a protected document keep it encrypted, and documents made from a
  protected one are protected like it. Passwords with accents or other non-ASCII characters open
  older AES-128 and RC4 files correctly.
- **Revocation checked**: signature certificates are checked against OCSP answers and revocation
  lists in the file, or fetched online once you turn that on; Add validation material… stores them
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
  hiding a layer hides its comments on the pages, thumbnails, prints, comment summaries and in the
  Comments pane (pictures exported from the pages follow the document's default layer
  visibility).

**Not yet verified**: printing on real printers, sending through real mail programs, public time
stamp and revocation servers (the tests use stand-ins for all three), and the first start of the
Windows and macOS packages on a real machine (they are built and unit-tested by the CI only).
Reports are welcome.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.2.1-windows-x64.zip` |
| Linux x64 | `folitecta-0.2.1-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.2.1-macos-universal.zip` (app bundle) or `folitecta-0.2.1-macos-universal.tar.gz` (binary) |

Each package carries the `dictionaries`, `ocr` and `fonts` folders next to the program (in the
app bundle's resources on macOS): keep them with it. SHA-256 checksums are attached.

- **Linux** needs glibc 2.35 or newer, the WebKitGTK 4.1 runtime and libxdo 3 (`sudo apt install
  libwebkit2gtk-4.1-0 libxdo3` on Debian and Ubuntu, `sudo dnf install webkit2gtk4.1 libxdo` on
  Fedora). Arch, Manjaro and CachyOS have only libxdo 4: see
  [Linux libraries](https://github.com/KvasFan/Folitecta#linux-libraries) for a compatibility link.
- **Windows**: if `VCRUNTIME140.dll` is reported missing, install the latest Microsoft Visual
  C++ Redistributable for x64 from Microsoft.
- **macOS** builds are not notarized: the first time, right-click → Open (on macOS 15 and later,
  System Settings › Privacy & Security › Open Anyway).

The full list of changes is in [CHANGELOG.md][changelog]; the status of every command is in
[docs/FEATURES.md][features]; known limitations are in [docs/ARCHITECTURE.md][limits] and
[ai/ROADMAP.md][roadmap].

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#021---2026-10-20
[features]: https://github.com/KvasFan/Folitecta/blob/v0.2.1/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
