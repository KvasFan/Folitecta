# Manual tests

What the automatic checks cannot see: real devices and services, other programs opening what
Folitecta writes, systems other than Linux, and how the work feels. The local check
(`target/preflight.sh`) already runs the tests on Linux and Windows (under Wine) and the
self-test of the interface on Linux; everything here needs a person. Tick each line, and write
what went wrong next to it.

Use the program built from `main` (version 0.73.0 in the window title until the next release)
and, where a file is needed, `docs/sample.pdf` or a document of your own.

## 1. On each system

- [ ] **Windows 10/11**: unpack the zip, start `folitecta.exe` on a machine without Visual C++
      installed; open, edit, save and reopen a PDF. Windows may warn (not code-signed).
- [ ] **macOS** (only after GitHub Actions builds it again): open the app bundle with right-click
      › Open; open, edit, save.
- [ ] **Linux on Ubuntu 22.04 or Debian 12** (not only this system): the package starts and
      draws pages (it needs glibc 2.35 and WebKitGTK 4.1).
- [ ] **HiDPI screen** (scale 150% or 200%): pages sharp, cursors the right size, scrolling and
      Ctrl+wheel smooth; **pinch zoom** on a touchpad.

## 2. Devices and services

- [ ] **Printing on a real printer** (each system): a page, a range, 2 pages per sheet, a
      booklet, two-sided; comments and form fields printed or not as chosen.
- [ ] **E-mail** with the system's mail program (Thunderbird, Outlook, Apple Mail): the
      attachment arrives; a protected copy stays protected.
- [ ] **Time stamp server** (e.g. a public RFC 3161 server) and **revocation** (OCSP/CRL turned
      on): sign, time-stamp, validate.
- [ ] **Smart card or token** (PKCS #11) and the **system certificate store** (Windows, macOS
      keychain): sign with the card; the PIN is asked and never kept.
- [ ] **Scanner**: Document › Insert Scanned Pages *(new in 0.87)* with a real scanner: the
      pages go after the current page of the open document; a new document from the scanner.
- [ ] **Reading aloud** with real voices (speech-dispatcher on Linux, the system voices on
      Windows and macOS): the word read is marked on the page.

## 3. What Folitecta writes, opened elsewhere

Export the same report (pictures, a table, bullet and numbered lists, lists inside lists, a
filled form with check boxes) and open each file in **Microsoft Word / PowerPoint** and
**LibreOffice**:

- [ ] **DOCX**: pictures and tables where the page has them; lists are real Word lists (Word
      renumbers them; lists inside lists indented) *(0.75, 0.76)*; check boxes as ☑/☐ before
      their labels *(0.74)*; form values in their place.
- [ ] **HTML** in a browser: the same, the pictures inside the file.
- [ ] **RTF** in Word/LibreOffice: text, tables, ☑/☐.
- [ ] **PPTX** *(0.73, 0.77)*: a slide per page, the notes under each slide hold its text.
- [ ] **XLSX/CSV** from Export Tables: numbers are numbers, decimal commas right.
- [ ] A document after **Replace Fonts** *(0.88)* opened in Adobe Reader and a browser: the
      text looks the same and does not move; Document › Fonts lists the new font as embedded.
- [ ] A PDF/A copy checked with **veraPDF**.

## 4. New since 0.73.0, by hand

For each: do it, look at the result, undo it (Ctrl+Z), redo it.

- [ ] **Path nodes** *(0.78)*: Edit Content › Shapes, click a path, click one of its nodes (it
      fills), then Smooth, Corner, Straight and Add Node on the bar over the path.
- [ ] **Shapes from three clicks** *(0.79)*: Edit › Path Editing › 3 Point › Arc, Circle, Rect,
      three clicks each; the colour and width of the Line tool are used.
- [ ] **Snapping** *(0.80)*: View › Snap and Snap › To Objects on; a line drawn near the middle
      of another line or where two lines cross lands there.
- [ ] **Fill Rule** *(0.80)*: on a filled shape with a hole (a letter shape, a star), the hole
      shows or fills.
- [ ] **Replace All Image Instances** *(0.81)*: a logo repeated on every page replaced once.
- [ ] **Comment style** *(0.82, 0.83, 0.90)*: in the properties pane: line ends, corners,
      blending (Multiply over text), fill opacity of its own; an underline made wavy; a link's
      On click; text box letters outlined.
- [ ] **Advanced Selection** *(0.84)*: Thumbnails › Select › Advanced Selection; odd pages
      with pictures; then Select › Shape.
- [ ] **Copy and paste pages** *(0.85)*: choose pages in the thumbnails, Ctrl+C, then in
      another document Paste Pages and Paste Pages Special (at the end, 3 copies); bookmarks and
      fields of the pages come along; pasting text from another program still pastes the text.
- [ ] **Paste as Stamp, snapshots** *(0.86, 0.87)*: a screenshot pasted as a stamp; Loupe ›
      Snapshot and Pan and Zoom › Snapshot pasted into another program.
- [ ] **Show Problem Details** *(0.89)*: on a damaged PDF from the wild (truncated, edited by
      hand) and on a sound one ("No problems found").
- [ ] **Document Actions › Clear** *(0.91)*: on a document that shows a message when opened;
      after Clear and save, it opens silently.
- [ ] **Link actions** *(0.92)*: choose a link, add a page turn and a web address, reorder,
      remove; save, reopen, click the link: the actions run in order (web addresses ask first).
- [ ] **Zoom** *(0.93)*: View › Zoom Slider, then the arrow keys change the zoom.

## 5. Use

- [ ] **Keyboard only**: open, navigate pages, add a comment, fill a form, save — without the
      mouse.
- [ ] **Screen reader** (Orca, NVDA, VoiceOver): menus, dialogs and the panes are read.
- [ ] **Italian interface**: the new windows and messages above read naturally.
- [ ] **A large document** (500+ pages, scans): opening, scrolling, search, thumbnails stay fast.
