## Folitecta 0.5.0 — drawing at hand

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release makes
drawing and measuring quicker: the tools now carry their own small panel over the pages, and the
Pencil follows the pressure of a pen.

- **A quick panel for every drawing tool**: choosing the Pencil, the free highlighter, a shape
  (rectangle, ellipse, line, arrow, polygon, polyline, cloud) or a measuring tool opens a small
  panel over the pages with the thickness of the line, four colours and the colour palette for
  any other. Closed shapes also get their fill. What you choose there is used by the next
  comments of that tool, and the panel goes with another tool.
- **Measuring at hand**: the panel of the distance, perimeter, area, angle, radius and volume
  tools shows the scale of the measures and opens it with a click; the measuring tools stay
  chosen, so several measures are taken one after the other.
- **Pen pressure**: with a graphics tablet or a stylus that tells how hard it presses, the
  Pencil's stroke is thinner where the pen presses lightly and thicker where it presses hard. It
  shows so while you draw, is saved as an appearance every reader shows, and keeps its pressures
  when it is moved, restyled or partly erased. A button of the Pencil's panel turns it off; the
  mouse draws as before. Tried with simulated pen input so far — reports from real tablets are
  welcome.

Everything 0.4.0 and 0.4.1 brought — PDF/A preflight and conversion, grey conversion, picture
recompression, vector path editing, action sequences, office exports, a sandboxed JavaScript
engine, signing with cards, tokens and the system store — is described in their notes and in
[CHANGELOG.md][changelog].

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.5.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.5.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.5.0-macos-universal.zip` (app bundle) or `folitecta-0.5.0-macos-universal.tar.gz` (binary) |

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

[changelog]: https://github.com/KvasFan/Folitecta/blob/main/CHANGELOG.md#050---2026-10-28
[features]: https://github.com/KvasFan/Folitecta/blob/v0.5.0/docs/FEATURES.md
[limits]: https://github.com/KvasFan/Folitecta/blob/main/docs/ARCHITECTURE.md#known-limitations
[roadmap]: https://github.com/KvasFan/Folitecta/blob/main/ai/ROADMAP.md
