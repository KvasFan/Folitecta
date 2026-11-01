## Folitecta 0.12.0 — nothing left behind

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release makes
commands do all they promise.

- **Remove Cropped Content removes it**: the command only clipped the pages, so what lay outside the
  crop box stayed in the saved file. It now takes that text, those pictures and drawings out of the
  content, as redaction does, after asking; digital signatures go with it.
- **Templates exchanged**: the gallery imports a template or any PDF file, and exports a template to
  a file.
- **Attached files come along**: inserting all the pages of a file brings the files attached to it,
  renamed when their name is already used. Taking some pages leaves them, and the window says so.

Everything 0.11.0 brought — right-to-left text drawn and read in its order, form values kept — and the
earlier releases is described in their notes and in [CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.12.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.12.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.12.0-macos-universal.zip` (app bundle) or `folitecta-0.12.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0120---2026-11-01
[features]: https://github.com/KvasFan/Folitecta/blob/v0.12.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
