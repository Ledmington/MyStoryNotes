use eframe::egui;

use my_story_notes_core::project::{NoteId, Project};
use my_story_notes_core::todo::find_todos;

/// How many characters of a TODO's text to show before truncating it with an ellipsis, so a long
/// paragraph doesn't blow out the sidebar's width.
const MAX_PREVIEW_CHARS: usize = 60;

/// Maximum height of the scrollable TODO list, so a project with many TODOs doesn't crowd the
/// note list above it out of the sidebar entirely.
const MAX_HEIGHT: f32 = 160.0;

/// Draws a small panel listing every TODO across `project`'s notes (see
/// [`my_story_notes_core::todo::find_todos`]), meant to sit directly under the sidebar's note
/// list. Returns the note a clicked TODO belongs to, if any, so the caller can open it — the same
/// way `graph_view::draw`/`search_panel::draw` report a click.
pub fn draw(ui: &mut egui::Ui, project: &Project) -> Option<NoteId> {
    let todos = find_todos(project);

    ui.separator();
    ui.heading(format!("TODOs ({})", todos.len()));

    if todos.is_empty() {
        ui.label("No TODOs yet.");
        return None;
    }

    let mut clicked = None;

    egui::ScrollArea::vertical()
        .max_height(MAX_HEIGHT)
        .show(ui, |ui| {
            for todo in &todos {
                let note_name = &project.notes[usize::from(todo.note)].name;
                let label = format!("{note_name}: {}", truncate(&todo.text, MAX_PREVIEW_CHARS));

                if ui.selectable_label(false, label).clicked() {
                    clicked = Some(todo.note);
                }
            }
        });

    clicked
}

/// `text` cut to at most `max_chars` *characters* (never splitting a multi-byte one), with a
/// trailing "…" appended if anything was actually cut.
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }

    let mut truncated: String = text.chars().take(max_chars).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_leaves_short_text_untouched() {
        assert_eq!(truncate("short", 60), "short");
    }

    #[test]
    fn truncate_cuts_long_text_and_appends_an_ellipsis() {
        let text = "a".repeat(100);

        let truncated = truncate(&text, 60);

        assert_eq!(truncated.chars().count(), 61);
        assert!(truncated.ends_with('…'));
        assert!(truncated.starts_with(&"a".repeat(60)));
    }

    #[test]
    fn truncate_never_splits_a_multi_byte_character() {
        let text = "é".repeat(65);

        let truncated = truncate(&text, 60);

        assert_eq!(truncated.chars().count(), 61);
        assert!(truncated.is_char_boundary(truncated.len()));
    }
}
