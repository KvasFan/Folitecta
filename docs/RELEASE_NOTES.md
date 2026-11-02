## Folitecta 0.19.0 — boxes and reading

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release prepares
pages for the printer and reads from where one looks.

- **Page boxes**: Page Boxes… lists the sheet, visible page, bleed, trim and art boxes of a page, and
  sets or removes the last three on a range of pages. Show All Boxes draws them on the pages. What a
  page shows does not change.
- **Reading from a paragraph**: a double click on a paragraph of the reflow view reads aloud from
  there, also while reading.
- **Crop**: a crop that would leave nothing no longer leaves an edit open.

Everything 0.18.0 brought — values from a file checked by their fields, values too long said — and the
earlier releases is described in their notes and in [CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.19.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.19.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.19.0-macos-universal.zip` (app bundle) or `folitecta-0.19.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0190---2026-11-02
[features]: https://github.com/KvasFan/Folitecta/blob/v0.19.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
