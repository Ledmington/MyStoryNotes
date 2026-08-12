mod note_lifecycle;
mod note_window;
mod persistence;
mod sort;

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;

use crate::{categories_panel, graph_view, search_panel, settings_panel, style, todo_panel};
use my_story_notes_core::graph::Simulation;
use my_story_notes_core::logging::Notifications;
use my_story_notes_core::project::{NoteId, Project};
use my_story_notes_core::search::Search;
use my_story_notes_core::settings::Settings;

use note_window::{CellAction, draw_note_window};
use persistence::{SaveKind, SaveStatus};
use sort::{NoteSort, sort_button, sorted_note_indices};

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
const CLOSE_PANEL_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Escape);
/// Either key opens the delete-confirmation dialog for the currently open note. "Delete" is
/// labeled "Canc" on Italian keyboards.
const DELETE_NOTE_SHORTCUTS: [egui::KeyboardShortcut; 2] = [
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Delete),
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Backspace),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellMode {
    Rendered,
    Editing,
}

struct Cell {
    note_index: NoteId,
    mode: CellMode,
}

pub struct App {
    project: Option<Project>,
    open_cell: Option<Cell>,
    new_note_dialog: bool,
    new_note_name: String,
    /// Set alongside [`Self::new_note_dialog`]/[`Self::rename_dialog`] whenever a dialog opens (or
    /// reopens), so its text field grabs keyboard focus rather than leaving the window unfocused
    /// underneath the main one; consumed the first frame it's drawn.
    new_note_request_focus: bool,
    /// The note being renamed, and the dialog's current text field content, while the rename
    /// dialog is open.
    rename_dialog: Option<NoteId>,
    rename_name: String,
    rename_request_focus: bool,
    /// The note pending a delete-confirmation prompt.
    delete_confirm: Option<NoteId>,
    /// A save queued by [`Self::request_save`], to be written by [`Self::process_pending_save`]
    /// on the *next* frame — deferred by a frame so the "Saving…" popup set alongside it actually
    /// gets painted before the (synchronous) write happens, rather than being immediately
    /// overwritten by "Saved" within the same frame.
    pending_save: Option<(PathBuf, SaveKind)>,
    /// The corner popup [`Self::draw_save_status`] shows, if any.
    save_status: Option<SaveStatus>,
    /// When the project was last saved (by either [`Self::save_project`] or the autosave timer in
    /// [`Self::check_autosave`]), or `App::new`'s startup time if it hasn't been saved yet this
    /// session — [`Self::check_autosave`] counts the configured interval from here.
    last_save_at: Instant,
    note_sort: NoteSort,
    settings: Settings,
    show_settings: bool,
    show_categories: bool,
    search: Search,
    notifications: Notifications,
    graph_sim: Simulation,
    graph_view: graph_view::View,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let notifications = my_story_notes_core::logging::init();

        crate::fonts::install(&cc.egui_ctx);

        log::info!("MyStoryNotes starting up");

        Self {
            project: None,
            open_cell: None,
            new_note_dialog: false,
            new_note_name: String::new(),
            new_note_request_focus: false,
            rename_dialog: None,
            rename_name: String::new(),
            rename_request_focus: false,
            delete_confirm: None,
            pending_save: None,
            save_status: None,
            last_save_at: Instant::now(),
            note_sort: NoteSort::Unsorted,
            settings: Settings::load(),
            show_settings: false,
            show_categories: false,
            search: Search::default(),
            notifications,
            graph_sim: Simulation::new(),
            graph_view: graph_view::View::new(),
        }
    }

    /// Shows queued error-level log messages as dismissible red popups stacked in the
    /// bottom-right corner.
    fn draw_notifications(&mut self, ctx: &egui::Context) {
        let messages = self.notifications.snapshot();

        if messages.is_empty() {
            return;
        }

        let mut dismiss = None;

        egui::Area::new(egui::Id::new("notifications"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
            .show(ctx, |ui| {
                for (index, message) in messages.iter().enumerate() {
                    egui::Frame::popup(ui.style())
                        .fill(egui::Color32::from_rgb(120, 20, 20))
                        .show(ui, |ui| {
                            ui.set_max_width(320.0);

                            ui.horizontal(|ui| {
                                ui.colored_label(egui::Color32::WHITE, message);

                                if ui.small_button("x").clicked() {
                                    dismiss = Some(index);
                                }
                            });
                        });

                    ui.add_space(4.0);
                }
            });

        if let Some(index) = dismiss {
            self.notifications.dismiss(index);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.process_pending_save();
        self.check_autosave(ui.ctx());

        let visuals = style::to_visuals(&self.settings.ui);
        let font_size = self.settings.font_size.clone();
        let theme = ui.ctx().theme();

        ui.ctx().style_mut_of(theme, |style| {
            style.visuals = visuals;
            style::apply_font_sizes(style, &font_size);
        });

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
        // Only consumed when nothing else would react to Escape first — otherwise it would eat
        // the keypress a dialog, the search window, or the categories window needs for its own
        // close-on-Escape handling.
        let close_panel_pressed = self.open_cell.is_some()
            && self.rename_dialog.is_none()
            && !self.new_note_dialog
            && self.delete_confirm.is_none()
            && !self.search.is_open()
            && !self.show_categories
            && ui.input_mut(|input| input.consume_shortcut(&CLOSE_PANEL_SHORTCUT));
        // Guarded the same way as `close_panel_pressed`, plus requiring the cell isn't mid-edit —
        // otherwise this would steal Backspace from the note's own multiline `TextEdit` before it
        // ever saw the keypress.
        let delete_note_pressed = self
            .open_cell
            .as_ref()
            .is_some_and(|cell| cell.mode == CellMode::Rendered)
            && self.rename_dialog.is_none()
            && !self.new_note_dialog
            && self.delete_confirm.is_none()
            && !self.search.is_open()
            && !self.show_categories
            && ui.input_mut(|input| {
                DELETE_NOTE_SHORTCUTS
                    .iter()
                    .any(|shortcut| input.consume_shortcut(shortcut))
            });

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
        if close_panel_pressed {
            self.open_cell = None;
        }
        if delete_note_pressed {
            self.delete_confirm = self.open_cell.as_ref().map(|cell| cell.note_index);
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

        if self.show_settings {
            egui::Panel::right("settings")
                .default_size(280.0)
                .show(ui, |ui| {
                    settings_panel::draw(ui, &mut self.settings, &mut self.show_settings);
                });
        }

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

                    let label: egui::WidgetText = if note.is_manuscript {
                        crate::fonts::icon_label(ui, crate::fonts::icon::BOOK, &note.name)
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

                if let Some(clicked) = todo_panel::draw(ui, project) {
                    self.open_cell = Some(Cell {
                        note_index: clicked,
                        mode: CellMode::Rendered,
                    });
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(project) = &mut self.project else {
                ui.centered_and_justified(|ui| {
                    ui.label("Open or create a project to begin.");
                });

                return;
            };

            let open_note = self.open_cell.as_ref().map(|cell| cell.note_index);

            if let Some(clicked) = graph_view::draw(
                ui,
                project,
                graph_view::NoteHighlight {
                    open_note,
                    hovered_note,
                },
                graph_view::GraphAppearance {
                    palette: &self.settings.ui,
                    background: &self.settings.graph_background,
                },
                &self.settings.simulation,
                &mut self.graph_sim,
                &mut self.graph_view,
            ) {
                match &mut self.open_cell {
                    Some(cell) => {
                        cell.note_index = clicked;
                        cell.mode = CellMode::Rendered;
                    }
                    None => {
                        self.open_cell = Some(Cell {
                            note_index: clicked,
                            mode: CellMode::Rendered,
                        });
                    }
                }
            }
        });

        if let Some(project) = &mut self.project
            && let Some(cell) = &mut self.open_cell
        {
            let cell_action = draw_note_window(ui.ctx(), project, cell, &self.settings);

            match cell_action {
                Some(CellAction::Rename) => {
                    self.rename_dialog = Some(cell.note_index);
                    self.rename_name = project.notes[usize::from(cell.note_index)].name.clone();
                    self.rename_request_focus = true;
                }
                Some(CellAction::Delete) => {
                    self.delete_confirm = Some(cell.note_index);
                }
                Some(CellAction::Close) => {
                    self.open_cell = None;
                }
                None => {}
            }
        }

        if let Some(project) = &mut self.project {
            categories_panel::draw(ui.ctx(), project, &mut self.show_categories);
        }

        self.draw_notifications(ui.ctx());
        self.draw_save_status(ui.ctx());

        self.show_new_note_dialog(ui.ctx());
        self.show_rename_dialog(ui.ctx());
        self.show_delete_confirm_dialog(ui.ctx());

        if let Some(index) = search_panel::draw(ui.ctx(), self.project.as_ref(), &mut self.search) {
            self.open_cell = Some(Cell {
                note_index: index,
                mode: CellMode::Rendered,
            });
        }
    }
}
