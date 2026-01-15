# Repository Guidelines

## Project Structure & Module Organization

- `src/` contains the core Rust server and library code (DNS pipeline, web API/UI, datastore).
- `src/tests/` holds integration/unit tests; `benches/` contains benchmarks.
- `templates/` are Askama HTML templates; `static_files/` holds CSS/JS/images.
- `docs/` is the mdBook source; configuration examples live in `goatns.example.json`,
  `zones.json`, and `hello.goat.json`.
- Workspace crates include `goatns-macros/` and `goat-lib/`.

## Build, Test, and Development Commands

- `cargo build --release`: build the release binary.
- `cargo test`: run the Rust test suite.
- `just run`: run the server in dev mode (`cargo run -- server`).
- `just check`: run the full quality gate (clippy, codespell, tests, doc checks).
- `just doc` or `just book`: build rustdoc or serve the mdBook locally.
- `just docker_build`: build the local container image.

## Coding Style & Naming Conventions

- Rust formatting is enforced via `rustfmt` (4-space indentation, standard Rust style).
- Linting uses `clippy` with strict settings; avoid `unwrap`/`expect` in production code.
- Follow Rust naming conventions: `snake_case` for functions/modules, `CamelCase` for types.
- Markdown formatting is checked with `deno fmt` via `just doc_check`.

## Testing Guidelines

- Tests live under `src/tests/` and are run with `cargo test`.
- Match existing module-based test structure (e.g., `src/tests/cli.rs`, `src/tests/db.rs`).
- Benchmarks are in `benches/` and can be run with `cargo bench` when needed.

## Commit & Pull Request Guidelines

- Recent commit subjects are short and lowercase (e.g., “checkpoint”, “more test coverage”);
  keep messages concise and descriptive.
- PRs should include a clear summary, testing notes (commands run), and screenshots for UI
  changes. Link relevant issues when applicable.

## Security & Configuration Tips

- Secrets/config belong in local config files; use `goatns.example.json` as a template.
- Review `SECURITY.md` before reporting or addressing vulnerabilities.
