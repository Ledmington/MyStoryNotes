//! Shared fixture-loading helpers for integration tests. Lives at `tests/common/mod.rs` rather
//! than `tests/common.rs` specifically so cargo doesn't treat it as its own test binary — bring it
//! in with `mod common;` from any file directly under `tests/`.

use std::path::{Path, PathBuf};

use my_story_notes_core::project::Project;

/// Loads a fixture file by name from `tests/fixtures/`, e.g. `fixture("empty_project.mystorynotes")`.
pub fn fixture(name: &str) -> Project {
    load(manifest_dir().join("tests/fixtures").join(name))
}

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_owned()
}

fn load(path: PathBuf) -> Project {
    Project::open(path.clone())
        .unwrap_or_else(|error| panic!("failed to load fixture {}: {error}", path.display()))
}
