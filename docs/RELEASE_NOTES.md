## Folitecta 0.4.1 — fixes

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release follows
0.4.0 with two changes asked for after it.

- **Export to Word, RTF and HTML works again**: in 0.4.0, Export to Office Formats… closed its
  dialog and wrote nothing. The file is now written where you choose.
- **Shape tools stay chosen**: the rectangle, ellipse, line, arrow, polygon, polyline and cloud
  tools stay chosen after a shape, like the pencil, so several shapes are drawn one after the
  other; Escape goes back to the hand.

Everything 0.4.0 brought — PDF/A preflight and conversion, grey conversion and preview, picture
recompression, vector path editing, action sequences, office exports, a sandboxed JavaScript
engine, signing with cards, tokens and the system store — is described in its notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.4.1-windows-x64.zip` |
| Linux x64 | `folitecta-0.4.1-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.4.1-macos-universal.zip` (app bundle) or `folitecta-0.4.1-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#041---2026-10-26
[features]: https://github.com/KvasFan/Folitecta/blob/v0.4.1/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
