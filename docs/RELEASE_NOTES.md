## Folitecta 0.45.0 — pictures in the office exports

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release holds the
work planned as 0.44.0 and 0.45.0.

- **Pictures in Word, rich text and web pages**: the export to DOCX, RTF and HTML carries the
  pictures of the pages, each where it stands among the paragraphs and at the size it has on the
  page. Photographs stay JPEG, other pictures become PNG, turned or mirrored ones are taken as the
  page shows them, and the web page holds them in itself, so it is one file.
- **Words marked as they are read**: while a page is read aloud, the word the voice reaches is
  marked on it (the voices of the system on Windows and macOS, speech-dispatcher on Linux).

Everything 0.43.0 brought — twenty milestones, from page labels that follow the pages to article
threads and presentations — and the earlier releases is described in their notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.45.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.45.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.45.0-macos-universal.zip` (app bundle) or `folitecta-0.45.0-macos-universal.tar.gz` (binary) |

Each package carries the `dictionaries`, `ocr` and `fonts` folders next to the program (in the
app bundle's resources on macOS): keep them with it. SHA-256 checksums are attached.

- **Linux** needs glibc 2.35 or newer and the WebKitGTK 4.1 runtime (`sudo apt install
  libwebkit2gtk-4.1-0` on Debian and Ubuntu, `sudo dnf install webkit2gtk4.1` on Fedora,
  `sudo pacman -S webkit2gtk-4.1` on Arch). Reading aloud uses speech-dispatcher with a voice,
  scanning uses SANE's `scanimage`, and cards and tokens use p11-kit and the card's PKCS #11
  module; all are optional.
- **Windows**: the program is not code-signed, so Windows may warn before the first start.
- **macOS** builds are not notarized: the first time, right-click → Open (on macOS 15 and later,
  System Settings › Privacy & Security › Open Anyway).

The status of every command is in [docs/FEATURES.md][features]; known limitations are in
[docs/ARCHITECTURE.md][limits] and [ai/ROADMAP.md][roadmap].

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0450---2026-11-05
[features]: https://github.com/KvasFan/Folitecta/blob/v0.45.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
