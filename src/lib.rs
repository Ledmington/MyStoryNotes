//! MyStoryNotes as a library: split out of the binary purely so integration tests under `tests/`
//! can exercise the app's modules like any other caller would, through their public API.

#![forbid(unsafe_code)]

pub mod app;
pub mod categories_panel;
pub mod fonts;
pub mod graph;
pub mod logging;
pub mod markdown;
pub mod note_editor;
pub mod project;
pub mod search;
pub mod settings;
pub mod settings_panel;
