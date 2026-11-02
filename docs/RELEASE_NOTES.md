## Folitecta 0.21.0 — fonts for PDF/A

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release makes
Convert to PDF/A produce files an independent validator accepts.

- **Fonts embedded**: the fonts a document names without carrying them — Helvetica, Times, Courier,
  Arial and other simple fonts — become subsets of GNU FreeFont FreeSans, FreeSerif or FreeMono of
  the same style, with the widths the text is laid out with, so no line moves. Documents made from
  plain text are now declared PDF/A-2b. The packages carry the twelve FreeFont faces.
- **What PDF/A asks of comments and forms**: appearances are drawn for comments and form fields
  that have none, buttons get their appearance as a state and lose their actions.
- **Checked by an independent validator**: compared with veraPDF on seventeen kinds of document, the
  preflight missed text painted in the initial grey and fonts written inside resources, so some
  copies were declared PDF/A-2b wrongly. Fixed: every copy declared PDF/A-2b now passes veraPDF.
- **Tick marks** are counted by shape in their window.

Composite fonts, Symbol and ZapfDingbats that are not embedded are left as they are: the copy is
converted but not declared PDF/A. Everything 0.20.0 brought — dark borders of scans whitened, the
certainty of doubtful words chosen — and the earlier releases is described in their notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.21.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.21.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.21.0-macos-universal.zip` (app bundle) or `folitecta-0.21.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0210---2026-11-02
[features]: https://github.com/KvasFan/Folitecta/blob/v0.21.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
