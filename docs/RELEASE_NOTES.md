## Folitecta 0.8.0 — documents you can trust

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release is about
trust: a protected file keeps everything encrypted and Folitecta keeps to its security settings,
what reaches outside a document asks first, and redaction also takes what is hidden beyond the
visible page.

- **Permissions kept**: opened with the password to open it, a file whose security settings
  forbid changing it, commenting, filling in forms, assembling pages or copying refuses a change
  of that kind and changes nothing. The owner password, asked right there, lifts the restrictions
  until the document is closed.
- **All of a protected file encrypted**: the strings in the dictionaries of streams, such as the
  dates of an attached file, stayed in plain text. They are encrypted now, and files written
  before still open.
- **Links and buttons that work, safely**: links on the page and push buttons are followed with
  the hand and select tools. A page or destination opens at once; a web address, a file or another
  document is named and asked first, with a choice to trust the document until it is closed.
  Programs are never started from a document, and form data is never sent: the button says where
  it would go.
- **Redaction beyond the visible page**: applying marks also takes the text, pictures and
  drawings of the marked pages that lie beyond the crop box, hidden from view but still in the
  file. Apply Redactions names those pages, and the choice can be turned off.

Everything 0.7.0 brought — a date field with a calendar, formats and checks chosen in a field's
properties, the user's own templates — and the earlier releases is described in their notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.8.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.8.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.8.0-macos-universal.zip` (app bundle) or `folitecta-0.8.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#080---2026-10-31
[features]: https://github.com/KvasFan/Folitecta/blob/v0.8.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
