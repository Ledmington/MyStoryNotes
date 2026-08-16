use eframe::egui;

use super::App;

const NEW_PROJECT_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N);
const OPEN_PROJECT_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
const SAVE_PROJECT_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
const NEW_NOTE_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::M);
const SEARCH_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::F);

impl App {
    /// The top toolbar: project and note actions, plus toggles for the settings and categories
    /// panels. Also consumes this frame's keyboard shortcuts for the actions it mirrors as
    /// buttons — [`super::App::ui`] handles the remaining shortcuts (closing/deleting the open
    /// note) itself, since those need state this panel doesn't otherwise touch.
    pub(super) fn draw_toolbar(&mut self, ui: &mut egui::Ui) {
        let new_project_pressed =
            ui.input_mut(|input| input.consume_shortcut(&NEW_PROJECT_SHORTCUT));
        let open_project_pressed =
            ui.input_mut(|input| input.consume_shortcut(&OPEN_PROJECT_SHORTCUT));
        let save_project_pressed =
            ui.input_mut(|input| input.consume_shortcut(&SAVE_PROJECT_SHORTCUT));
        let new_note_pressed = self.project.is_some()
            && ui.input_mut(|input| input.consume_shortcut(&NEW_NOTE_SHORTCUT));
        let search_pressed = self.project.is_some()
            && ui.input_mut(|input| input.consume_shortcut(&SEARCH_SHORTCUT));

        if new_project_pressed {
            self.new_project();
        }
        if open_project_pressed {
            self.open_project();
        }
        if save_project_pressed {
            self.save_project();
        }
        if new_note_pressed {
            self.new_note_dialog = true;
            self.new_note_name.clear();
            self.new_note_request_focus = true;
        }
        if search_pressed {
            self.search.open();
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button("New Project")
                    .on_hover_text(ui.ctx().format_shortcut(&NEW_PROJECT_SHORTCUT))
                    .clicked()
                {
                    self.new_project();
                }

                if ui
                    .button("Open Project")
                    .on_hover_text(ui.ctx().format_shortcut(&OPEN_PROJECT_SHORTCUT))
                    .clicked()
                {
                    self.open_project();
                }

                if !self.settings.recent_projects.is_empty() {
                    let mut recent_clicked = None;

                    ui.menu_button("Open Recent Project", |ui| {
                        for path in &self.settings.recent_projects {
                            let label = path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.display().to_string());

                            if ui
                                .button(label)
                                .on_hover_text(path.display().to_string())
                                .clicked()
                            {
                                recent_clicked = Some(path.clone());
                                ui.close();
                            }
                        }
                    });

                    if let Some(path) = recent_clicked {
                        self.open_project_from(path);
                    }
                }

                if self.project.is_some() {
                    if ui
                        .button("Save Project")
                        .on_hover_text(ui.ctx().format_shortcut(&SAVE_PROJECT_SHORTCUT))
                        .clicked()
                    {
                        self.save_project();
                    }

                    ui.separator();

                    if ui
                        .button("+ New Note")
                        .on_hover_text(ui.ctx().format_shortcut(&NEW_NOTE_SHORTCUT))
                        .clicked()
                    {
                        self.new_note_dialog = true;
                        self.new_note_name.clear();
                        self.new_note_request_focus = true;
                    }

                    ui.separator();

                    let label =
                        crate::fonts::icon_label(ui, crate::fonts::icon::BOOK, "Manuscript");

                    if ui
                        .button(label)
                        .on_hover_text("Your story's text, linked to your notes")
                        .clicked()
                    {
                        self.open_manuscript();
                    }

                    ui.separator();

                    let label = crate::fonts::icon_label(ui, crate::fonts::icon::SEARCH, "Search");

                    if ui
                        .button(label)
                        .on_hover_text(ui.ctx().format_shortcut(&SEARCH_SHORTCUT))
                        .clicked()
                    {
                        self.search.open();
                    }

                    ui.separator();

                    let label =
                        crate::fonts::icon_label(ui, crate::fonts::icon::TAGS, "Categories");

                    if ui
                        .button(label)
                        .on_hover_text("Manage this project's note categories")
                        .clicked()
                    {
                        self.show_categories = !self.show_categories;
                    }
                }

                ui.separator();

                let label = crate::fonts::icon_label(ui, crate::fonts::icon::COG, "Settings");

                if ui.button(label).clicked() {
                    self.show_settings = !self.show_settings;
                }
            });
        });
    }
}
