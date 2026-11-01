## Folitecta 0.13.0 — buttons that work

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release makes the
push buttons and links of a document do what it says.

- **Scripts of buttons and links**: a press runs the script in the sandbox of the JavaScript console,
  with no files and no network, within its time limit. Scripts fill fields, move through the pages
  and show messages.
- **Chains of actions**: every action a button or link goes on to runs in order, where only the first
  one used to.

Everything 0.12.0 brought — Remove Cropped Content that really removes, templates exchanged, attached
files of inserted documents — and the earlier releases is described in their notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.13.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.13.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.13.0-macos-universal.zip` (app bundle) or `folitecta-0.13.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#0130---2026-11-01
[features]: https://github.com/KvasFan/Folitecta/blob/v0.13.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
