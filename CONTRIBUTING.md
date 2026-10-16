# Contributing to Folitecta

Thanks for helping. A few rules keep the project coherent:

1. **Free software only.** New dependencies must be written in Rust and licensed under MIT,
   Apache-2.0 or a GNU licence (dual licences qualify through any matching alternative).
   `cargo deny check licenses` enforces the list in `deny.toml`.
2. **Implement commands, don't invent them.** The command surface lives in
   `crates/folitecta-core/assets/commands.json`. To implement one, add a handler to `HANDLERS`
   in `crates/folitecta/src/commands.rs` (and to `PARTIAL` if it is a reduced version). Menus,
   badges and `docs/FEATURES.md` update automatically; regenerate the matrix with
   `cargo run -p folitecta -- --features-md docs/FEATURES.md`.
3. **Core first.** Document operations belong in `folitecta-core` with an integration test in
   `crates/folitecta-core/tests/`; the UI only calls them.
4. **Before pushing**: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`.
5. Folitecta is an independent implementation. Do not copy code, text or assets from other PDF
   products, and do not reference other products by name in the repository.
