use eframe::egui;

use super::{App, Cell, CellMode};

impl App {
    fn create_note(&mut self) {
        let Some(project) = &mut self.project else {
            return;
        };

        match project.create_note(&self.new_note_name) {
            Ok(note_index) => {
                log::info!("Created note '{}'", self.new_note_name);

                self.open_cell = Some(Cell {
                    note_index,
                    mode: CellMode::Editing,
                });

                self.new_note_name.clear();
                self.new_note_dialog = false;
            }
            Err(error) => {
                log::error!("Failed to create note: {error}");
            }
        }
    }

    /// Opens the project's manuscript note, creating it first if this is the first time it's
    /// been opened.
    pub(super) fn open_manuscript(&mut self) {
        let Some(project) = &mut self.project else {
            return;
        };

        match project.get_or_create_manuscript() {
            Ok(note_index) => {
                self.open_cell = Some(Cell {
                    note_index,
                    mode: CellMode::Rendered,
                });
            }
            Err(error) => {
                log::error!("Failed to open the manuscript: {error}");
            }
        }
    }

    pub(super) fn show_new_note_dialog(&mut self, ctx: &egui::Context) {
        if !self.new_note_dialog {
            return;
        }

        let request_focus = self.new_note_request_focus;
        self.new_note_request_focus = false;

        egui::Window::new("New Note")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Name:");

                let response = ui.text_edit_singleline(&mut self.new_note_name);

                if request_focus {
                    response.request_focus();
                }

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.new_note_dialog = false;
                        self.new_note_name.clear();
                    }

                    let create = ui.button("Create").clicked();

                    if create
                        || (response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                    {
                        self.create_note();
                    }
                });
            });
    }

    /// Renames the note in [`Self::rename_dialog`] to [`Self::rename_name`], following it to its
    /// (possibly re-sorted) new position — the rename dialog is only ever opened from that note's
    /// own cell, so it's always the one that should still be showing afterward.
    fn rename_note(&mut self) {
        let Some(project) = &mut self.project else {
            return;
        };
        let Some(id) = self.rename_dialog else {
            return;
        };

        match project.rename_note(id, &self.rename_name) {
            Ok(note_index) => {
                log::info!("Renamed note to '{}'", self.rename_name);

                self.open_cell = Some(Cell {
                    note_index,
                    mode: CellMode::Rendered,
                });

                self.rename_dialog = None;
                self.rename_name.clear();
            }
            Err(error) => {
                log::error!("Failed to rename note: {error}");
            }
        }
    }

    pub(super) fn show_rename_dialog(&mut self, ctx: &egui::Context) {
        if self.rename_dialog.is_none() {
            return;
        }

        let request_focus = self.rename_request_focus;
        self.rename_request_focus = false;

        egui::Window::new("Rename Note")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Name:");

                let response = ui.text_edit_singleline(&mut self.rename_name);

                if request_focus {
                    response.request_focus();
                }

                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    self.rename_dialog = None;
                    self.rename_name.clear();
                }

                ui.horizontal(|ui| {
                    if ui
                        .button("Cancel")
                        .on_hover_text(ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(
                            egui::Modifiers::NONE,
                            egui::Key::Escape,
                        )))
                        .clicked()
                    {
                        self.rename_dialog = None;
                        self.rename_name.clear();
                    }

                    let rename = ui.button("Rename").clicked();

                    if rename
                        || (response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                    {
                        self.rename_note();
                    }
                });
            });
    }

    /// Deletes the note in [`Self::delete_confirm`]. Other notes' links to it are left dangling
    /// (see [`my_story_notes_core::project::Project::delete_note`]). If it was the open cell,
    /// closes the cell; if some other note was open, keeps it open, adjusting for the index shift
    /// deletion causes.
    fn delete_note(&mut self) {
        let Some(project) = &mut self.project else {
            return;
        };
        let Some(id) = self.delete_confirm else {
            return;
        };

        if let Some(note) = project.notes.get(usize::from(id)) {
            log::info!("Deleted note '{}'", note.name);
        }

        project.delete_note(id);

        self.open_cell = self.open_cell.take().and_then(|cell| {
            cell.note_index.after_removing(id).map(|note_index| Cell {
                note_index,
                mode: cell.mode,
            })
        });

        self.delete_confirm = None;
    }

    pub(super) fn show_delete_confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.delete_confirm else {
            return;
        };
        let Some(project) = &self.project else {
            self.delete_confirm = None;
            return;
        };
        let Some(name) = project
            .notes
            .get(usize::from(id))
            .map(|note| note.name.clone())
        else {
            self.delete_confirm = None;
            return;
        };

        egui::Window::new("Delete Note?")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Delete \"{name}\"? This cannot be undone."));

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.delete_confirm = None;
                    }

                    if ui.button("Delete").clicked() {
                        self.delete_note();
                    }
                });
            });
    }
}
