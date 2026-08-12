//! MyStoryNotes's GUI: split out of the binary purely so integration tests under `tests/` can
//! exercise the app's modules like any other caller would, through their public API. Domain logic
//! (projects/notes, settings data, markdown parsing, the graph's physics and layout, search,
//! logging) lives in the UI-agnostic `my_story_notes_core` crate instead — this crate is just
//! egui/eframe drawing code on top of it.

#![forbid(unsafe_code)]

pub mod app;
pub mod categories_panel;
pub mod fonts;
pub mod graph_view;
pub mod markdown;
pub mod note_editor;
pub mod search_panel;
pub mod settings_panel;
pub mod style;
pub mod todo_panel;
