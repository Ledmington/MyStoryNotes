use eframe::egui;

use my_story_notes_core::project::{NoteId, Project};
use my_story_notes_core::search::{Search, search_notes};

const ESCAPE_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Escape);

/// Shows the project-wide note search window when open: a query field plus a list of every note
/// whose name or content matches (case-insensitively), clicking or pressing Enter on which returns
/// that note's id and closes the search window. Closes itself if `project` is `None`.
pub fn draw(ctx: &egui::Context, project: Option<&Project>, search: &mut Search) -> Option<NoteId> {
    if !search.is_open() {
        return None;
    }

    let Some(project) = project else {
        search.close();
        return None;
    };

    let results: Vec<(NoteId, String)> = search_notes(project, &search.query)
        .into_iter()
        .map(|index| (index, project.notes[usize::from(index)].name.clone()))
        .collect();

    let request_focus = search.take_request_focus();

    let mut selected = None;

    egui::Window::new("Search")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let response = ui.text_edit_singleline(&mut search.query);

            if request_focus {
                response.request_focus();
            }

            if ui.input(|input| input.key_pressed(ESCAPE_SHORTCUT.logical_key)) {
                search.close();
            }

            let enter_pressed =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let jump_to_first = enter_pressed
                .then(|| results.first().map(|(index, _)| *index))
                .flatten();

            ui.separator();

            if search.query.trim().is_empty() {
                ui.label("Type to search note names and content.");
            } else if results.is_empty() {
                ui.label("No matches.");
            } else {
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for (index, name) in &results {
                            if ui.selectable_label(false, name).clicked() {
                                selected = Some(*index);
                            }
                        }
                    });
            }

            if let Some(index) = jump_to_first {
                selected = Some(index);
            }

            ui.separator();

            if ui
                .button("Close")
                .on_hover_text(ctx.format_shortcut(&ESCAPE_SHORTCUT))
                .clicked()
            {
                search.close();
            }
        });

    if selected.is_some() {
        search.close();
    }

    selected
}
