use eframe::egui;

use my_story_notes_core::project::NoteId;

use crate::style;

use super::sort::{NoteSort, sort_button, sorted_note_indices};
use super::{App, Cell, CellMode};

impl App {
    /// The left sidebar: the current project's notes, sortable by name or connection count and
    /// colored by category, or a placeholder message if there's no project (or no notes) open
    /// yet. Returns the note the pointer is hovering, if any, so [`super::App::ui`] can pass it
    /// on to the graph view's own hover highlighting.
    pub(super) fn draw_sidebar(&mut self, ui: &mut egui::Ui) -> Option<NoteId> {
        let mut hovered_note = None;

        egui::Panel::left("sidebar")
            .default_size(240.0)
            .show(ui, |ui| {
                ui.heading("Notes");

                let Some(project) = &self.project else {
                    ui.label("No project open.");
                    return;
                };

                ui.separator();

                if project.notes.is_empty() {
                    ui.label("No notes yet.");
                    return;
                }

                ui.horizontal(|ui| {
                    ui.label("Sort:");

                    if sort_button(
                        ui,
                        "Name",
                        self.note_sort,
                        NoteSort::NameAscending,
                        NoteSort::NameDescending,
                    ) {
                        self.note_sort = self
                            .note_sort
                            .cycle(NoteSort::NameAscending, NoteSort::NameDescending);
                    }
                    if sort_button(
                        ui,
                        "Connections",
                        self.note_sort,
                        NoteSort::ConnectionsAscending,
                        NoteSort::ConnectionsDescending,
                    ) {
                        self.note_sort = self.note_sort.cycle(
                            NoteSort::ConnectionsAscending,
                            NoteSort::ConnectionsDescending,
                        );
                    }
                });
                ui.separator();

                for index in sorted_note_indices(project, self.note_sort) {
                    let note = &project.notes[usize::from(index)];
                    let is_open = self
                        .open_cell
                        .as_ref()
                        .is_some_and(|cell| cell.note_index == index);

                    let category_color = project.category_color(note).map(style::rgb);

                    let label: egui::WidgetText = if note.is_manuscript {
                        crate::fonts::icon_label_colored(
                            ui,
                            crate::fonts::icon::BOOK,
                            &note.name,
                            category_color.unwrap_or_else(|| ui.visuals().text_color()),
                        )
                    } else if let Some(color) = category_color {
                        egui::RichText::new(&note.name).color(color).into()
                    } else {
                        note.name.clone().into()
                    };

                    let response = ui.selectable_label(is_open, label);

                    if response.hovered() {
                        hovered_note = Some(index);
                    }

                    if response.clicked() {
                        self.open_cell = if is_open {
                            None
                        } else {
                            Some(Cell {
                                note_index: index,
                                mode: CellMode::Rendered,
                            })
                        };
                    }
                }
            });

        hovered_note
    }
}
