# CLAUDE.md

Guidance and conventions for working on this codebase.

## What is this project?
This project (MyStoryNotes) is an offline-only note-taking app.
Its main use case is to create graphs of notes regarding stories the user may want to write.

## Requirements
Non-negotiables:
- offline-only
- the save file for each "project" must be a single, human-readable file
- no HTML
- the application must be self-contained, it needs to be a single static executable

Negotiables:
- pretty UI
- animations
- pretty colors

## Commands
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

Format the code:
```bash
cargo fmt
```

## General philosophy and code style
Don't overcomplicate things until there is a clear need.
Prefer keeping everything in a single package until there is a clear need to split things into different libraries.
Try to follow the rule of three: if a certain functionality is not needed in at least three different places, do not refactor it.
A lot of small functions is preferable to few big ones.
Keep comments to a minimum, unless something weird/unusual is going on.
Document all public (`pub` and `pub(crate)`) APIs with a minimal description of what the struct/field/method's purpose is.
When possible, abstract the UI logic into something testable without the graphics.
Formatting, linting, and testing don't need to happen at every step, but are required before a commit.

Open questions should be clarified before implementation, rather than resolved by guessing.
A request that contradicts this document should be flagged, along with a proposed update to this document.
Any change to the project's workflow (build/run/test/lint commands, requirements, setup steps, etc.) must be reflected in both this file and `README.md`.
