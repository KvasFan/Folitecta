# Folitecta target requirements

The behaviour Folitecta must reach, as 67 stable requirements: **53 local** (FOL-L001–L053) and
**14 remote** (FOL-C001–C014). They describe the target, not the current state — the state is in
[ROADMAP.md](ROADMAP.md). The ids never change; a sub-task is written `FOL-L036/S01`.

## Classes and priorities

| Class | Meaning |
|---|---|
| **Local** (FOL-L) | Runs on the device after the declared components are installed (libraries, fonts, dictionaries, models, or a helper engine reached only through IPC/loopback). No document leaves the device. |
| **Local with a device** | Scanner, printer, smart card, token, tablet and their drivers. System and device APIs are not cloud APIs. |
| **Remote service** (FOL-C) | Needs a service outside the device for that workflow (storage, e-signature invitations, time stamps, fresh revocation data, enterprise rights, a chosen AI endpoint). A LAN server is remote too. |
| **Optional remote variant** | A capability that also exists locally; choosing the service adds destinations, transfers and maybe costs, and never becomes mandatory. |

| Priority | Meaning |
|---|---|
| **P0 — integrity** | Limits that can lose content, protection or meaning. Blocks the release of the operations concerned. |
| **P1 — complete desktop** | Daily work: reading, editing, pages, comments, forms, OCR, search, saving, basic accessibility. |
| **P2 — professional** | Specialist engines and workflows: prepress, advanced conversions, measures, XFA, 3D, automation, on-device AI. |
| **P3 — advanced integrations** | Services and connectors that need back ends, contracts or tenants. |

A requirement is complete only when the behaviour is available, errors are handled, the result is
saved where expected and the acceptance below is verified. A command that shows a roadmap notice
is not complete. Downloading a model or a language pack once does not make later offline work
remote; every local package must be installable offline too.

## Part A — local features

### A.1 Reading, workspace and search

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L001 · P1** | Open local PDFs, including password/certificate encrypted ones with locally available credentials; progressive rendering, thumbnails, selectable text, single/continuous/two-page layouts, right-to-left, zoom and loupe, split view, temporary rotation, presentation and transitions. View rotation is distinct from page rotation. | On a corpus with embedded fonts, transparency, scans and rotated pages, rendering and selection agree; changing the view does not change the file. Unsupported input or protection gives an error and leaves the original untouched. |
| **FOL-L002 · P1** | Real tabs, windows and groups, tiling, sessions with page/zoom/layout, recovery and controlled closing of modified documents. | Two groups keep different positions; reopening a session restores the workspace without treating unsaved changes as confirmed; missing files are reported one by one; recovery of protected documents follows FOL-L051. |
| **FOL-L003 · P1** | Unicode search in the PDF, a selection, folders and collections; phrases and patterns; filters on comments, bookmarks and attachments; embedded indexes and local catalogues that can be updated; export of results. | Results name file, page and object; direct and indexed search agree after an update; an outdated index is marked; invalid patterns and inaccessible encrypted files give explicit results; no implicit OCR of scans. |
| **FOL-L004 · P1** | Panes for content, thumbnails, properties, fonts, security, metadata/XMP, history, links and destinations; inspection of object types and properties. | Selecting a node highlights the right object; metadata and initial view survive reopening; inspection does not change the document; diagnostic exports carry no credentials or keys. |

### A.2 Creation, import, export and templates

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L005 · P1** | Create PDFs that are blank, from ordered images, the clipboard, TXT/RTF, CSV and Markdown with local CSS and LaTeX formulas; page size, margins, encoding and order; keep the formatting each format has. | RTF with styles, CSV with local separators and Markdown with a table and a formula give correct output, never silently reduced to plain text; CSS and resources work offline. |
| **FOL-L006 · P2** | Convert DOCX/XLSX/PPTX and local HTML through declared local engines; import e-mail (EML) and, with a dedicated local integration, mail archives with attachments. | Headings, links, tables, sheets and message/attachment relations are kept for the supported profiles; a missing engine is reported before starting, never replaced by a cloud service; remote URLs belong to FOL-C013. |
| **FOL-L007 · P2** | Virtual PDF printer, local office add-ins, shell associations and previews, conversion from the context menu; optional installation, distinct from the portable edition. | A test print gives a PDF that reopens; an add-in keeps headings, links and tags of its profile; the portable edition keeps every feature that needs no driver and never claims missing components. |
| **FOL-L008 · P2** | Specialist imports (XPS/OXPS, metafiles, CAD DWG/DXF) through local modules, declaring formats/versions, units, scale, layers and 2D/3D. | A known drawing keeps scale, geometry and layers of its profile; unsupported formats are refused explicitly. |
| **FOL-L009 · P1/P2** | Export PDFs, ranges and selections to Unicode TXT, accessible text, PNG/JPEG/TIFF and original images; advanced DOCX/RTF, XLSX, PPTX, HTML, XPS/XML profiles, choosing page fidelity or editable reflow for office formats. | P1: raster/text/images with verified DPI, transparency and order. P2: exported tables keep numeric types and columns; a report lists unsupported conversions; export never overwrites the original. |
| **FOL-L010 · P1** | Gallery of installed templates with thumbnails, search, categories, favourites, import/export and creation of a working copy; document templates, stamps, XMP and form page templates kept apart. | Search and filters work offline; a new document never changes its template; every template has origin, version and dependencies; opening the gallery downloads nothing. |

### A.3 Scanning and OCR

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L011 · P1** | Local scanner acquisition with the available driver: flatbed/feeder, duplex, colour, DPI, size, presets; new PDF, append/insert and interleaving of fronts and backs; a webcam only when chosen explicitly. | A scanned batch keeps order and sides; empty feeder, cancel and disconnection leave a recoverable result and the original intact; the hardware/driver dependency is stated. |
| **FOL-L012 · P1** | Scan clean-up: orientation, deskew, blank page detection, noise, contrast and black-and-white, with a preview of the pages concerned and adjustable thresholds. | A page with a meaningful mark is never deleted automatically; candidates are shown; straightening keeps the whole content and later OCR coordinates stay consistent. |
| **FOL-L013 · P1** | Local multilingual OCR of pages, ranges, regions, images and batches: image with invisible text or editable text; exclusions for existing text, fields, comments and logos; language packs installable offline. | On known scans selected text, search and coordinates match the image; existing text is not duplicated; output and accuracy limits are stated per language; the document never leaves the device. |
| **FOL-L014 · P1** | Guided review of OCR suspects (image crop, recognized text, confidence when available, accept/correct/not text, navigation); form field recognition proposes rectangles and types to confirm. | A correction changes the right OCR text and is saved; a field proposal can be refused or resized before the form field is created; no invented confidence values. |

### A.4 Pages and composition

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L015 · P1** | Insert, copy, move, swap, delete, replace, extract, interleave and reverse pages; combine files; split by count/size/bookmarks/blank pages; find duplicates with a declared criterion. | A preview of the plan and unambiguous ranges; final count and order correct; bookmarks, links, fields, tags and destinations updated where supported, unresolved references listed before committing; same-named fields from different sources renamed traceably; no automatic removal of false duplicates. |
| **FOL-L016 · P1/P2** | Permanent rotation, page/content resize, coordinate normalization; MediaBox/CropBox/BleedBox/TrimBox/ArtBox; auto-crop, page split/merge, overlay and selective flattening. | Comment, field and link coordinates stay aligned; visual cropping is distinct from deleting the data outside the page (which follows FOL-L037); flattening one field does not flatten the others. |
| **FOL-L017 · P1** | Headers/footers, batch Bates numbering, watermarks, backgrounds, logical page labels, date/name/page macros and presets; update or remove Folitecta's own marks without touching similar content of others. | Bates numbers are unique in the chosen sequence; physical page and logical label are distinguished; after inserting pages the preset updates without duplicating marks. |

### A.5 Native editing of text, images and vectors

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L018 · P0** | Unicode, shaping and embedded/substitute fonts in added or edited text, including the appearances of comments, stamps, fields, tables, headers/Bates and every generated object; font inspection and selective replacement, consistent ToUnicode, embedding restrictions respected. | Accents, non-Latin scripts and bidirectional text display, copy and search correctly, also in generated objects; never a silent `?`; a missing glyph leads to an explicit choice or an error before the change. |
| **FOL-L019 · P1** | Line and paragraph editing, linking/joining/splitting blocks, reflow inside an area; vertical text, spacing, scale, indents, single and multiple find/replace; shared Form XObjects edited with an explicit scope. | Longer text reflows without losing characters and the preview warns of overflow; editing one instance of a shared object does not change the others by accident; skipped occurrences are listed with the reason. |
| **FOL-L020 · P1/P2** | Select, insert, extract, replace, crop and transform images, inline and nested ones too; keep ICC/CMYK/bit depth when supported; a raster editor (selections, clone, brush, filters, blending) as a separate mode. | Replacing one image changes that instance only, a global replacement is explicit; colour conversions or bit loss are shown before committing; raster editing never rasterizes the page silently. |
| **FOL-L021 · P2** | Selection by type and multiple, numeric transforms, align/distribute/order; editing of Bézier paths, nodes, clipping, gradients/shadings, stroke, fill and transparency. | A moved node updates the native path and reopens; coordinates and snapping agree with zoom and rotation; geometric comments are not presented as an editor of existing vectors. |

### A.6 Structured navigation, containers and embedded content

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L022 · P1** | Links and named destinations, hierarchical bookmarks and their generation from text/styles/table of contents; import/export, duplicate cleanup, editing of titles, destinations and zoom; clickable table of contents and article threads. | An inserted table of contents updates page references; reordering pages does not break supported links; external actions can be inspected and are never executed while importing or generating. |
| **FOL-L023 · P1/P2** | Attachments with controlled extraction; layers (OCG) with visibility and view/print properties, import/merge/flatten; local audio/video with playback controls and declared codecs. | An extracted attachment has the expected bytes; selecting a layer's objects selects all of them; an unsupported medium is kept and reported; attachments and URLs never open by themselves. |
| **FOL-L024 · P2** | PDF portfolios with components, folders, metadata and cover; a Folitecta binder project (sources, hierarchy, index, order, regeneration) distinct from the PDF container. | An extracted component keeps its bytes; updating or renaming loses no other component; regenerating a binder keeps the configured order and reports missing sources; printing a portfolio prints only the chosen components. |

### A.7 Comments, local review and technical tools

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L025 · P1** | Text markup, notes, rich text, typewriter/callout, shapes, text-free regions, pen/eraser and pressure with supported hardware; properties, defaults, lock and print; multiple selection, group/align/distribute and order. | Comments and appearances agree in a second reader; groups and selections never change unrelated objects; markup stays anchored to its text after saving. |
| **FOL-L026 · P1** | Comments pane with filters, search, replies, states and history; FDF/XFDF/PDF import and export; PDF/CSV summaries and highlighted text; an optional local word-processor integration distinct from export. | A round trip keeps identity, threads and states without duplicates; the summary gives page and link; suggested revisions never change the PDF text automatically; file exchange is not cloud synchronization. |
| **FOL-L027 · P1/P2** | Personal, dynamic and sequential stamps, categories, favourites and a designer; reusable symbol libraries to import/export with fields asked to the user. | Dynamic values are resolved when placed and stay after reopening; sequences never skip or repeat a number because of a cancelled preview; the design selection includes every intended object. |
| **FOL-L028 · P2** | X/Y scale calibration, units, snapping, distance/perimeter/area, diameter/radius/angle and volume from depth; geometry converted into measures, dimension handling and CSV export of the selected measures or the whole scope. | A geometric fixture gives values within the profile tolerance; changing units does not change the physical value; the CSV holds only the chosen subset with correct numbers and locale. |
| **FOL-L029 · P2** | A calculator with an editable tape placed as a comment; alphanumeric and symbol tick marks with sequences, styles and navigation. | Recomputing the tape and rounding are repeatable; tick mark sequences and values survive reopening; the tape is not presented as an accounting check of the document. |
| **FOL-L030 · P1/P2** | Generate and read local barcodes, vector ones too, from text/URL/vCard and form data; editable tables as generated objects, distinct from extracting tables from a PDF. | Independent decoding returns the original payload; minimum resolution and error correction are stated; table export never invents structure that was not recognized. |

### A.8 Forms and local scripts

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L031 · P1** | Form filling and designer: text/date/image, check boxes/radio buttons, lists/combo boxes, buttons and signatures; names, tooltips, visibility, print, required/read-only, appearance, align/duplicate and tab order; graphic fill & sign on non-interactive pages. | Names and export values are distinct; radio groups and lists keep their selection; required fields and overflow are reported; duplication follows the same-name policy of FOL-L015; the date field has a real date behaviour. |
| **FOL-L032 · P1** | Import/export of FDF/XFDF/CSV/TXT/XML; filling from rows, collecting forms, reset, number/date/currency formats, validation, calculations and dependency order; page templates with show/hide and controlled duplication. | CSV with quotes, separators and new lines loses no data; XML profiles are named and never confused with XFA; calculation cycles, missing fields and invalid data give traceable errors, never silently partial values. |
| **FOL-L033 · P2** | A separate XFA engine: detection, display and filling of declared static/dynamic profiles, XML/XDP and explicit conversion when supported. | A public matrix lists supported layouts, events and versions; an unsupported XFA form is never shown as working and the original is kept; conversion to static content warns that dynamic behaviour is lost. |
| **FOL-L034 · P2** | JavaScript console/editor and PDF actions with a documented supported API, events and action chains; time-outs and cancelling; a database bridge only when installed and enabled explicitly. | Fixture scripts give repeatable results; unsupported operations are reported; opening a PDF never gives it network, arbitrary files, databases or system commands; remote submission belongs to FOL-C010. |

### A.9 Comparison, encryption, redaction and local signatures

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L035 · P1** | Synchronized side-by-side comparison of text and geometry/images, ranges and filters; added/deleted pages identified; a PDF report with summary and navigation. | A fixture with replacement, move and a new page tells them apart; "both" really shows two views; a time-out gives an incomplete state, never a report that claims no differences. |
| **FOL-L036 · P0** | Passwords and permissions, encryption with declared profiles including AES-256 where supported; encryption for recipient certificates and reusable local policies; protection kept by Save and Save Copy. | A file opened encrypted stays encrypted after reopening and asks for the expected credentials; removing or changing encryption is an explicit authorized choice; if the profile cannot be kept the save stops without an implicit plain copy. |
| **FOL-L037 · P0** | Permanent selective redaction of text/images/areas, search-and-mark by pattern, profiles, codes/overlays; sanitizing metadata, attachments, hidden content and earlier revisions within the chosen scope; preview, visual covering and removal are distinct; P2: proposals of sensitive data by local rules or models, always reviewed first. | In the new output the removed data cannot be recovered from text extraction, objects, streams or earlier revisions; the rest keeps text/vectors when possible; the safe path writes a new verified copy and states the effect on existing signatures; blur, pixelation, clipping and incremental saves are never called secure redaction. |
| **FOL-L038 · P1** | A library of graphic signatures/initials and local placeholders with protected reuse, multi-page placement and chosen flattening, clearly labelled as different from cryptographic signatures. | A graphic signature stays an appearance and never yields a "valid digital signature" state; a placeholder implies no envelope and no recipient authentication. |
| **FOL-L039 · P1/P2** | Local signing/certification with PKCS#12, the system certificate store and, through middleware, tokens/smart cards/PKCS#11; fields, appearance, reason/location, lock and allowed changes; batches with confirmed scope. | Digest and signature verified by an independent tool; the right identity and key are used and no PIN is stored; signed revisions are kept when compatible, incompatible changes are explicit; no automatic legal qualification. |
| **FOL-L040 · P1** | Offline verification of signature, digest, chain and the time/revocation data already available; local trust management, inspection of signed revisions and comparison with the current one. | Integrity, identity/trust, time, revocation, later changes and policy are reported separately; missing or expired data give "cannot be determined" for that check; the local clock is not a time stamp authority; nothing is fetched from the network without FOL-C006. |

### A.10 Accessibility, standards, optimization and printing

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L041 · P1** | Accessibility checks with a report, assisted auto-tagging, tag tree, language/title/ActualText, reading order, decorative figures/alternate text, links/fields and table repair (TH/TD/spans/headers). | The report separates automatic, manual and not-applicable checks; repairing a table keeps the semantic associations; passing the checker is not claimed as full conformance. |
| **FOL-L042 · P1** | Italian/English interface, full keyboard use, focus and accessible names, contrast, reading reflow and accessible text; local text-to-speech with voice, range, speed, pause and stop. | Open → comment → save can be completed with the keyboard and a screen reader; stopping speech is always immediately available; reflow and voice follow the defined order without changing the PDF. |
| **FOL-L043 · P2** | Preflight with named, versioned profiles, checks/fix-ups and reports; validation of explicitly supported subsets of PDF/A, PDF/E, PDF/X, PDF/UA, PDF/VT; conversion to PDF/A and PDF/X first. | A profile states standard, version, level and covered rules; each fix-up shows its effect, keeps the original and re-runs the checks; metadata alone never attests conformance; each profile needs independent verification before release. |
| **FOL-L044 · P2** | ICC profiles and rendering intents, selective colour conversion, spot colours, grey/monochrome, boxes, overprint and transparency; output preview and explicit flattening. | ICC target, intent and transforms are in the report; excluded objects are not converted; permanent changes are distinct from view simulation. |
| **FOL-L045 · P2** | Space audit and optimization of images/fonts/objects/transparency/data; MRC for scans when available; recompression of one image, a page or a range, presets and estimated effects. | "Selected image" changes no other image of the page; the report compares sizes and changes; quality and searchability are verified on the reopened file; a worse output never silently overwrites. |
| **FOL-L046 · P1/P2** | Printing of physical/logical ranges, selection/view, tabs, comments/fields/pop-ups and portfolios; scale/paper, raster/colour, N-up/poster/booklet, duplex and a matching preview. | The plan shows pages, sheets, components and copies before spooling; portfolio printing includes no unrelated open PDF; on a test device order, scale, margins and counts match; opening the dialog never prints. |

### A.11 Automation, on-device AI and specialist functions

| Id · priority | Behaviour | Acceptance |
|---|---|---|
| **FOL-L047 · P2** | Reusable local action sequences with interactive steps, files/folders/portfolios, macros and presets; a command line with structured results and documented local integrations; scheduling through the operating system. | The preview shows inputs, outputs and collisions; every file has a result and a log; cancelling never reports the batch as fully successful; UI, batch and command line share the same behaviour; a remote step cannot hide in a local action. |
| **FOL-L048 · P2** | On-device AI, built in or through a declared IPC/loopback engine: questions and answers, summaries, extraction, rewriting, translation, suggestions of bookmarks/alternate text/MathML/LaTeX; local models with declared hardware needs and local multi-source projects. | With the external network disabled the installed model does the declared tasks without remote fallback; answers cite pages/regions, separate inferences and admit when there is no answer; edits and tool calls become proposals with preview and confirmation; instructions inside a PDF are never treated as the user's authorization. |
| **FOL-L049 · P2** | Local 3D: inserting/editing 3D annotations, view/part tree, cameras, sections, measures and anchored comments; U3D/PRC/glTF and CAD import declared separately. | Views restore camera and parts; a measure is tied to the right unit and part; unsupported 3D content is kept and identified; authoring is not claimed because a viewer exists. |
| **FOL-L050 · P1/P2** | Local preferences, shortcuts, consistent ribbon/menus/palette, toolboxes, importable department profiles and diagnostics without documents or credentials; local plug-ins with declared capabilities and versions; normal and portable distributions, local administrative settings. | The same command applies the same scope from every entry point; a missing plug-in is reported before the operation; importing a profile imports no secrets; the portable package needs no cloud account to start or to use local tools. |

### A.12 Integrity contracts for every local feature

**FOL-L051 · P0 — Transactions, persistence and recovery.** Every change has an explicit scope and an
undoable transaction until it is committed. Saving writes a temporary file, verifies it and
replaces the target atomically where the file system allows it; errors, a full disk or an
interruption never truncate the original. The project/session format is versioned. Recovery never
writes plain copies of protected documents (it needs equivalent protection and safe key handling,
or stays disabled with a visible explanation). **Acceptance:** simulate a write error or an
interruption and reopen the last valid original; check that temporary and recovery files of
encrypted documents are protected too.

**FOL-L052 · P0 — Identity, references and active content.** The model distinguishes physical page,
label, object/instance, field, comment, destination and revision. Transformations keep or update
relations and report those they cannot keep. Opening a PDF, an attachment, a template or a project
never authorizes scripts, programs, network access or submissions by itself: a safe reading mode and
explicit permissions for active actions. **Acceptance:** after reordering/copying/merging and saving,
links, fields, tags and comments are verified on a known corpus; a document with external actions
causes no traffic and opens no program without the authorized action.

### A.13 Local preparation of sharing

**FOL-L053 · P1 — PDFs, attachments and EML ready for delivery.** Prepare a chosen copy of the
document, ranges, comments or form data as an attachment; create an EML draft or hand it to a
configured local mail program. The user chooses which versions to include and can inspect the
attachments. **Acceptance:** the draft holds the selected attachments and their bytes — not only a
`mailto:` with a subject or an opened folder; preparing sends nothing; a missing mail program, or
one that cannot attach, is reported with the option to save the local draft. Real delivery through
a service belongs to FOL-C009.

## Part B — remote services

Every FOL-C connector states its protocol/API, the owner of the endpoint, the operations it offers
and those it does not. Not every provider is needed in the first release; a provider name visible
without a working integration must be labelled unavailable.

| Id · priority | Function and dependency | Behaviour and data | Acceptance and offline behaviour |
|---|---|---|---|
| **FOL-C001 · P3** | Native remote storage — one connector per provider (personal and business cloud drives); WebDAV through its own protocol. | File/folder browser, open/save/copy, account identity, permissions, remote revision and declared cache; only the file/version and the metadata needed are sent. | An expired login or a concurrent change never overwrites an unknown version; without conditional updates or locks the limit is stated and a new copy is offered; offline opens only authorized cached copies, marked with state and revision. |
| **FOL-C002 · P3** | Document management systems and intranet libraries — one connector per system. | Required metadata, search, check-out/check-in, versions, locks and the privileges each system supports. | A save never bypasses someone else's check-out and never mistakes a local path for a managed revision; on errors a protected local draft is kept with a way to resume. |
| **FOL-C003 · P3** | Shared review — a coordinating service (self-hostable) or a documented protocol on shared storage. | Sessions with participants and roles, identified base document, comments/threads/states, events and conflict resolution; presence and co-authoring declared separately. | Two reviewers online/offline lose no replies and duplicate no events on retry; an incompatible PDF version needs reconciliation; a final copy and XFDF can be exported. |
| **FOL-C004 · P3** | Sharing by link and remote project spaces — a back end (self-hostable). | Explicit upload, reader/reviewer/editor roles, link expiry/revocation, versions and project artefacts; only the selected items are published. | A revoked link gives no new access; limits on copies already downloaded are stated; offline keeps the last known remote state with its check date. |
| **FOL-C005 · P3** | E-signature and remote signing — a provider or own back end for the chosen workflow. | Recipients and roles, fields/placeholders, envelope, invitation, authentication, status, reminders, cancelling and download of the final PDF and report; invitation, graphic and remote cryptographic signatures kept distinct. | Preview before sending; a double click or a retry never creates two envelopes; offline prepares a local draft without marking invitations as sent; remote keys stay with the provider. |
| **FOL-C006 · P1, optional to the desktop** | Time stamps, fresh revocation data and long-term validation material — an external service for online acquisition. | A configured time stamp authority, OCSP/CRL and chosen trust sources; responses, identities, time intervals and links to the signature are verified; the data of the chosen validation profile are embedded; normally only digests and identifiers are sent. | An unreachable authority is never replaced by the local clock as if it had answered; unknown revocation stays unknown; embedded material can be verified offline by FOL-L040 within its time limits; no long-term validation claim without proof of the profile. |
| **FOL-C007 · P3** | Enterprise rights management — API, identity and tenant required. | Tenant connection, reading/applying supported labels and rights, caching allowed by policy, blocking of unauthorized operations; a module separate from PDF passwords. | Without valid rights the content is not shown and no plain copy is saved; offline applies only the offline policy actually granted. |
| **FOL-C008 · P3** | Third-party DRM — service/licence when a policy requires it. | Reading documents of the chosen system through a legitimate integration; content, granted rights and verification result kept separate. | A missing server or a refused grant gives an explicit state; protection is never removed or bypassed. |
| **FOL-C009 · P3** | Mail, notifications and recipient apps — a service for remote delivery. | Preparation is FOL-L053; direct SMTP/API delivery or delivery to apps needs the chosen connector and confirmed recipients and attachments. | Preparation is not delivery; a retry never resends a message whose outcome is unknown; offline saves an explicit draft. |
| **FOL-C010 · P3** | Form submission, response collection and remote databases — an endpoint for the remote destination. | Only the expected data/files to an approved URL, declared schema and types, receipt, status and duplicate handling; a local database bridge stays FOL-L034. | The content of a PDF never authorizes a submission by itself; an attempt without a receipt is not "collected"; data stay exportable locally when the destination is unavailable. |
| **FOL-C011 · P3** | Remote AI/LLM and bring-your-own-model endpoints — optional remote variant of FOL-L048. | Replaceable provider, self-hosted or cloud endpoint, credentials, models/embeddings and declared capabilities; destination, data, cost/limits and retention shown before use; only the chosen scope is sent. | No automatic switch from local to remote or between providers; generation and embedding destinations declared separately; quota, time-out and cancelling leave identified partial work. |
| **FOL-C012 · P3** | Remote OCR, conversion and recognition — only for this variant. | An explicit alternative to the local engines of FOL-L006, 009, 013–014, 037, 041–045: selected upload, job id, engine version, parameters, retention and deletion, download and local verification. | A local feature never needs this account; a missing or invalid result leaves the original intact; quality is measured on the corpus. |
| **FOL-C013 · P3** | URL content, template catalogues and remote AI/tool connectors — a service for the chosen resource. | Fetching URLs/templates, updating catalogues, web search and links to apps with declared destinations and permissions; local cache/import usable without the service. | Opening the local gallery contacts no catalogue; downloading a template sends no PDF; a tool call can never read or publish other documents because of instructions inside a PDF. |
| **FOL-C014 · P3** | Central administration and policies — a back end for remote management. | Organization, roles, allowed configurations, distribution of templates/presets and aggregated diagnostics without content by default; imported local configurations and installer policies stay FOL-L050. | A policy never silently enables uploads or incompatible AI providers; precedence between policy and preferences, offline behaviour and revocation are defined; an unavailable console never blocks allowed local features. |

### Common contract of every connector

1. **Service and protocol:** allowed endpoints, API/protocol version, implemented capabilities, contractual dependencies and a test environment.
2. **Data and destinations:** whole file or only pages/regions/text/images/prompts/embeddings/digests/metadata actually needed; location and retention.
3. **Identity and permissions:** visible account/tenant, least privilege, credentials in the system's protected store; logout, revocation and deletion of local copies; no secret in projects, logs, shared URLs or exported preferences.
4. **Operational consent:** the user configures the connector and confirms the operation (or enables an explicit synchronization policy); the consent never extends to other providers, documents or capabilities.
5. **Jobs and reliability:** local state distinct from remote state; time-outs, cancelling, quotas, idempotency keys where supported and reconciliation of uncertain outcomes.
6. **Versions and conflicts:** provider revision/ETag, hash of the copy being processed, conditional saving.
7. **Offline mode:** available, available from cache, local draft, queued or unavailable — never a hidden queue or an automatic resend of signatures, e-mails or invitations with an uncertain outcome.
8. **Result verification:** protocol authenticity where foreseen, validity of the artefact, explicit local application; an HTTP success never certifies PDF fidelity, conformance or AI correctness.
9. **Costs and dependencies:** subscriptions, quotas and costs visible before acting when knowable; no paid service becomes an implicit requirement for local files.
10. **Diagnostics:** operation id, connector, state, times and error codes with minimal sensitive data; PDFs, prompts or content in logs only in an explicit diagnostic mode.

## Workflows the interface must make simple

| Workflow | Steps | Local / remote |
|---|---|---|
| Searchable scan | scanner/file → proposed clean-up → OCR languages and scope → review of suspects → save a copy and check search | Local (FOL-L011–014); remote only by choosing FOL-C012. |
| Reliable native editing | select object/paragraph → properties/font → preview of reflow or transform → edit → check overflow → save | FOL-L018–021; rasterizing is an explicit alternative that states the loss of editability. |
| Technical file | local sources → page order → bookmarks/index → comments/measures → Bates → binder/portfolio or final PDF | FOL-L015–017, 022–030; upload to a document system is an extra step (FOL-C002). |
| Fillable form | import PDF → proposed fields → designer review → tab order/calculations → test filling → save and export data | FOL-L014, 031–034; submission needs FOL-C010 and an explicit destination. |
| Copy to distribute without confidential data | choose data/patterns → preview marks → sanitizing scope → signature warning → new rewritten copy → independent check | FOL-L037; semantic redaction may use FOL-L048 or FOL-C011/012 by explicit choice. |
| Local signature or remote request | first choose graphic signature, local cryptographic signature or remote envelope → specific checks → distinct result | FOL-L038–040 local; time stamps/revocation and envelopes are FOL-C006 and FOL-C005; no single button mixes them. |
| Document assistant | choose local model or endpoint → choose sources and scope → question → answer with references → preview of proposed edits → confirm | FOL-L048 or FOL-C011. |
| Team review | base copy → participants and roles → share → comment and reconcile → close the review → export the final copy | FOL-C003/004; review by XFDF exchange stays FOL-L026. |

Where the distinction changes the user's choice, the interface shows **Local**, **Remote service** or
**Local device**; unavailable functions name the missing component instead of simulating a result.

## Release increments

| Increment | Scope | Exit condition |
|---|---|---|
| **R0 — baseline integrity** | FOL-L018, 036–037, 051–052: persistence, encryption, Unicode, references and operations on signed files. | Loss and protection cases pass on the corpus; operations not yet safe are blocked or limited explicitly. |
| **R1 — daily local work** | P1 parts of FOL-L001–005, 009–019, 022–023, 025–027, 030–032, 035, 038–042, 046, 050, 053. | Open → edit → reopen, scan → OCR and form → data completed offline; relevant accessibility and interoperability criteria pass. |
| **R2 — professional local tools** | P2 parts: conversions/drivers/CAD, raster and vectors, collections, measures, XFA, scripting/batch, standards/colour/optimization, 3D, on-device AI. | Released per declared module/profile with published corpus and limits. |
| **R3 — trust services** | FOL-C006 and consolidation of FOL-L039–040; optional to the desktop. | Time stamps/revocation/long-term material tested with valid, expired and refused responses and an absent service. |
| **R4 — collaboration and integrations** | FOL-C001–005, 007–014 for selected providers. | Each connector passes the common contract, offline, conflict and retry tests; a disabled service never degrades local features. |

Before building a module, decide its engine and licence, the formats/versions/profiles it covers,
offline distribution, corpus and quality thresholds, memory/time budget and — for services — the
first providers, protocols and data contracts. Content provenance, compatibility with proprietary
plug-in interfaces and any feature without a sufficient contract need a separate specification
first.
