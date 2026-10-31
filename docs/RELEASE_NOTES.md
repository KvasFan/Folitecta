## Folitecta 0.9.0 — text in every field

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release keeps the
text people type into forms what they typed, and gives files of sessions a version.

- **Form fields in more alphabets**: values, list items and button captions beyond Western European
  letters — Polish, Greek, Cyrillic and others — are drawn with the embedded font instead of
  question marks. Text that cannot be drawn is refused, named, and stays in the editor to be
  corrected; imported form data with such text changes nothing. Right-to-left text in a field is
  refused for now.
- **Files of sessions with a version**: exported sessions say their format and version. Earlier
  files still open, files of a later version are refused with a message, and other JSON files are
  no longer imported as empty sessions.

Everything 0.8.0 brought — permissions kept, all of a protected file encrypted, links and buttons
that ask first, redaction beyond the visible page — and the earlier releases is described in their
notes and in [CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.9.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.9.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.9.0-macos-universal.zip` (app bundle) or `folitecta-0.9.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#090---2026-10-31
[features]: https://github.com/KvasFan/Folitecta/blob/v0.9.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
