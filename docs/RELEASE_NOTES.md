## Folitecta 0.7.0 — forms that check themselves

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release gives
form fields formats and checks chosen instead of written as scripts, a date field with a
calendar, and templates of the user's own.

- **A date field**: Forms › Date makes a text field that takes only dates, in the format of the
  language (`dd/mm/yyyy` in Italian, `mm/dd/yyyy` in English, any other chosen in its
  properties). Filling it offers a calendar; a date that does not exist (30 February) is
  refused, naming the format, and the field keeps what was typed so it can be corrected.
- **Formats and checks chosen, not written**: the properties of a text field offer a number with
  its decimals, separators and currency, a percentage, a date, a time, a postal code or a phone
  number, and the lowest and highest value of a number. They are kept as the standard scripts of
  forms, so other readers apply them too; scripts written by hand stay as they are.
- **Values checked when committed**: a word in a number field, a postal code of the wrong length
  or a date that does not exist is refused, as the format's script says. The form scripts know
  dates and times (`AFDate_*`, `AFTime_*`, `util.printd` and `util.scand`, which reads dates as
  people type them, with English and Italian month names).
- **Your own templates**: File › Save as Template… keeps a document under a name. The gallery of
  File › New › From Template… lists them under My templates with their pictures, where they come
  from, when and with which version they were made and the fonts they need; it makes new
  documents from them (the template never changes), renames them and moves them to the bin.
- **Fixed**: scripts typed into a field's properties lost characters beyond plain ASCII (a € in a
  format) when they were read back.

Everything 0.6.0 brought — page operations that keep bookmarks, links, destinations, fields and
tags with their pages — and the earlier releases is described in their notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.7.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.7.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.7.0-macos-universal.zip` (app bundle) or `folitecta-0.7.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#070---2026-10-30
[features]: https://github.com/KvasFan/Folitecta/blob/v0.7.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
