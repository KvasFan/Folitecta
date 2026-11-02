## Folitecta 0.16.0 — what the file holds

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release shows and
removes what a file holds out of sight.

- **Hidden content on every page**: Apply Redactions offers to take out what lies beyond the crop box
  of the pages without marks too, chosen by default; what shows on them does not change.
- **Fonts pane**: the fonts of pages, forms and comments, with their kind, whether the file holds them
  and the pages they appear on. Fonts that other computers would replace are said.
- **Tabs keep their page**: a document no longer takes the scroll position of the tab shown before it
  when another tab closes or is chosen.

Everything 0.15.0 brought — exports that keep to the copy permission, a protection that stays without
the owner password — and the earlier releases is described in their notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.16.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.16.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.16.0-macos-universal.zip` (app bundle) or `folitecta-0.16.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0160---2026-11-02
[features]: https://github.com/KvasFan/Folitecta/blob/v0.16.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
