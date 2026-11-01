## Folitecta 0.11.0 — right to left

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release brings
right-to-left text.

- **Hebrew drawn from right to left**: comments, stamps, tables, page marks, form fields and
  signatures draw Hebrew in its order, with brackets turned and fields aligned to the right. Its
  points are still refused.
- **Right-to-left text read in its order**: Hebrew and Arabic lines of any PDF came out reversed in
  search and copying. They are read in their order now, and text marks fall on the right letters.
- **Form values are kept**: a value Folitecta cannot draw, such as Arabic or Chinese, is no longer
  refused as it was in 0.9.0. It stays as typed, Folitecta warns and shows question marks, and other
  readers are asked to draw the field.

Everything 0.10.0 brought — names in every alphabet in signatures, media posters and redaction
codes — and the earlier releases is described in their notes and in [CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.11.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.11.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.11.0-macos-universal.zip` (app bundle) or `folitecta-0.11.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0110---2026-11-01
[features]: https://github.com/KvasFan/Folitecta/blob/v0.11.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
