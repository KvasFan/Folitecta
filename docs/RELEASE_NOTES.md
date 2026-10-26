## Folitecta 0.3.1 — shorter menus

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release follows
0.3.0 with two changes asked for after it.

- **Shorter menus**: each menu of the menu bar keeps the commands used most at the top (at most
  22 rows, where File and Edit had more than 80 and 110) and puts the others in submenus by
  subject — File › New, Save in Other Ways, History and Sessions, File on Disk, Send and Share,
  Places; Edit › Paste Special, Select, Page Text, Arrange, Format, Pictures and Objects,
  Layers, Interface; and so on in View, Document, Comments, Tools and Window. The long
  submenus (Tools › Tool, Annotation, the macros, the Bookmarks and Layers pane menus) are split
  the same way, also in the ribbon dropdowns and the pane menus.
- **Pencil pointer on Linux**: on HiDPI screens it was twice as large as the other pointers
  (WebKitGTK up to 2.54 enlarges SVG pointers twice); it is now a picture that keeps its size.

Everything 0.3.0 brought — reading aloud and reflow, tab groups, templates, scanning,
Markdown and CSV documents, scan clean-up and OCR review, tables of contents, comparison side
by side, angle, radius and volume measures, tick marks — is described in its notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.3.1-windows-x64.zip` |
| Linux x64 | `folitecta-0.3.1-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.3.1-macos-universal.zip` (app bundle) or `folitecta-0.3.1-macos-universal.tar.gz` (binary) |

Each package carries the `dictionaries`, `ocr` and `fonts` folders next to the program (in the
app bundle's resources on macOS): keep them with it. SHA-256 checksums are attached.

- **Linux** needs glibc 2.35 or newer and the WebKitGTK 4.1 runtime (`sudo apt install
  libwebkit2gtk-4.1-0` on Debian and Ubuntu, `sudo dnf install webkit2gtk4.1` on Fedora,
  `sudo pacman -S webkit2gtk-4.1` on Arch). Reading aloud uses speech-dispatcher with a voice
  and scanning uses SANE's `scanimage`; both are optional.
- **Windows**: the program is not code-signed, so Windows may warn before the first start.
- **macOS** builds are not notarized: the first time, right-click → Open (on macOS 15 and later,
  System Settings › Privacy & Security › Open Anyway).

The status of every command is in [docs/FEATURES.md][features]; known limitations are in
[docs/ARCHITECTURE.md][limits] and [ai/ROADMAP.md][roadmap].

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#031---2026-10-26
[features]: https://github.com/KvasFan/Folitecta/blob/v0.3.1/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
