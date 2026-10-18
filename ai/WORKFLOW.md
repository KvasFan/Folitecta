# Working on Folitecta

How the repository is organized and how a change goes from idea to release. Architecture details
are in [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md); the general rules in
[CONTRIBUTING.md](../CONTRIBUTING.md).

## Layout

| Path | Contents |
|---|---|
| `crates/folitecta-core/` | Document model and every operation on PDFs, independent from the interface. Modules grouped by area: `organize`, `markup`, `graphics`, `page_text`, `interactive`, `navigation`, `protection`, `tags`; shared helpers at the root (`util`, `geom`, `xml`, `zip`). Integration tests in `tests/`. |
| `crates/folitecta-core/assets/commands.json` | The command registry: every user-facing command (id, label, parent, menu, ribbon tab/group, pane, kind, shortcut). |
| `crates/folitecta/` | The Dioxus desktop interface: `commands/` (handler tables by area), `ui/` (chrome, panes, canvas, dialogs, features), `state/`, `files/` (native file dialogs), `mail.rs`, `clipboard.rs`, `recovery.rs`, `selftest/`. |
| `crates/folitecta/assets/i18n/<lang>/<area>.json` | English and Italian strings; a key lives in the file of the area of its first word (`toast.*` in `messages.json`, `short.*` in `short.json`, …). |
| `crates/folitecta/assets/styles/` | Style sheets by part of the window and by feature; colours only through the tokens of `theme.css`. |
| `ocr/`, `dictionaries/` | OCR models and Hunspell dictionaries read at run time, copied next to the executable in release packages. |
| `docs/` | Architecture, feature matrix (generated), release notes. |
| `ai/` | These guides. |

## Adding or completing a feature

1. **Core first.** Put the document operation in `folitecta-core` (one undo step: `begin_edit()`
   … `commit()`), with an integration test in `crates/folitecta-core/tests/` that saves and
   reopens the result.
2. **Command.** Find the command id in `commands.json` (do not invent ids; the registry closes over
   parents, groups and panes). Add `"id" => handler` to the `handlers!` table of the area module
   in `crates/folitecta/src/commands/`; a new module also goes into `TABLES` in
   `commands/mod.rs`. A reduced implementation goes into `PARTIAL` with a comment saying what is
   missing.
3. **Interface.** Dialogs are variants of `state::Dialog` drawn in `ui/dialogs/mod.rs`; feature
   windows live in `ui/features/`. Long work runs in `tokio::task::spawn_blocking` inside
   `dioxus::core::spawn_forever` (a closing dialog must not stop it). Never keep a `read()` or
   `peek()` guard of a signal while writing the same signal.
4. **Strings.** Add English and Italian strings for every new key (`t(lang, "key")`); the tests
   fail when a key is missing in Italian or lives in the wrong file. Ribbon tools also get a short
   name (`short.<command id>`, at most 13 characters) for the label under the icon.
5. **Icons.** `icons/commands.rs` picks the icon of a command (Tabler Icons, MIT).
6. **Self-test.** Add an interaction step in `crates/folitecta/src/selftest/` that drives the
   feature like a user and records pass/fail with details; new dialogs join the dialog audit.
7. **Docs.** Regenerate `docs/FEATURES.md`, add the change to `CHANGELOG.md` (Unreleased), update
   [ROADMAP.md](ROADMAP.md) and [PROGRESS.md](PROGRESS.md).

## Checks

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets --release -- -D warnings
cargo test --workspace --release
cargo build --release -p folitecta
```

The interface self-test runs the real application, captures states at several window sizes,
checks that nothing overflows or is clipped, and performs scripted interactions:

```powershell
$env:FOLITECTA_CONFIG_DIR = "<empty folder>"        # never the user's settings
target\release\folitecta.exe --ui-selftest <output folder>
# one interaction step only:
$env:FOLITECTA_SELFTEST_ONLY = "step:layout"         # arrange, field_items, layout, ocr, policies, qat, spelling
```

The report is `<output folder>/report.json`: `total_problems`, then `states` with layout audits
and `interaction` entries with `pass` and `detail`. Copy the executable to a separate folder
before running it, do not compile while a self-test runs (timing), and never stop a Folitecta the
user is running.

Licences of dependencies: `cargo deny check licenses` (the list is in `deny.toml`).

## Commits and releases

- Commit with the repository's configured identity; messages describe the change in plain words
  and never name other products or tools used for analysis.
- Small steps go to `main` without a tag. A **big release** bumps the workspace version, completes
  `CHANGELOG.md`, `docs/RELEASE_NOTES.md` and the status line of the README, regenerates the
  feature matrix, and pushes a `vX.Y.Z` tag: the tag runs the full CI (Windows, macOS, Linux) and
  the release workflow, which publishes the packages with their SHA-256 files.
- After a release, download the Windows package, check its SHA-256, extract it and run the `ocr`
  and `spelling` self-test steps with that executable (they prove the data folders next to the
  executable are found).
