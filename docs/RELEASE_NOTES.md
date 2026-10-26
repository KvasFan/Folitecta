## Folitecta 0.3.0 — daily work

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release is
about everyday work: documents read aloud and reflowed, tabs in two groups, new documents from
templates, a scanner and Markdown or CSV files, scanned pages cleaned up and their recognized
text checked, tables of contents, and comparisons that scroll together.

**Highlights**

- **Read aloud and reflow**: View › Read Aloud reads the document paragraph by paragraph with the
  voices of the system, marking the paragraph being read on its page; pause, go back or forward,
  choose the voice and the speed. The reflow view shows the text in the width of the window, at
  the size you choose. On Linux the voices come from speech-dispatcher.
- **Tab groups and windows**: put a document in a second group of tabs beside the others or
  below them, each group with its own tab strip; move tabs between groups, share them out or
  gather them again. Move to New Window opens a saved document in another window.
- **New documents from templates**: File › From Template… has ruled, squared, dotted and graph
  paper, music staves, Cornell notes and handwriting lines, week and month planners, a to-do list,
  a letter, meeting notes and an invoice that adds up its own amounts, on the paper you choose
  and in the language of the interface.
- **Scanning**: File › From Scanner… makes a document of scanned pages, each the size of the
  paper. On Linux choose the scanner, source, colours and resolution (through SANE) and save them
  as presets; on Windows the system's scanning window is used. Sheets printed on both sides can be
  scanned as two stacks, the backs put back between the fronts.
- **Markdown and CSV**: open a Markdown file as a document with its headings (as bookmarks),
  lists, quotes, code, tables and links, or a CSV file as a table with its header on every page.
- **Scans cleaned up, recognized text checked**: Clean Up Scans finds blank and askew scanned
  pages and deletes or straightens them in one step. After text recognition, the words it was
  unsure of are shown one by one with their picture, to correct, keep or remove.
- **Tables of contents**: bookmarks made from the headings found in the text, and a contents page
  inserted from the bookmarks, with dotted leaders, page numbers and a link for every entry.
- **Comparison side by side**: the old document follows the new one page for page as it scrolls.
- **More to measure and mark**: angles, radii and diameters, and volumes (an area times a depth);
  tick marks placed with a click (checks, crosses, dots, numbers, letters or roman numerals going
  on by one, plain or circled).
- **Text beyond Western European letters** in text stamps, designed stamps, tables, calculator
  tapes, watermarks, headers and footers, page and Bates numbers, comment summaries and documents
  made from text, with an embedded subset of GNU FreeFont FreeSans.
- **Packages**: the Linux program no longer needs libxdo; the Windows program no longer needs the
  Visual C++ Redistributable and shows its icon and version; the macOS app is signed ad hoc as a
  whole bundle; every package carries the licences of the libraries built into it
  (`THIRD-PARTY-LICENSES.txt`).

**Not yet verified**: reading aloud with real voices (the tests use a stand-in engine), scanning
with real scanners, printing on real printers, sending through real mail programs, public time
stamp and revocation servers, and the first start of the Windows and macOS packages on a real
machine (they are built and unit-tested by the CI only). Reports are welcome.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.3.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.3.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.3.0-macos-universal.zip` (app bundle) or `folitecta-0.3.0-macos-universal.tar.gz` (binary) |

Each package carries the `dictionaries`, `ocr` and `fonts` folders next to the program (in the
app bundle's resources on macOS): keep them with it. SHA-256 checksums are attached.

- **Linux** needs glibc 2.35 or newer and the WebKitGTK 4.1 runtime (`sudo apt install
  libwebkit2gtk-4.1-0` on Debian and Ubuntu, `sudo dnf install webkit2gtk4.1` on Fedora,
  `sudo pacman -S webkit2gtk-4.1` on Arch). Reading aloud uses speech-dispatcher with a voice
  (for example `espeak-ng`) and scanning uses SANE's `scanimage`; both are optional.
- **Windows**: the program is not code-signed, so Windows may warn before the first start.
- **macOS** builds are not notarized: the first time, right-click → Open (on macOS 15 and later,
  System Settings › Privacy & Security › Open Anyway). Scanning is not available on macOS yet.

The full list of changes is in [CHANGELOG.md][changelog]; the status of every command is in
[docs/FEATURES.md][features]; known limitations are in [docs/ARCHITECTURE.md][limits] and
[ai/ROADMAP.md][roadmap].

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#030---2026-10-26
[features]: https://github.com/KvasFan/Folitecta/blob/v0.3.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
