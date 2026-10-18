# Progress log

Finished work, newest first. One entry per sub-task: requirement, commit, what changed, how it was
checked, what is still open. The history before October 2026 is in
[CHANGELOG.md](../CHANGELOG.md). Never rewrite an old entry: when a task is reopened, add a new one.

## 2026-10-18 — interface: short tool names, ribbon group edges, submenus

- **Requirements:** FOL-L050 (consistent ribbon/menus), FOL-L042 (readable interface).
- **Commit:** "Short tool names under the icons; group edges and submenus follow the pointer" on `main`.
- **Changed:** the ribbon tools are tiles with a short name under the icon (`Pencil`, `Loupe`,
  `Zoom +`, `Zoom −`; 275 names per language in `assets/i18n/<lang>/short.json`, others shortened
  from their label), on by default; View › Tool Names shows icons alone; tooltips keep the whole
  name and the shortcut. Rows of tiles and rows of icons are saved separately. Dragging the edge
  of a ribbon group now picks the number of tools whose edge is nearest to the pointer (a model of
  the real widths) instead of a fixed number of pixels per column; moves without a pressed button
  are ignored during drags. An open submenu waits 300 ms before another row replaces or closes
  it, so the pointer can cross rows on its way; entering the submenu cancels the change; a click
  opens a submenu instead of toggling it, and its position is taken from the row.
- **Checked:** unit tests (short names, nearest-edge drags, tile widths); self-test step `layout`
  7/7 (pane edge +90 px, ribbon rows, group edge follows a 90 px drag, all tools shown at the
  end, short names and tooltips, names hidden and shown, submenu kept while crossing and closed
  after resting); full interface self-test: 255 states, 136 interactions, 0 problems.
- **Open:** short names for the extra tools of widened groups are derived automatically.

## 2026-10-18 — PDF from e-mail messages

- **Requirement:** FOL-L006 (EML profile).
- **Commit:** same series on `main`.
- **Changed:** `folitecta_core::email` reads messages with `mail-parser` (any charset, encoded
  words, quoted-printable, HTML made into text) and `Document::from_email` typesets the subject,
  the header lines and the text on pages, embedding the attachments; File › Open and
  Create › From E-mail open `.eml` files as new documents.
- **Checked:** core tests (a multipart message with an encoded subject, quoted-printable text and a
  PDF attachment; a long message on several pages; save and reopen with the attachment); self-test
  step `email` (an `.eml` opened from disk: new tab, text on the page, attachment bytes).
- **Open:** pictures of the message on pages; mail archives.

## 2026-10-18 — e-mail with attachments, EML drafts and ZIP archives

- **Requirement:** FOL-L053 (partial).
- **Commit:** same series on `main`.
- **Changed:** `sendByEmail`, `sendByEmailZip`, `sendByEmailOpenAll`, `sendByEmailOpenAllZip`,
  `forms.emailData`: the files (the saved file when it holds every change, a copy otherwise) are
  attached through Simple MAPI on Windows, Mail on macOS or `xdg-email` on Linux; when no program
  takes attachments, an unsent EML draft (`X-Unsent: 1`) with the files attached is saved and
  opened with the program that reads e-mail files; only then a `mailto:` message opens with the
  folder of the files. `folitecta_core::zip` writes and reads ZIP archives (deflate through
  flate2, CRC checks).
- **Checked:** ZIP round trip and damaged archives, EML draft round trip (core tests); names,
  outbox, numbering of duplicate names, drafts holding the files (interface tests); the `email`
  self-test step reads back a draft written by the mail code. **Not tried with real mail
  programs** (it would open the user's mail program).
- **Open:** a dialog to choose and inspect the attachments.

## 2026-10-18 — smaller modules (no change of behaviour)

- **Commit:** `705cff8`.
- **Changed:** `files.rs` → `files/`; canvas previews and comment creation in their own files;
  `markup/appearance/` (style, shapes, note icons, measurement labels, drawing); comment exchange
  split into XFDF, FDF and helpers; form field drawing in `interactive/form_appearance.rs`.
- **Checked:** 287 tests; full interface self-test: 255 states, 134 interactions, 0 problems.

## 2026-10-18 — release 0.1.0

- **Commit and tag:** `08f685b`, `v0.1.0` (full CI on Windows, macOS and Linux; release
  packages with SHA-256 files).
- **Requirements advanced:** FOL-L013 (local OCR of pages, areas and pictures), FOL-L019 (spell
  checking with corrections), FOL-L050 (Quick Access Toolbar), FOL-L031 (items of choice fields,
  pictures on push buttons), FOL-L036 (security policies), FOL-L025 (grouping comments).
- **Checked:** the Windows package downloaded from the release, SHA-256 verified, `ocr` and
  `spelling` self-test steps passed with the packaged executable and its data folders.
