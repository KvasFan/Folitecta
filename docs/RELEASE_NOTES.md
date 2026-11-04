## Folitecta 0.43.0 — twenty milestones

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release holds the
work planned as 0.24.0 to 0.43.0, published together.

- **Page labels**: they follow the pages when pages are inserted, moved or deleted, every page range
  takes them (`iii-v`, `"A-1"`), and an inserted table of contents is numbered with them.
- **Scans**: Clean Up Scans turns pages upside down or sideways upright, removes specks and makes
  pages black and white; Interleave Fronts and Backs puts two stacks of scans in order.
- **Words read with doubt** by text recognition join the spell check, and the search can keep only
  the results on them.
- **History of Changes**: every undo step named after what made it, with a click back or forward to
  any of them.
- **Presentation**: the pages one at a time on the whole screen, with the transitions they set and
  going on by themselves after their time.
- **Articles**: the article threads of a document listed and followed, drawn with an Article tool,
  edited, and kept with the pages of an inserted file.
- **Accessibility**: the check fixes tables without header cells, rows of different lengths, and
  asks a summary for each table.
- **And**: tabs dragged between groups, duplicate bookmarks deleted, Flatten Comments by kind and
  author, reading aloud from a paragraph double-clicked, the depth of a volume measurement changed,
  saved sets of tick styles, and the Fonts pane listing only the fonts the pages use.

Everything 0.23.0 brought — Symbol, ZapfDingbats and composite fonts for PDF/A, scanning into the
open document, recognition of folders, Table Options and template groups — and the earlier releases
is described in their notes and in [CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.43.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.43.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.43.0-macos-universal.zip` (app bundle) or `folitecta-0.43.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0430---2026-11-04
[features]: https://github.com/KvasFan/Folitecta/blob/v0.43.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
