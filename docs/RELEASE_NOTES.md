## Folitecta 0.2.0 — safe saving, redaction by content, time stamps

A free PDF editor written entirely in Rust with a Dioxus desktop interface. This release is about
the integrity of the files and about sending them.

**Highlights**

- **Saving can no longer damage a file**: every save is written to a temporary file next to the
  original and replaces it only once it is complete.
- **Protected files stay protected**: a file opened with a password or with restrictions is
  saved again with the same encryption, passwords and permissions. Save Unrestricted Copy removes
  the protection on purpose.
- **Redaction by content**: the text and the pictures under the redaction marks are taken out of
  the page content, while the rest of the page stays text; comments and form fields over the
  marks are removed with their values, and no earlier revision of the file keeps what was
  removed. Pages that cannot be edited that way are drawn as pictures, as before (or every page,
  when chosen in the redaction settings).
- **Time stamps (RFC 3161)**: signatures can get a time stamp from a time stamp server, and a
  document time stamp can be added to any document; only a SHA-256 digest is sent to the server.
  Signature details now show separate checks: integrity, signer, time and revocation (not checked
  yet).
- **E-mail with real attachments**: the document, every open document, as a ZIP archive, or the
  form data, through the mail program; when none takes attachments, an e-mail draft with the
  files is opened. E-mail messages (`.eml`) open as PDF documents with their attachments.
- **Ribbon**: short names under the tool icons (on by default, View › Tool Names hides them),
  group edges that follow the pointer, submenus that wait for the pointer.

**Binaries**

| Platform | File |
|---|---|
| Windows x64 | `folitecta-0.2.0-windows-x64.zip` |
| Linux x64 | `folitecta-0.2.0-linux-x64.tar.gz` |
| macOS (Apple silicon and Intel) | `folitecta-0.2.0-macos-universal.zip` (app bundle) or `.tar.gz` (binary) |

Each package carries the `dictionaries` and `ocr` folders next to the program (in the app
bundle's resources on macOS): keep them with it. Linux needs the WebKitGTK 4.1 runtime
(`libwebkit2gtk-4.1-0`). macOS builds are not notarized: right-click → Open on first launch.

The status of every command is listed in `docs/FEATURES.md`; known limitations are in
`docs/ARCHITECTURE.md`.
