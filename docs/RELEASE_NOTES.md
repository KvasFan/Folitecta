## Folitecta 0.4.0 — professional tools

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release brings
tools for preparing documents for print, archive and signature.

- **PDF/A**: Preflight PDF/A… checks a document against 18 rules of PDF/A-2b and lists each
  problem with its page; Convert to PDF/A… fixes what it can in a copy (with an sRGB output
  intent) and declares PDF/A-2b only when no rule checked is left broken. Saving keeps the XMP
  metadata in step with the document information.
- **Colours to grey and grey preview**: the colours of pages, drawings, pictures, comments and
  fields become grey in one undo step, after a report of what would change and what would stay;
  View › Grey Preview shows the pages in grey without changing anything.
- **Recompress pictures** of the document, the page or one picture, with presets (screen,
  e-book, print, archive) and an estimate picture by picture before anything changes.
- **Edit Paths** (Edit Content › Shapes): the vector paths a page draws chosen with a click,
  their nodes dragged, their colours and line width changed, or deleted, in the page content.
- **Action sequences** (Tools › Action Sequences…): named sequences of steps run on the open
  document as one undo step, or on files and folders with a result and a log for every file,
  and from the command line with `folitecta --run-actions`.
- **Export to Word, RTF and HTML**: the text with its headings and paragraphs in reading order.
- **JavaScript**: a sandboxed engine (no network, files or programs in reach, stopped when a
  script runs too long) runs the console and the scripts of form fields; calculate, format and
  validate scripts can be typed in a field's properties, and the format, range and calculation
  functions of forms work in opened documents.
- **Cards, tokens and the system store**: digital IDs on smart cards and tokens (through their
  PKCS #11 module; on Linux the modules the system has registered), in the certificate store of
  Windows and in the keychain of macOS sign documents; their keys never leave them and the PIN
  is never kept. These have been tried with a stand-in card only: reports from real cards are
  welcome.

The full list, with the fixes, is in [CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.4.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.4.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.4.0-macos-universal.zip` (app bundle) or `folitecta-0.4.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#040---2026-10-26
[features]: https://github.com/KvasFan/Folitecta/blob/v0.4.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
