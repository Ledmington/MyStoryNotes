use eframe::egui;

use crate::project::{NoteId, Project};

/// State for the project-wide note search window (opened with Ctrl+F).
#[derive(Default)]
pub struct Search {
    open: bool,
    query: String,
    request_focus: bool,
}

impl Search {
    /// Opens the search window, focusing its query field. Reopening an already-open window just
    /// refocuses it, leaving whatever the user already typed in place.
    pub fn open(&mut self) {
        if !self.open {
            self.query.clear();
        }
        self.open = true;
        self.request_focus = true;
    }
}

/// Shows the project-wide note search window when open: a query field plus a list of every note
/// whose name or content matches (case-insensitively), clicking or pressing Enter on which returns
/// that note's id and closes the search window. Closes itself if `project` is `None`.
pub fn draw(ctx: &egui::Context, project: Option<&Project>, search: &mut Search) -> Option<NoteId> {
    if !search.open {
        return None;
    }

    let Some(project) = project else {
        search.open = false;
        return None;
    };

    let results: Vec<(NoteId, String)> = search_notes(project, &search.query)
        .into_iter()
        .map(|index| (index, project.notes[usize::from(index)].name.clone()))
        .collect();

    let request_focus = search.request_focus;
    search.request_focus = false;

    let mut selected = None;

    egui::Window::new("Search")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let response = ui.text_edit_singleline(&mut search.query);

            if request_focus {
                response.request_focus();
            }

            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                search.open = false;
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

            if ui.button("Close").clicked() {
                search.open = false;
            }
        });

    if selected.is_some() {
        search.open = false;
    }

    selected
}

/// The indices into `project.notes`, in project order, whose name or content contains `query`
/// (case-insensitive). Empty for a blank or whitespace-only `query`, rather than matching every
/// note.
fn search_notes(project: &Project, query: &str) -> Vec<NoteId> {
    let query = query.trim();

    if query.is_empty() {
        return Vec::new();
    }

    let query = query.to_lowercase();

    (0..project.notes.len())
        .map(NoteId::from)
        .filter(|&index| {
            let note = &project.notes[usize::from(index)];
            note.name.to_lowercase().contains(&query) || note.source.to_lowercase().contains(&query)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(name: &str, source: &str) -> crate::project::Note {
        crate::project::Note {
            name: name.to_owned(),
            source: source.to_owned(),
            is_manuscript: false,
        }
    }

    #[test]
    fn search_notes_matches_name_or_content_case_insensitively() {
        let project = Project {
            path: None,
            notes: vec![
                note("Alice", "Lives in the old lighthouse."),
                note("Bob", "Alice's brother."),
                note("Lighthouse", "A landmark on the cliffs."),
            ],
        };

        assert_eq!(
            search_notes(&project, "alice"),
            vec![NoteId::from(0), NoteId::from(1)]
        );
        assert_eq!(
            search_notes(&project, "LIGHTHOUSE"),
            vec![NoteId::from(0), NoteId::from(2)]
        );
        assert_eq!(search_notes(&project, "brother"), vec![NoteId::from(1)]);
        assert!(search_notes(&project, "nonexistent").is_empty());
    }

    #[test]
    fn search_notes_treats_a_blank_query_as_no_matches() {
        let project = Project {
            path: None,
            notes: vec![note("Alice", "")],
        };

        assert!(search_notes(&project, "").is_empty());
        assert!(search_notes(&project, "   ").is_empty());
    }
}
