use eframe::egui;

use crate::{
    graph,
    logging::Notifications,
    markdown, note_editor,
    project::{NoteId, Project},
    search::{self, Search},
    settings::Settings,
    settings_panel,
};

const FILE_EXTENSION: &str = "mystorynotes";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellMode {
    Rendered,
    Editing,
}

struct Cell {
    note_index: NoteId,
    mode: CellMode,
}

/// How the note list sidebar orders its notes. Clicking a sort button cycles its two variants
/// (ascending, then descending) before landing back on [`Self::Unsorted`] — see
/// [`NoteSort::cycle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoteSort {
    /// The order notes are stored in the project file, unchanged.
    Unsorted,
    NameAscending,
    NameDescending,
    ConnectionsAscending,
    ConnectionsDescending,
}

impl NoteSort {
    /// The state after clicking a sort button whose two directions are `ascending`/`descending`:
    /// `ascending` if neither is currently active, `descending` if `ascending` is active, or
    /// [`Self::Unsorted`] if `descending` is active — so repeated clicks cycle ascending →
    /// descending → unsorted → ascending → ...
    fn cycle(self, ascending: Self, descending: Self) -> Self {
        if self == ascending {
            descending
        } else if self == descending {
            Self::Unsorted
        } else {
            ascending
        }
    }
}

pub struct App {
    project: Option<Project>,
    open_cell: Option<Cell>,
    new_note_dialog: bool,
    new_note_name: String,
    /// The note being renamed, and the dialog's current text field content, while the rename
    /// dialog is open.
    rename_dialog: Option<NoteId>,
    rename_name: String,
    /// The note pending a delete-confirmation prompt.
    delete_confirm: Option<NoteId>,
    note_sort: NoteSort,
    settings: Settings,
    show_settings: bool,
    search: Search,
    notifications: Notifications,
    graph_sim: graph::Simulation,
    graph_view: graph::View,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let notifications = crate::logging::init();

        crate::fonts::install(&cc.egui_ctx);

        log::info!("MyStoryNotes starting up");

        Self {
            project: None,
            open_cell: None,
            new_note_dialog: false,
            new_note_name: String::new(),
            rename_dialog: None,
            rename_name: String::new(),
            delete_confirm: None,
            note_sort: NoteSort::Unsorted,
            settings: Settings::load(),
            show_settings: false,
            search: Search::default(),
            notifications,
            graph_sim: graph::Simulation::new(),
            graph_view: graph::View::new(),
        }
    }

    fn set_project(&mut self, project: Project) {
        self.open_cell = None;
        self.project = Some(project);
    }

    fn new_project(&mut self) {
        log::info!("Created a new, unsaved project");
        self.set_project(Project::new());
    }

    fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("MyStoryNotes Project", &[FILE_EXTENSION])
            .pick_file()
        else {
            return;
        };

        match Project::open(path) {
            Ok(project) => {
                log::info!("Opened project from {:?}", project.path);
                self.set_project(project);
            }
            Err(error) => {
                log::error!("Failed to open project: {error}");
            }
        }
    }

    fn save_project(&mut self) {
        let Some(project) = &self.project else {
            return;
        };

        if let Some(path) = project.path.clone() {
            self.save_project_to(&path);
        } else {
            self.save_project_as();
        }
    }

    fn save_project_as(&mut self) {
        if self.project.is_none() {
            return;
        }

        let Some(path) = rfd::FileDialog::new()
            .add_filter("MyStoryNotes Project", &[FILE_EXTENSION])
            .set_file_name(format!("story.{FILE_EXTENSION}"))
            .save_file()
        else {
            return;
        };

        self.save_project_to(&path);
    }

    fn save_project_to(&mut self, path: &std::path::Path) {
        let Some(project) = &mut self.project else {
            return;
        };

        match project.save(path) {
            Ok(()) => {
                log::info!("Saved project to {path:?}");
                project.path = Some(path.to_owned());
            }
            Err(error) => {
                log::error!("Failed to save project: {error}");
            }
        }
    }

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
    fn open_manuscript(&mut self) {
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

    fn show_new_note_dialog(&mut self, ctx: &egui::Context) {
        if !self.new_note_dialog {
            return;
        }

        egui::Window::new("New Note")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Name:");

                let response = ui.text_edit_singleline(&mut self.new_note_name);

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

    fn show_rename_dialog(&mut self, ctx: &egui::Context) {
        if self.rename_dialog.is_none() {
            return;
        }

        egui::Window::new("Rename Note")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Name:");

                let response = ui.text_edit_singleline(&mut self.rename_name);

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
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
    /// (see [`Project::delete_note`]). If it was the open cell, closes the cell; if some other
    /// note was open, keeps it open, adjusting for the index shift deletion causes.
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

    fn show_delete_confirm_dialog(&mut self, ctx: &egui::Context) {
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
        let visuals = self.settings.ui.to_visuals();
        let font_size = self.settings.font_size.clone();
        let theme = ui.ctx().theme();

        ui.ctx().style_mut_of(theme, |style| {
            style.visuals = visuals;
            font_size.apply_to_style(style);
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
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(project) = &mut self.project else {
                ui.centered_and_justified(|ui| {
                    ui.label("Open or create a project to begin.");
                });

                return;
            };

            let open_note = self.open_cell.as_ref().map(|cell| cell.note_index);

            let Some(cell) = &mut self.open_cell else {
                if let Some(clicked) = graph::draw(
                    ui,
                    project,
                    graph::NoteHighlight {
                        open_note,
                        hovered_note,
                    },
                    &self.settings.ui,
                    &self.settings.simulation,
                    &mut self.graph_sim,
                    &mut self.graph_view,
                ) {
                    self.open_cell = Some(Cell {
                        note_index: clicked,
                        mode: CellMode::Rendered,
                    });
                }

                return;
            };

            egui::Panel::left("graph_panel")
                .resizable(true)
                .default_size(360.0)
                .show(ui, |ui| {
                    if let Some(clicked) = graph::draw(
                        ui,
                        project,
                        graph::NoteHighlight {
                            open_note,
                            hovered_note,
                        },
                        &self.settings.ui,
                        &self.settings.simulation,
                        &mut self.graph_sim,
                        &mut self.graph_view,
                    ) {
                        cell.note_index = clicked;
                        cell.mode = CellMode::Rendered;
                    }
                });

            // The note's width is capped to the smaller of what the graph panel actually leaves
            // it and half the window, so it can still shrink below half (as the graph panel
            // grows) but never grow past it. `Ui::set_max_width` sets an exact width rather than
            // an upper bound, so that smaller value has to be computed ourselves first.
            let half_window_width = ui.ctx().input(|input| input.viewport_rect().width()) * 0.5;
            let max_note_width = ui.available_width().min(half_window_width);
            let mut cell_action = None;
            ui.scope(|ui| {
                ui.set_max_width(max_note_width);
                cell_action = draw_cell(ui, project, cell, &self.settings);
            });

            match cell_action {
                Some(CellAction::Rename) => {
                    self.rename_dialog = Some(cell.note_index);
                    self.rename_name = project.notes[usize::from(cell.note_index)].name.clone();
                }
                Some(CellAction::Delete) => {
                    self.delete_confirm = Some(cell.note_index);
                }
                None => {}
            }
        });

        self.draw_notifications(ui.ctx());

        self.show_new_note_dialog(ui.ctx());
        self.show_rename_dialog(ui.ctx());
        self.show_delete_confirm_dialog(ui.ctx());

        if let Some(index) = search::draw(ui.ctx(), self.project.as_ref(), &mut self.search) {
            self.open_cell = Some(Cell {
                note_index: index,
                mode: CellMode::Rendered,
            });
        }
    }
}

/// An action requested from a note's cell, for the caller to act on — renaming or deleting a
/// note needs a dialog and touches [`crate::app::App`] state (`draw_cell` only has the one note
/// it's drawing, not the whole project's UI state), so it's handed back up rather than handled
/// here.
enum CellAction {
    Rename,
    Delete,
}

fn draw_cell(
    ui: &mut egui::Ui,
    project: &mut Project,
    cell: &mut Cell,
    settings: &Settings,
) -> Option<CellAction> {
    let mut link_clicked = false;
    let mut action = None;

    let response = egui::Frame::group(ui.style())
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (icon, label) = match cell.mode {
                    CellMode::Rendered => (crate::fonts::icon::PENCIL, "Edit"),
                    CellMode::Editing => (crate::fonts::icon::CHECK, "Done"),
                };
                if icon_button(ui, icon, label) {
                    cell.mode = match cell.mode {
                        CellMode::Rendered => CellMode::Editing,
                        CellMode::Editing => CellMode::Rendered,
                    };
                }

                if icon_button(ui, crate::fonts::icon::PENCIL_SQUARE, "Rename") {
                    action = Some(CellAction::Rename);
                }
                if icon_button(ui, crate::fonts::icon::TRASH, "Delete") {
                    action = Some(CellAction::Delete);
                }
            });

            if let Some(note) = project.notes.get(usize::from(cell.note_index))
                && markdown::title(&note.source).as_deref() != Some(note.name.as_str())
            {
                ui.label(
                    egui::RichText::new(format!("Saved as \"{}\"", note.name))
                        .italics()
                        .weak(),
                );
            }

            match cell.mode {
                CellMode::Rendered => {
                    let Some(note) = project.notes.get(usize::from(cell.note_index)) else {
                        return;
                    };

                    let clicked_link = egui::ScrollArea::vertical()
                        .id_salt(("note_scroll", cell.note_index))
                        .show(ui, |ui| {
                            markdown::render(
                                ui,
                                &note.source,
                                &settings.render,
                                settings.font_size.render,
                            )
                        })
                        .inner;

                    if let Some(target) = clicked_link {
                        link_clicked = true;

                        if let Some(index) =
                            project.notes.iter().position(|note| note.name == target)
                        {
                            cell.note_index = NoteId::from(index);
                        } else if is_web_url(&target) {
                            match webbrowser::open(&target) {
                                Ok(()) => log::info!("Opened '{target}' in the browser"),
                                Err(error) => log::error!("Failed to open '{target}': {error}"),
                            }
                        }
                    }
                }

                CellMode::Editing => {
                    let Some(note) = project.notes.get_mut(usize::from(cell.note_index)) else {
                        return;
                    };

                    let id = ui.make_persistent_id(("note_editor", cell.note_index));

                    let done = egui::ScrollArea::vertical()
                        .id_salt(("note_scroll", cell.note_index))
                        .show(ui, |ui| {
                            note_editor::draw_note_editor(
                                ui,
                                &mut note.source,
                                id,
                                &settings.edit,
                                settings.font_size.edit,
                            )
                        })
                        .inner;

                    if done {
                        cell.mode = CellMode::Rendered;
                    }
                }
            }
        })
        .response;

    if !link_clicked && cell.mode == CellMode::Rendered && response.clicked() {
        cell.mode = CellMode::Editing;
    }

    action
}

/// Whether a clicked link's destination looks like a web address rather than another note's
/// name, and so should be opened in the browser instead of navigated to in-app.
fn is_web_url(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://")
}

/// The indices into `project.notes`, ordered per `sort` — the save file's own order for
/// [`NoteSort::Unsorted`]. Ties within the two connections orderings break alphabetically, for a
/// stable, predictable order.
fn sorted_note_indices(project: &Project, sort: NoteSort) -> Vec<NoteId> {
    let mut order: Vec<NoteId> = (0..project.notes.len()).map(NoteId::from).collect();

    match sort {
        NoteSort::Unsorted => {}
        NoteSort::NameAscending => {
            order.sort_by(|&a, &b| {
                project.notes[usize::from(a)]
                    .name
                    .cmp(&project.notes[usize::from(b)].name)
            });
        }
        NoteSort::NameDescending => {
            order.sort_by(|&a, &b| {
                project.notes[usize::from(b)]
                    .name
                    .cmp(&project.notes[usize::from(a)].name)
            });
        }
        NoteSort::ConnectionsAscending | NoteSort::ConnectionsDescending => {
            let counts = graph::connection_counts(project);
            let ascending = sort == NoteSort::ConnectionsAscending;

            order.sort_by(|&a, &b| {
                let by_count = if ascending {
                    counts[usize::from(a)].cmp(&counts[usize::from(b)])
                } else {
                    counts[usize::from(b)].cmp(&counts[usize::from(a)])
                };
                by_count.then_with(|| {
                    project.notes[usize::from(a)]
                        .name
                        .cmp(&project.notes[usize::from(b)].name)
                })
            });
        }
    }

    order
}

/// A sort-toggle button: `label` plus an up/down arrow when `current` is `ascending` or
/// `descending`, highlighted while either is active. Returns whether it was clicked this frame.
fn sort_button(
    ui: &mut egui::Ui,
    label: &str,
    current: NoteSort,
    ascending: NoteSort,
    descending: NoteSort,
) -> bool {
    let text = if current == ascending {
        crate::fonts::icon_label(ui, crate::fonts::icon::ARROW_UP, label)
    } else if current == descending {
        crate::fonts::icon_label(ui, crate::fonts::icon::ARROW_DOWN, label)
    } else {
        label.into()
    };

    let active = current == ascending || current == descending;
    ui.selectable_label(active, text).clicked()
}

/// A small icon-labeled button, for a cell's action row (mode switch, rename, delete). Returns
/// whether it was clicked this frame.
fn icon_button(ui: &mut egui::Ui, icon: char, label: &str) -> bool {
    let label = crate::fonts::icon_label(ui, icon, label);
    ui.small_button(label).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_web_url_recognizes_http_and_https_but_not_note_names() {
        assert!(is_web_url("https://en.wikipedia.org/wiki/Cartography"));
        assert!(is_web_url("http://example.com"));
        assert!(!is_web_url("Mira Solenne"));
        assert!(!is_web_url("ftp://example.com"));
        assert!(!is_web_url(""));
    }
}
