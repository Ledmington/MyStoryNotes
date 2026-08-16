//! MyStoryNotes's UI-agnostic core: project/note/category data and persistence, settings data,
//! markdown parsing, graph domain logic and physics, search, and logging — all independent of
//! any particular GUI toolkit, so it can be exercised by tests (or, someday, a different
//! frontend) without pulling in `egui`/`eframe` at all.

pub mod graph;
mod hex_color;
pub mod inline_format;
pub mod logging;
pub mod markdown;
pub mod math;
pub mod project;
pub mod search;
pub mod settings;
