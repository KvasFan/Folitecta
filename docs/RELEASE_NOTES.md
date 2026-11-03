## Folitecta 0.23.0 — choices of one's own

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release holds the
work planned as 0.22.0 and 0.23.0, published together.

- **More fonts for PDF/A**: Symbol and ZapfDingbats are embedded (in FreeSerif, with a map back to
  their characters, so their text reads as Greek letters, symbols and dingbats), and so are
  composite fonts with Identity-H and a ToUnicode map, with their widths kept. A font is embedded only
  when its substitute draws every code the document shows; checked against veraPDF.
- **Scanning into the open document**: the scanning window can put the pages after the current page,
  and a scan under way can be stopped, keeping the sheets the feeder gave.
- **Text recognition on folders**: action sequences have a Recognize Text step, for folders of files
  and the command line.
- **Table Options**: a table opened from a CSV file is made again with the separator, the first row,
  the paper, the turn of the pages and the size of the text chosen.
- **Templates in groups**: templates of one's own go in groups of the gallery, and a search box finds
  templates.

Everything 0.21.0 brought — fonts embedded for PDF/A and copies checked against an independent
validator — and the earlier releases is described in their notes and in [CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.23.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.23.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.23.0-macos-universal.zip` (app bundle) or `folitecta-0.23.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0230---2026-11-03
[features]: https://github.com/KvasFan/Folitecta/blob/v0.23.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
