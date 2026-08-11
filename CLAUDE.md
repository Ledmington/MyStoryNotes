# CLAUDE.md

Guidance and conventions for working on this codebase.

## What is this project?
This project (MyStoryNotes) is an offline-only note-taking app.
Its main use case is to create graphs of notes regarding stories the user may want to write.

## Requirements
Non-negotiables:
- offline-only — the app itself makes no network requests; clicking a link to a web address is the one exception, and only opens the user's own browser (a separate process) rather than the app fetching anything
- the save file for each "project" must be a single, human-readable file
- as little HTML as possible — prefer plain CommonMark syntax; raw HTML in note source is only acceptable where CommonMark has no native construct for what's needed (e.g. `<u>...</u>` for underline, which CommonMark doesn't support)
- the application must be self-contained, it needs to be a single static executable
- no `unsafe` code
- no dead code (including code kept alive only via `#[allow(dead_code)]` or similar suppressions) — if something is unused, delete it

Negotiables:
- pretty UI
- animations
- pretty colors

## Minimum supported Rust version
The MSRV is 1.95.0, tracked via `rust-version` in `Cargo.toml`. CI checks both the MSRV and the latest toolchain it's pinned to (currently 1.97.0). Re-check the MSRV with `cargo msrv find` after bumping dependencies or using newer language features, and update `Cargo.toml`, this file, `README.md`, and `.github/workflows/ci.yml` together if it changes.

## Commands
Format the code:
```bash
cargo fmt
```

Debug build:
```bash
cargo build
```

Release build:
```bash
cargo build --release
```

Linting:
```bash
cargo clippy --all-targets --all-features
```

Check documentation (catches broken intra-doc links, e.g. a public item's doc comment linking to something private):
```bash
cargo doc --no-deps
```

Run the app in debug mode:
```bash
cargo run
```

Run the app in release mode:
```bash
cargo run --release
```

Run the test suite:
```bash
cargo test
```

Build and run in a container, as an alternative to installing system libraries locally (see the README's "Running in Docker" section for the full `docker run` invocation, since the app is a windowed GUI program and needs a display server forwarded in):
```bash
docker build -t my_story_notes -f etc/Dockerfile .
```

## Testing
Unit tests live alongside the code they test, in `#[cfg(test)] mod tests` within each module, and may use private internals freely.
Integration/end-to-end tests live under `tests/` and exercise the app only through its public API — this is why `src/lib.rs` exists (with `src/main.rs` as a thin binary wrapper around it): a `tests/` file is compiled as a separate crate and can't see anything the lib doesn't expose as `pub`.
`tests/common/mod.rs` holds shared fixture-loading helpers; fixture project files live in `tests/fixtures/`, including `example_project.mystorynotes`, the user-facing sample referenced from the README.

## General philosophy and code style
Don't overcomplicate things until there is a clear need.
Prefer keeping everything in a single package until there is a clear need to split things into different libraries.
Try to follow the rule of three: if a certain functionality is not needed in at least three different places, do not refactor it.
A lot of small functions is preferable to few big ones.
Many small files are preferable to a few big ones.
Keep comments to a minimum, unless something weird/unusual is going on.
Document all public (`pub` and `pub(crate)`) APIs with a minimal description of what the struct/field/method's purpose is.
When possible, abstract the UI logic into something testable without the graphics.
Formatting, linting, testing, and the documentation check don't need to happen at every step, but are required before a commit.

Open questions should be clarified before implementation, rather than resolved by guessing.
A request that contradicts this document should be flagged, along with a proposed update to this document.
Any change to the project's workflow (build/run/test/lint commands, requirements, setup steps, etc.) must be reflected in both this file and `README.md`.
