## Folitecta 0.1.0 — text recognition, spell checking, quick access toolbar

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This is the first
larger release after the 0.0.x series.

**Highlights**

- **Text recognition (OCR)** in pure Rust: scanned pages (all, the current one, a range, or only
  the pictures in them) are read and their text is written invisibly over every word, so they
  can be searched, selected and copied; an area of a page can be read and its text copied; picture
  files become a searchable document. English, Italian and 30 more languages of the Latin script,
  slightly turned lines and turned pages included; pages are read in the background with a
  progress bar.
- **Spell checking** with Hunspell dictionaries (English and Italian included, any other can be
  added): Spell Check and Replace goes through the page text, the comments and the form fields,
  with suggestions in the language of the text, change or ignore once or everywhere, and your own
  words; misspelled words are marked while the page text is edited.
- **Quick Access Toolbar**: right-click any tool of the ribbon to keep it next to the tabs (or in
  a row below the ribbon); Customize Toolbars adds, orders and separates commands.
- **Groups of comments**: comments grouped together are chosen with one click and moved,
  nudged and deleted together; several comments chosen with Shift move together too.
- **Forms**: the items of combo boxes and list boxes edited as a list (sorted or in any order);
  pictures on push buttons for the normal, pressed and pointed-at states, and what a click does.
- **Security policies**: named sets of the permissions of password protection, imported and
  exported, applied in one step (passwords are never kept).

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.1.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.1.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.1.0-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Each package carries the `dictionaries` and `ocr` folders next to the program (in the app
bundle's resources on macOS): keep them with it. Linux needs the WebKitGTK 4.1 runtime
(`libwebkit2gtk-4.1-0`). macOS builds are not notarized: right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
