use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::{
    categories_panel, graph,
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
const CLOSE_PANEL_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Escape);
/// Either key opens the delete-confirmation dialog for the currently open note. "Delete" is
/// labeled "Canc" on Italian keyboards.
const DELETE_NOTE_SHORTCUTS: [egui::KeyboardShortcut; 2] = [
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Delete),
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Backspace),
];

/// How long the "Saved in …" popup stays up after a save finishes before disappearing on its
/// own.
const SAVE_STATUS_DISPLAY_DURATION: Duration = Duration::from_secs(3);

/// Whether a save was requested by the user or fired on its own from the autosave timer —
/// [`App::draw_save_status`] labels the popup differently for each.
#[derive(Clone, Copy)]
enum SaveKind {
    Manual,
    Auto,
}

/// What [`App::draw_save_status`] shows in the corner popup. `Saved` remembers when it was set
/// (rather than the draw function taking a snapshot) so the popup's remaining lifetime survives
/// across frames instead of resetting on every repaint.
enum SaveStatus {
    Saving(SaveKind),
    Saved {
        kind: SaveKind,
        duration: Duration,
        shown_at: Instant,
    },
}

/// Formats a save's elapsed time for the "Saved in …" popup. Writing this app's small,
/// human-readable project files is usually well under a millisecond, hence the separate case
/// rather than always rounding to whole milliseconds (which would read "Saved in 0ms").
fn format_save_duration(duration: Duration) -> String {
    let millis = duration.as_secs_f64() * 1000.0;

    if millis < 1.0 {
        "less than 1ms".to_owned()
    } else {
        format!("{millis:.0}ms")
    }
}

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
            graph_sim: graph::Simulation::new(),
            graph_view: graph::View::new(),
        }
    }

    fn set_project(&mut self, project: Project) {
        self.open_cell = None;
        if let Some(path) = project.path.clone() {
            self.record_recent_project(path);
        }
        self.project = Some(project);
        // Otherwise a project opened long after startup (or long after the previous project was
        // last saved) could autosave itself within moments of being opened.
        self.last_save_at = Instant::now();
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

        self.open_project_from(path);
    }

    /// Opens the project at `path`, e.g. from the file picker in [`Self::open_project`] or a
    /// click in the "Open Recent Project" menu. On failure, drops `path` from the recent-projects
    /// list (via [`Self::forget_recent_project`]) — the most likely cause is the file having
    /// moved or been deleted since it was last opened.
    fn open_project_from(&mut self, path: PathBuf) {
        match Project::open(path.clone()) {
            Ok(project) => {
                log::info!("Opened project from {:?}", project.path);
                self.set_project(project);
            }
            Err(error) => {
                log::error!("Failed to open project: {error}");
                self.forget_recent_project(&path);
            }
        }
    }

    /// Records `path` as the most recently used project (see
    /// [`Settings::record_recent_project`]) and persists it immediately, so the "Open Recent
    /// Project" list survives a restart even if the user quits without otherwise touching
    /// Settings.
    fn record_recent_project(&mut self, path: PathBuf) {
        self.settings.record_recent_project(path);
        if let Err(error) = self.settings.save() {
            log::error!("Failed to save settings: {error}");
        }
    }

    /// Drops `path` from the recent-projects list (see [`Settings::forget_recent_project`]) and
    /// persists the change immediately.
    fn forget_recent_project(&mut self, path: &Path) {
        self.settings.forget_recent_project(path);
        if let Err(error) = self.settings.save() {
            log::error!("Failed to save settings: {error}");
        }
    }

    fn save_project(&mut self) {
        let Some(project) = &self.project else {
            return;
        };

        if let Some(path) = project.path.clone() {
            self.request_save(path, SaveKind::Manual);
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

        self.request_save(path, SaveKind::Manual);
    }

    /// If autosave is enabled and the open project has already been saved at least once (so
    /// there's a path to autosave to), triggers an autosave once [`Self::last_save_at`] is
    /// further in the past than the configured interval — or, if it isn't due yet, schedules a
    /// repaint for exactly when it will be, so the check re-runs on time even with the user
    /// otherwise idle.
    fn check_autosave(&mut self, ctx: &egui::Context) {
        if !self.settings.autosave.enabled || self.pending_save.is_some() {
            return;
        }

        let Some(path) = self
            .project
            .as_ref()
            .and_then(|project| project.path.clone())
        else {
            return;
        };

        let interval = Duration::from_secs(u64::from(self.settings.autosave.interval_minutes) * 60);
        let elapsed = self.last_save_at.elapsed();

        if elapsed >= interval {
            self.request_save(path, SaveKind::Auto);
        } else {
            ctx.request_repaint_after(interval - elapsed);
        }
    }

    /// Queues `path` to be written next frame (see [`Self::pending_save`]) and shows the
    /// "Saving…"/"Auto-saving…" popup right away.
    fn request_save(&mut self, path: PathBuf, kind: SaveKind) {
        self.pending_save = Some((path, kind));
        self.save_status = Some(SaveStatus::Saving(kind));
    }

    /// Performs a save queued by [`Self::request_save`] on a previous frame, if any, and updates
    /// [`Self::save_status`] and [`Self::last_save_at`] with the result.
    fn process_pending_save(&mut self) {
        let Some((path, kind)) = self.pending_save.take() else {
            return;
        };
        let Some(project) = &mut self.project else {
            self.save_status = None;
            return;
        };

        let start = Instant::now();
        let mut saved_path = None;

        match project.save(&path) {
            Ok(()) => {
                let duration = start.elapsed();
                log::info!(
                    "Saved project to {path:?} in {}",
                    format_save_duration(duration)
                );
                project.path = Some(path.clone());
                self.last_save_at = Instant::now();
                self.save_status = Some(SaveStatus::Saved {
                    kind,
                    duration,
                    shown_at: Instant::now(),
                });
                saved_path = Some(path);
            }
            Err(error) => {
                log::error!("Failed to save project: {error}");
                self.save_status = None;
            }
        }

        if let Some(path) = saved_path {
            self.record_recent_project(path);
        }
    }

    /// Shows [`Self::save_status`], if any, as a small popup in the bottom-left corner (the
    /// opposite corner from the error notifications). The "Saved" variant clears itself once
    /// [`SAVE_STATUS_DISPLAY_DURATION`] has passed.
    fn draw_save_status(&mut self, ctx: &egui::Context) {
        let Some(status) = &self.save_status else {
            return;
        };

        let text = match status {
            SaveStatus::Saving(kind) => {
                // Guarantees a next frame even with no further input, so
                // `process_pending_save` (called at the top of the *next* frame) actually runs
                // promptly instead of waiting for the user to move the mouse.
                ctx.request_repaint();
                match kind {
                    SaveKind::Manual => "Saving…".to_owned(),
                    SaveKind::Auto => "Auto-saving…".to_owned(),
                }
            }
            SaveStatus::Saved { kind, duration, .. } => match kind {
                SaveKind::Manual => format!("Saved in {}", format_save_duration(*duration)),
                SaveKind::Auto => format!("Auto-saved in {}", format_save_duration(*duration)),
            },
        };

        if let SaveStatus::Saved { shown_at, .. } = status {
            let elapsed = shown_at.elapsed();

            if elapsed >= SAVE_STATUS_DISPLAY_DURATION {
                self.save_status = None;
                return;
            }

            ctx.request_repaint_after(SAVE_STATUS_DISPLAY_DURATION - elapsed);
        }

        egui::Area::new(egui::Id::new("save_status"))
            .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -12.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(text);
                });
            });
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

    fn show_rename_dialog(&mut self, ctx: &egui::Context) {
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
        self.process_pending_save();
        self.check_autosave(ui.ctx());

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
        // Only consumed when nothing else would react to Escape first — otherwise it would eat
        // the keypress a dialog or the search window needs for its own close-on-Escape handling.
        let close_panel_pressed = self.open_cell.is_some()
            && self.rename_dialog.is_none()
            && !self.new_note_dialog
            && self.delete_confirm.is_none()
            && !self.search.is_open()
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
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(project) = &mut self.project else {
                ui.centered_and_justified(|ui| {
                    ui.label("Open or create a project to begin.");
                });

                return;
            };

            let open_note = self.open_cell.as_ref().map(|cell| cell.note_index);

            if let Some(clicked) = graph::draw(
                ui,
                project,
                graph::NoteHighlight {
                    open_note,
                    hovered_note,
                },
                graph::GraphAppearance {
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

        if let Some(index) = search::draw(ui.ctx(), self.project.as_ref(), &mut self.search) {
            self.open_cell = Some(Cell {
                note_index: index,
                mode: CellMode::Rendered,
            });
        }
    }
}

/// An action requested from a note's cell, for the caller to act on — each one touches
/// [`crate::app::App`] state that `draw_cell` doesn't have access to (`draw_cell` only has the one
/// note it's drawing, not the whole project's UI state), so it's handed back up rather than
/// handled here.
enum CellAction {
    Rename,
    Delete,
    Close,
}

/// Draws the currently open note as a floating, resizable, movable window on top of the graph
/// view (which otherwise always occupies the whole central area — see [`eframe::App::ui`]),
/// rather than splitting the graph into a side panel to make room for it. The window keeps a
/// fixed [`egui::Id`] rather than one derived from its title, so resizing or moving it persists
/// across switching to a different note (e.g. by clicking a link) instead of resetting.
fn draw_note_window(
    ctx: &egui::Context,
    project: &mut Project,
    cell: &mut Cell,
    settings: &Settings,
) -> Option<CellAction> {
    let title = project
        .notes
        .get(usize::from(cell.note_index))
        .map_or_else(String::new, |note| note.name.clone());

    let mut action = None;

    egui::Window::new(title)
        .id(egui::Id::new("note_window"))
        .resizable(true)
        .collapsible(false)
        .default_size([420.0, 520.0])
        .min_size([280.0, 200.0])
        .show(ctx, |ui| {
            action = draw_cell(ui, project, cell, settings);
        });

    action
}

fn draw_cell(
    ui: &mut egui::Ui,
    project: &mut Project,
    cell: &mut Cell,
    settings: &Settings,
) -> Option<CellAction> {
    let mut link_clicked = false;
    let mut switch_to_editing = false;
    let mut action = None;

    {
        ui.horizontal(|ui| {
            let (icon, label) = match cell.mode {
                CellMode::Rendered => (crate::fonts::icon::PENCIL, "Edit"),
                CellMode::Editing => (crate::fonts::icon::CHECK, "Done"),
            };
            let toggle_label = crate::fonts::icon_label(ui, icon, label);
            let mut toggle_response = ui.small_button(toggle_label);
            // Only Editing -> Rendered has a keyboard shortcut; the other direction is
            // double-click-only, so there's nothing to hint at in Rendered mode.
            if cell.mode == CellMode::Editing {
                toggle_response = toggle_response.on_hover_text(
                    ui.ctx()
                        .format_shortcut(&note_editor::SWITCH_TO_RENDER_SHORTCUT),
                );
            }
            if toggle_response.clicked() {
                cell.mode = match cell.mode {
                    CellMode::Rendered => CellMode::Editing,
                    CellMode::Editing => CellMode::Rendered,
                };
            }

            if icon_button(ui, crate::fonts::icon::PENCIL_SQUARE, "Rename") {
                action = Some(CellAction::Rename);
            }

            let delete_label = crate::fonts::icon_label(ui, crate::fonts::icon::TRASH, "Delete");
            let delete_shortcuts = DELETE_NOTE_SHORTCUTS
                .iter()
                .map(|shortcut| ui.ctx().format_shortcut(shortcut))
                .collect::<Vec<_>>()
                .join(" / ");
            if ui
                .small_button(delete_label)
                .on_hover_text(delete_shortcuts)
                .clicked()
            {
                action = Some(CellAction::Delete);
            }

            let close_label = crate::fonts::icon_label(ui, crate::fonts::icon::TIMES, "Close");
            if ui
                .small_button(close_label)
                .on_hover_text(ui.ctx().format_shortcut(&CLOSE_PANEL_SHORTCUT))
                .clicked()
            {
                action = Some(CellAction::Close);
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

        // The manuscript note isn't drawn as a node in the graph view at all (see
        // `crate::graph::resolve_edges`), so a category assigned to it would have nothing to
        // color.
        let is_manuscript = project
            .notes
            .get(usize::from(cell.note_index))
            .is_some_and(|note| note.is_manuscript);
        if !is_manuscript && !project.categories.is_empty() {
            draw_category_picker(ui, project, cell.note_index);
        }

        match cell.mode {
            CellMode::Rendered => {
                let Some(note) = project.notes.get(usize::from(cell.note_index)) else {
                    return action;
                };

                let scroll_output = egui::ScrollArea::vertical()
                    .id_salt(("note_scroll", cell.note_index))
                    .show(ui, |ui| {
                        markdown::render(
                            ui,
                            &note.source,
                            &settings.render,
                            settings.font_size.render,
                        )
                    });

                // Scoped to just the rendered content's own rect, rather than the whole
                // cell (which would also cover the Edit/Rename/Delete buttons above): egui
                // only lets one interactive widget "win" the pointer at a given position each
                // frame, so a click-sensing region spanning the buttons would shadow them and
                // they'd stop registering hover or clicks at all.
                let content_response = ui.interact(
                    scroll_output.inner_rect,
                    ui.make_persistent_id(("note_content_click", cell.note_index)),
                    egui::Sense::click(),
                );

                let clicked_link = scroll_output.inner;

                if let Some(target) = clicked_link {
                    link_clicked = true;

                    if let Some(index) = project.notes.iter().position(|note| note.name == target) {
                        cell.note_index = NoteId::from(index);
                    } else if is_web_url(&target) {
                        match webbrowser::open(&target) {
                            Ok(()) => log::info!("Opened '{target}' in the browser"),
                            Err(error) => log::error!("Failed to open '{target}': {error}"),
                        }
                    }
                }

                switch_to_editing = !link_clicked && content_response.double_clicked();
            }

            CellMode::Editing => {
                let Some(note) = project.notes.get_mut(usize::from(cell.note_index)) else {
                    return action;
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
    }

    if switch_to_editing {
        cell.mode = CellMode::Editing;
    }

    action
}

/// A labeled dropdown to assign the note at `note_index` to one of `project.categories`, or back
/// to none. Only shown by [`draw_cell`] once the project actually has at least one category —
/// an empty dropdown would just be clutter.
fn draw_category_picker(ui: &mut egui::Ui, project: &mut Project, note_index: NoteId) {
    let Some(note) = project.notes.get(usize::from(note_index)) else {
        return;
    };
    let mut selected = note.category.clone();

    ui.horizontal(|ui| {
        ui.label("Category:");

        egui::ComboBox::from_id_salt(("note_category", note_index))
            .selected_text(selected.as_deref().unwrap_or("None"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut selected, None, "None");
                for category in &project.categories {
                    ui.selectable_value(&mut selected, Some(category.name.clone()), &category.name);
                }
            });
    });

    if let Some(note) = project.notes.get_mut(usize::from(note_index)) {
        note.category = selected;
    }
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
