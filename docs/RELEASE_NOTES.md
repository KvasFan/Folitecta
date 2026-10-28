## Folitecta 0.6.0 — safe edits

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release makes
page operations safe: what ties a document together — bookmarks, links, named destinations, form
fields, comments and tags — now follows the pages when they leave, arrive or are copied, and
Folitecta says before OK what it cannot keep.

- **Deleted pages really go**: deleting, extracting or splitting pages takes away the bookmarks,
  named destinations and links that lead to the pages that go, their form fields and tags, and
  the opening view when it showed one of them. Before, a page a bookmark or a tag still pointed
  to stayed inside the saved file; now nothing of it is left. Delete Pages names what goes with
  the pages and asks first.
- **Inserted pages bring their parts**: Insert Pages from a file and Combine Files bring the form
  fields of those pages into the form, with their named destinations, bookmarks, layers and —
  when the document is tagged — their tags. A field or destination name the document already has
  gets a suffix (`name_2`), the links follow, and the insert window lists the renames before OK.
  Combine Files puts each file under a bookmark named after it.
- **Duplicated pages are copies of their own**: a note on a copy keeps its pop-up and its replies;
  the fields of a copy are more widgets of the same fields, so both pages show the same values.
- **Exports written whole**: pictures, text, attachments, form data, comments, Office documents,
  digital IDs and settings go to a temporary file first and replace the target only when
  complete, as saving a PDF already did.

Everything 0.5.0 brought — the quick panels of the drawing and measuring tools and pen pressure
for the Pencil — and the earlier releases is described in their notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.6.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.6.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.6.0-macos-universal.zip` (app bundle) or `folitecta-0.6.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#060---2026-10-28
[features]: https://github.com/KvasFan/Folitecta/blob/v0.6.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
