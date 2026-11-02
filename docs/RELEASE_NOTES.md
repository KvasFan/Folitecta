## Folitecta 0.20.0 — cleaner scans

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release cleans up
more of what a scanner leaves behind.

- **Dark borders**: Clean Up Scans finds the dark edges of scanned pages, told from lines of text and
  from pages dark all over, and paints them the colour of the paper. A page can be whitened and
  straightened in the same step.
- **Doubtful words**: the certainty below which a recognized word is kept for review is chosen in the
  recognition window.

Everything 0.19.0 brought — bleed, trim and art boxes, reading from a paragraph — and the earlier
releases is described in their notes and in [CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.20.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.20.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.20.0-macos-universal.zip` (app bundle) or `folitecta-0.20.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0200---2026-11-02
[features]: https://github.com/KvasFan/Folitecta/blob/v0.20.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
