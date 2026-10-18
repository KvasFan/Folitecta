# Progress log

Finished work, newest first. One entry per sub-task: requirement, commit, what changed, how it was
checked, what is still open. The history before October 2026 is in
[CHANGELOG.md](../CHANGELOG.md). Never rewrite an old entry: when a task is reopened, add a new one.

## 2026-10-18 — release 0.2.0

- **Tag:** `v0.2.0` on `main` (full CI on Windows, macOS and Linux; release packages).
- **Contents:** FOL-L051/S01, FOL-L036/S01, FOL-L037/S01, FOL-L006/S01, FOL-C006/S01 with
  FOL-L040/S01, the e-mail work of FOL-L053 and the ribbon changes below. Steps 4 (attachment
  dialog) and 7 (Unicode generated text) of the 0.2.0 plan moved to 0.2.1.
- **Checked:** 324 tests in 50 suites; full interface self-test before tagging (see the entry
  below); the packages of the release downloaded and their SHA-256 compared.

## 2026-10-18 — FOL-C006/S01, FOL-L040/S01: time stamps and separate checks

- **Commit:** "Time stamps (RFC 3161) and separate checks of signatures; Folitecta 0.2.0".
- **Current state before:** no time stamps at all; validation gave one state per signature
  (valid, changed, trusted) and the signing time came from the signer's computer only.
- **Changed:** `digsig::timestamp` builds RFC 3161 requests (SHA-256 imprint, random nonce,
  certificate requested), checks the answer (status, imprint, nonce, signature of the authority,
  time stamping usage of its certificate, validity at the stamp time; genTime with fractions
  accepted) and holds `respond`, a local authority for tests. `cms_sig::sign_content` signs
  detached data or embedded content and puts the token of the signature value in the unsigned
  attribute `id-aa-timeStampToken`; `Document::sign_with` signs with a time stamp,
  `Document::time_stamp` adds a document time stamp (`ETSI.RFC3161`, incremental on signed
  files); `DigitalId::create_time_stamp_authority`. Validation reports `checks` (integrity,
  identity, time, revocation: not checked), `time_stamp`, `document_time_stamp` and
  `later_revisions_time_stamped` (later revisions that all end with valid document time stamps;
  what else they change is not analysed, so integrity stays a warning). Interface: an HTTP/1.1
  client (`http.rs`, TLS of the system through native-tls, redirections, chunked answers), the
  time stamp option and server address in the signing window, Document › Signatures › Timestamp,
  the checks and the time stamp in the signature details, a clock chip in the pane.
- **Checked:** core tests `timestamp` (tokens answer their request, other data and changed
  tokens fail, other nonce and refusals are errors; authorities without the usage are reported;
  document time stamps valid and trusted once the authority is; a signature with a time stamp
  passes every check, without one the time is a warning; a document time stamp after a signature
  keeps it valid; a later edit is not counted as a time stamp; failing transports leave no file);
  HTTP client tests against a local server (redirection, chunked answer, server error); self-test
  step `timestamp` (local authority over HTTP: a signature with a time stamp and a document time
  stamp, pane, four checks in the details).
- **Open:** not tried against a public time stamp server; revocation (OCSP/CRL) and long-term
  validation material; analysis of the changes of later revisions; time-stamping an encrypted
  document writes a plain file (FOL-L036/S02).

## 2026-10-18 — FOL-L037/S01: redaction by content

- **Commit:** "Saves are atomic and keep the encryption of protected files; redaction by content" on `main`.
- **Current state before:** applying redaction drew every marked page as a JPEG picture: the
  data under the marks was gone but so was the text of the whole page; comments and form fields
  over the marks stayed (with their contents and values); a signed document was then saved as an
  incremental update, keeping the earlier revision with the content in the file.
- **Changed:** the text probe of `textedit` records the advance of every glyph (its width with
  the spacing, in thousandths of the font size) and the bounds of every picture drawn;
  `remove_text_in` takes the glyphs under the marks out of their text operations, each replaced
  by a `TJ` move of its advance, so the rest of the line keeps its place; pictures the page draws
  itself get their pixels under the marks covered (`put_page_image`, no undo step of its own);
  comments over the marks and form fields with a widget over them are removed with their values
  (`remove_form_field`); the covers and overlay texts are drawn upright for the reader on rotated
  pages; unreferenced objects are pruned and the document stops being a base for incremental
  updates. Pages with text or pictures inside reused drawings, pictures written in the content or
  fonts the editor cannot handle fall back to the picture method; the redaction settings offer
  the picture method for every page. One undo step for the whole operation.
- **Checked:** core tests `redaction_content` — a word under a mark leaves every stream of the
  saved file (as text and as hex codes) and the next word keeps its position; half of a picture
  becomes black and the other half stays white; a note and a text field over the mark go, their
  texts absent from the saved bytes, a note elsewhere stays; on a page turned by 90° the code is
  upright inside the mark; a signed document is saved with one revision only. Updated
  `document_ops_2` (text outside the marks stays; the picture method still makes the page a
  picture).
- **Open:** vector paths under the marks; reused drawings; metadata and other sanitizing.

## 2026-10-18 — FOL-L036/S01: protected files stay protected

- **Commit:** same as above.
- **Current state before:** opening an encrypted file decrypted it and dropped the encryption;
  Save, Save As and Save Copy then wrote plain files (confirmed in `document.rs`, `load_inner`).
- **Changed:** the lopdf `EncryptionState` of the opened file (file key, O/U values, permissions,
  crypt filters) is kept and applied again to a copy of the document when it is written, the old
  encryption dictionary removed; the `/ID` is kept, so the same user and owner passwords open the
  saved file with the same permissions. `Document::save_unprotected_copy` and Save Unrestricted
  Copy write a plain copy on purpose. Undo snapshots stay plain in memory.
- **Checked:** core tests `keep_protection` — a file with user and owner passwords stays
  encrypted after Save As, Save and Save Copy, both passwords open it, `/P` unchanged, the edit is
  kept; a file with restrictions but no open password keeps its encryption dictionary and `/P`;
  an unrestricted copy opens without password while the document stays protected; plain files
  stay plain.
- **Open:** signing or time-stamping an encrypted document writes a plain signed file; AES-256
  for new protection; certificate encryption.

## 2026-10-18 — FOL-L051/S01: atomic saving

- **Commit:** same as above.
- **Current state before:** `Document::save_as` and `save_copy` wrote the target with
  `std::fs::write`, so an error in the middle (disk full, a locked file) could leave a truncated
  original.
- **Changed:** `util::write_atomic` (temporary file in the same folder, `sync_all`, rename over
  the target, temporary file removed on failure, symbolic links followed) and
  `document::write_pdf_file` (refuses bytes that are not a whole PDF) are used by Save, Save As,
  Save Copy, protected copies, the signed file written after signing and the settings file.
- **Checked:** core tests `atomic_save` — replacement leaves no temporary file; a failing rename
  (target is a folder) and a missing folder leave everything as it was; incomplete PDF bytes are
  refused and the original kept; a failed Save As leaves the document unsaved; on Windows a file
  locked by another program keeps its content and the document stays unsaved.
- **Open:** a versioned session format; exports (pictures, text, attachments) still write
  directly, they never replace the document.

## 2026-10-18 — interface: short tool names, ribbon group edges, submenus

- **Requirements:** FOL-L050 (consistent ribbon/menus), FOL-L042 (readable interface).
- **Commit:** `c7d3599`.
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
- **Commit:** `c7d3599`.
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
- **Commit:** `c7d3599`.
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
