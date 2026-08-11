use eframe::egui;

use crate::{
    graph,
    logging::Notifications,
    markdown,
    project::Project,
    settings::{EditPalette, Settings},
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

/// The five inline markdown constructs a selection can be wrapped in from the note editor.
/// Underline has no native CommonMark syntax, so it's the one represented with raw HTML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineFormat {
    Bold,
    Italic,
    Underline,
    Verbatim,
    Hyperlink,
}

impl InlineFormat {
    // egui's `TextEdit` hard-codes Ctrl+H/K/U/W to delete text (previous char, to-end-of-line,
    // to-start-of-line, previous word — see `check_for_mutating_key_press` in egui's
    // `text_edit/builder.rs`) on every platform where `Modifiers::COMMAND` is Ctrl (i.e. not
    // macOS). Since Underline and Hyperlink use two of those letters, they can only be handled by
    // consuming the keypress *before* `TextEdit::show()` ever sees it — see the comment where
    // `InlineFormat::consume_pressed` is called in `draw_cell` for how that's arranged.
    const BOLD_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::B);
    const ITALIC_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::I);
    const UNDERLINE_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::U);
    const VERBATIM_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::E);
    const HYPERLINK_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::K);

    /// The shortcut key press that was consumed this frame, if any, checked in priority order
    /// (irrelevant here since the five shortcuts share no keys, but kept explicit for clarity).
    fn consume_pressed(ui: &egui::Ui) -> Option<Self> {
        ui.input_mut(|input| {
            if input.consume_shortcut(&Self::BOLD_SHORTCUT) {
                Some(Self::Bold)
            } else if input.consume_shortcut(&Self::ITALIC_SHORTCUT) {
                Some(Self::Italic)
            } else if input.consume_shortcut(&Self::UNDERLINE_SHORTCUT) {
                Some(Self::Underline)
            } else if input.consume_shortcut(&Self::VERBATIM_SHORTCUT) {
                Some(Self::Verbatim)
            } else if input.consume_shortcut(&Self::HYPERLINK_SHORTCUT) {
                Some(Self::Hyperlink)
            } else {
                None
            }
        })
    }

    /// The markup placed before and after the wrapped text.
    fn markers(self) -> (&'static str, &'static str) {
        match self {
            Self::Bold => ("**", "**"),
            Self::Italic => ("*", "*"),
            Self::Underline => ("<u>", "</u>"),
            Self::Verbatim => ("`", "`"),
            Self::Hyperlink => ("[", "]()"),
        }
    }
}

/// Toggles `format`'s markup around `source[selection]` (a *character*, not byte, range): if the
/// selection is already immediately wrapped in `format`'s markers, they're stripped; otherwise
/// they're added, or, if `selection` is empty, inserted as an empty pair at the cursor. Returns
/// the new source and the character range the caller should re-select afterward — the wrapped or
/// unwrapped text, or, for an empty selection, the point between the two markers (so hyperlink
/// lands the cursor inside `[]`, ready to type the link text).
fn apply_inline_format(
    source: &str,
    selection: std::ops::Range<usize>,
    format: InlineFormat,
) -> (String, std::ops::Range<usize>) {
    let (prefix, suffix) = format.markers();

    let start = char_to_byte_index(source, selection.start);
    let end = char_to_byte_index(source, selection.end);

    if marker_ends_at(source, start, prefix) && marker_starts_at(source, end, suffix) {
        let unwrap_start = start - prefix.len();
        let unwrap_end = end + suffix.len();

        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..unwrap_start]);
        new_source.push_str(&source[start..end]);
        new_source.push_str(&source[unwrap_end..]);

        let prefix_len = prefix.chars().count();
        let new_selection = (selection.start - prefix_len)..(selection.end - prefix_len);

        (new_source, new_selection)
    } else {
        let mut new_source = String::with_capacity(source.len() + prefix.len() + suffix.len());
        new_source.push_str(&source[..start]);
        new_source.push_str(prefix);
        new_source.push_str(&source[start..end]);
        new_source.push_str(suffix);
        new_source.push_str(&source[end..]);

        let prefix_len = prefix.chars().count();
        let new_selection = (selection.start + prefix_len)..(selection.end + prefix_len);

        (new_source, new_selection)
    }
}

/// Whether `marker` sits immediately before byte offset `boundary` in `source` — and, when
/// `marker` is a run of one repeated character (as Bold's `**` and Italic's `*` both are), isn't
/// merely the tail of a *longer* run of that character. Without that check, reselecting bold text
/// and pressing Italic would see Bold's `**` and mistake it for two copies of Italic's `*`.
fn marker_ends_at(source: &str, boundary: usize, marker: &str) -> bool {
    if !source[..boundary].ends_with(marker) {
        return false;
    }

    match repeated_char(marker) {
        Some(c) => !source[..boundary - marker.len()].ends_with(c),
        None => true,
    }
}

/// The mirror of [`marker_ends_at`]: whether `marker` sits immediately after byte offset
/// `boundary`, and isn't the head of a longer run of the same repeated character.
fn marker_starts_at(source: &str, boundary: usize, marker: &str) -> bool {
    if !source[boundary..].starts_with(marker) {
        return false;
    }

    match repeated_char(marker) {
        Some(c) => !source[boundary + marker.len()..].starts_with(c),
        None => true,
    }
}

/// If `s` is one or more repetitions of a single character, that character.
fn repeated_char(s: &str) -> Option<char> {
    let mut chars = s.chars();
    let first = chars.next()?;
    chars.all(|c| c == first).then_some(first)
}

/// The byte offset of the `char_index`-th character in `s`, or `s.len()` if `char_index` is at or
/// past the end — `str` indexing needs byte offsets, but egui's text cursors count characters.
fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map_or(s.len(), |(byte_index, _)| byte_index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellMode {
    Rendered,
    Editing,
}

struct Cell {
    note_index: usize,
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
    note_sort: NoteSort,
    settings: Settings,
    show_settings: bool,
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
            note_sort: NoteSort::Unsorted,
            settings: Settings::load(),
            show_settings: false,
            notifications,
            graph_sim: graph::Simulation::new(),
            graph_view: graph::View::new(),
        }
    }

    /// Draws the color pickers for the Settings panel, saving to `~/.my_story_notes` whenever
    /// one changes.
    fn draw_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Settings");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = crate::fonts::icon_label(ui, crate::fonts::icon::TIMES, "Close");

                if ui.small_button(label).clicked() {
                    self.show_settings = false;
                }
            });
        });
        ui.separator();

        let mut changed = false;

        ui.label("Themes");
        ui.horizontal_wrapped(|ui| {
            for theme in crate::settings::themes() {
                if ui.button(theme.name).clicked() {
                    log::info!("Applying '{}' theme", theme.name);
                    self.settings.apply_theme(&theme);
                    changed = true;
                }
            }
        });

        ui.add_space(12.0);
        ui.label("Interface");
        changed |= font_size_row(ui, "Font size", &mut self.settings.font_size.ui);
        changed |= color_row(
            ui,
            "Window background",
            &mut self.settings.ui.window_background,
        );
        changed |= color_row(
            ui,
            "Panel background",
            &mut self.settings.ui.panel_background,
        );
        changed |= color_row(ui, "Text", &mut self.settings.ui.text);
        changed |= color_row(ui, "Accent", &mut self.settings.ui.accent);
        changed |= color_row(ui, "Hyperlinks", &mut self.settings.ui.hyperlink);

        ui.add_space(12.0);
        ui.label("Render mode");
        changed |= font_size_row(ui, "Font size", &mut self.settings.font_size.render);
        changed |= color_row(ui, "Heading", &mut self.settings.render.heading);
        changed |= color_row(ui, "Bold", &mut self.settings.render.bold);
        changed |= color_row(ui, "Code", &mut self.settings.render.code);
        changed |= color_row(ui, "Links", &mut self.settings.render.link);

        ui.add_space(12.0);
        ui.label("Edit mode");
        changed |= font_size_row(ui, "Font size", &mut self.settings.font_size.edit);
        changed |= color_row(ui, "Heading", &mut self.settings.edit.heading);
        changed |= color_row(ui, "Bold", &mut self.settings.edit.bold);
        changed |= color_row(ui, "Punctuation", &mut self.settings.edit.punctuation);
        changed |= color_row(ui, "Code", &mut self.settings.edit.code);
        changed |= color_row(ui, "Links", &mut self.settings.edit.link);

        ui.add_space(12.0);
        ui.label("Graph physics");
        changed |= simulation_param_row(
            ui,
            "Unconnected distance",
            "How far apart two notes with no link between them settle.",
            &mut self.settings.simulation.weak_distance,
            50.0..=500.0,
        );
        changed |= simulation_param_row(
            ui,
            "Unconnected strength",
            "How strongly unconnected notes resist moving away from that distance.",
            &mut self.settings.simulation.weak_strength,
            50.0..=2_000.0,
        );
        changed |= simulation_param_row(
            ui,
            "Linked distance",
            "How far apart two linked notes settle.",
            &mut self.settings.simulation.strong_distance,
            20.0..=300.0,
        );
        changed |= simulation_param_row(
            ui,
            "Linked strength",
            "How strongly linked notes resist moving away from that distance.",
            &mut self.settings.simulation.strong_strength,
            500.0..=20_000.0,
        );
        changed |= simulation_param_row(
            ui,
            "Angular spread",
            "How strongly a note's links fan out around it instead of bunching together.",
            &mut self.settings.simulation.angular_repulsion,
            0.0..=2_000.0,
        );
        changed |= simulation_param_row(
            ui,
            "Damping",
            "How quickly motion settles down; higher values are stiffer but settle faster.",
            &mut self.settings.simulation.damping,
            0.5..=15.0,
        );
        changed |= simulation_param_row(
            ui,
            "Centering",
            "How strongly the whole graph is pulled toward the center of the canvas.",
            &mut self.settings.simulation.centering,
            0.0..=2.0,
        );

        ui.add_space(12.0);

        if ui.button("Reset to Defaults").clicked() {
            self.settings = Settings::default();
            changed = true;
        }

        if changed && let Err(error) = self.settings.save() {
            log::error!("Failed to save settings: {error}");
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
                    self.draw_settings(ui);
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
                    let note = &project.notes[index];
                    let is_open = self
                        .open_cell
                        .as_ref()
                        .is_some_and(|cell| cell.note_index == index);

                    let response = ui.selectable_label(is_open, &note.name);

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
            ui.scope(|ui| {
                ui.set_max_width(max_note_width);
                draw_cell(ui, project, cell, &self.settings);
            });
        });

        self.draw_notifications(ui.ctx());

        self.show_new_note_dialog(ui.ctx());
    }
}

fn draw_cell(ui: &mut egui::Ui, project: &mut Project, cell: &mut Cell, settings: &Settings) {
    let mut link_clicked = false;

    let response = egui::Frame::group(ui.style())
        .show(ui, |ui| match cell.mode {
            CellMode::Rendered => {
                if mode_switch_button(ui, crate::fonts::icon::PENCIL, "Edit") {
                    cell.mode = CellMode::Editing;
                }

                let Some(note) = project.notes.get(cell.note_index) else {
                    return;
                };

                if let Some(target) = markdown::render(
                    ui,
                    &note.source,
                    &settings.render,
                    settings.font_size.render,
                ) {
                    link_clicked = true;

                    if let Some(index) = project.notes.iter().position(|note| note.name == target) {
                        cell.note_index = index;
                    }
                }
            }

            CellMode::Editing => {
                if mode_switch_button(ui, crate::fonts::icon::CHECK, "Done") {
                    cell.mode = CellMode::Rendered;
                }

                let Some(note) = project.notes.get_mut(cell.note_index) else {
                    return;
                };

                let id = ui.make_persistent_id(("note_editor", cell.note_index));

                if draw_note_editor(
                    ui,
                    &mut note.source,
                    id,
                    &settings.edit,
                    settings.font_size.edit,
                ) {
                    cell.mode = CellMode::Rendered;
                }
            }
        })
        .response;

    if !link_clicked && cell.mode == CellMode::Rendered && response.clicked() {
        cell.mode = CellMode::Editing;
    }
}

/// Draws the raw-source editor for a note's `source` at the given persistent `id`.
///
/// Applies any pending [`InlineFormat`] shortcut *before* handing input to [`egui::TextEdit`],
/// rather than after (as would be the natural order, since we need the widget's own cursor state
/// to know the selection). `TextEdit` hard-codes Ctrl+K/Ctrl+U to delete text — the same keys used
/// here for Hyperlink and Underline — so if the widget saw the keypress first, it would delete the
/// selection *in addition to* whatever we did with it. Consuming the shortcut first removes the
/// event from the input queue, so by the time `TextEdit::show()` runs, the key press is already
/// gone and its built-in binding never fires. Returns whether the widget lost focus this frame.
fn draw_note_editor(
    ui: &mut egui::Ui,
    source: &mut String,
    id: egui::Id,
    edit: &EditPalette,
    edit_size: f32,
) -> bool {
    if ui.memory(|memory| memory.has_focus(id))
        && let Some(format) = InlineFormat::consume_pressed(ui)
        && let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ui.ctx(), id)
        && let Some(cursor_range) = state.cursor.char_range()
    {
        let selection = cursor_range.as_sorted_char_range();
        let (new_source, new_selection) = apply_inline_format(
            source,
            usize::from(selection.start)..usize::from(selection.end),
            format,
        );
        *source = new_source;

        let new_range = egui::text::CCursorRange::two(
            egui::text::CCursor::new(new_selection.start),
            egui::text::CCursor::new(new_selection.end),
        );
        state.cursor.set_char_range(Some(new_range));
        state.store(ui.ctx(), id);
    }

    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
        let mut layout_job = markdown::highlight(ui, buf.as_str(), edit, edit_size);
        layout_job.wrap.max_width = wrap_width;
        ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
    };

    let output = egui::TextEdit::multiline(source)
        .id(id)
        .desired_width(f32::INFINITY)
        .desired_rows(15)
        .layouter(&mut layouter)
        .show(ui);

    output.response.lost_focus()
}

/// The indices into `project.notes`, ordered per `sort` — the save file's own order for
/// [`NoteSort::Unsorted`]. Ties within the two connections orderings break alphabetically, for a
/// stable, predictable order.
fn sorted_note_indices(project: &Project, sort: NoteSort) -> Vec<usize> {
    let mut order: Vec<usize> = (0..project.notes.len()).collect();

    match sort {
        NoteSort::Unsorted => {}
        NoteSort::NameAscending => {
            order.sort_by(|&a, &b| project.notes[a].name.cmp(&project.notes[b].name));
        }
        NoteSort::NameDescending => {
            order.sort_by(|&a, &b| project.notes[b].name.cmp(&project.notes[a].name));
        }
        NoteSort::ConnectionsAscending | NoteSort::ConnectionsDescending => {
            let counts = graph::connection_counts(project);
            let ascending = sort == NoteSort::ConnectionsAscending;

            order.sort_by(|&a, &b| {
                let by_count = if ascending {
                    counts[a].cmp(&counts[b])
                } else {
                    counts[b].cmp(&counts[a])
                };
                by_count.then_with(|| project.notes[a].name.cmp(&project.notes[b].name))
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

/// A small icon-labeled button in its own row, for switching a cell's mode (Edit/Done). Returns
/// whether it was clicked this frame.
fn mode_switch_button(ui: &mut egui::Ui, icon: char, label: &str) -> bool {
    let mut clicked = false;

    ui.horizontal(|ui| {
        let label = crate::fonts::icon_label(ui, icon, label);
        clicked = ui.small_button(label).clicked();
    });

    clicked
}

/// A labeled color picker row. Returns whether the color changed this frame.
fn color_row(ui: &mut egui::Ui, label: &str, color: &mut [u8; 3]) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        changed = ui.color_edit_button_srgb(color).changed();
        ui.label(label);
    });

    changed
}

/// A labeled font-size slider row. Returns whether the size changed this frame.
fn font_size_row(ui: &mut egui::Ui, label: &str, size: &mut f32) -> bool {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(size, 8.0..=32.0).text(label))
            .changed()
    })
    .inner
}

/// A labeled slider row for a [`crate::settings::SimulationSettings`] field, with a tooltip
/// explaining what it does. Returns whether the value changed this frame.
fn simulation_param_row(
    ui: &mut egui::Ui,
    label: &str,
    tooltip: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(value, range).text(label))
            .on_hover_text(tooltip)
            .changed()
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_a_selection_in_markers_and_reselects_the_wrapped_text() {
        let (source, selection) = apply_inline_format("hello world", 6..11, InlineFormat::Bold);
        assert_eq!(source, "hello **world**");
        assert_eq!(selection, 8..13);
        assert_eq!(
            &source[char_to_byte_index(&source, selection.start)..],
            "world**"
        );
    }

    #[test]
    fn empty_selection_inserts_an_empty_pair_and_lands_the_cursor_between_them() {
        let (source, selection) = apply_inline_format("hello ", 6..6, InlineFormat::Italic);
        assert_eq!(source, "hello **");
        assert_eq!(selection, 7..7);
    }

    #[test]
    fn hyperlink_lands_the_cursor_inside_the_brackets_when_nothing_is_selected() {
        let (source, selection) = apply_inline_format("see also ", 9..9, InlineFormat::Hyperlink);
        assert_eq!(source, "see also []()");
        assert_eq!(selection, 10..10);
    }

    #[test]
    fn underline_wraps_with_html_since_commonmark_has_no_native_syntax() {
        let (source, selection) = apply_inline_format("plain text", 0..5, InlineFormat::Underline);
        assert_eq!(source, "<u>plain</u> text");
        assert_eq!(selection, 3..8);
    }

    #[test]
    fn char_to_byte_index_accounts_for_multi_byte_characters() {
        let s = "héllo";
        assert_eq!(char_to_byte_index(s, 0), 0);
        assert_eq!(char_to_byte_index(s, 1), 1);
        // 'é' is 2 bytes, so the 3rd character ('l') starts at byte 3, not 2.
        assert_eq!(char_to_byte_index(s, 2), 3);
        assert_eq!(char_to_byte_index(s, 100), s.len());
    }

    /// Regression test: applying the same format to an already-wrapped selection used to wrap it
    /// a second time ("**hello**" -> "****hello****") instead of removing the markup.
    #[test]
    fn pressing_the_same_format_twice_toggles_it_back_off() {
        let (wrapped, selection) = apply_inline_format("hello world", 6..11, InlineFormat::Bold);
        assert_eq!(wrapped, "hello **world**");

        let (unwrapped, selection) = apply_inline_format(&wrapped, selection, InlineFormat::Bold);
        assert_eq!(unwrapped, "hello world");
        assert_eq!(selection, 6..11);
    }

    #[test]
    fn toggling_off_verbatim_and_underline_also_round_trips() {
        let (wrapped, selection) =
            apply_inline_format("hello world", 6..11, InlineFormat::Verbatim);
        let (unwrapped, selection) =
            apply_inline_format(&wrapped, selection, InlineFormat::Verbatim);
        assert_eq!(unwrapped, "hello world");
        assert_eq!(selection, 6..11);

        let (wrapped, selection) =
            apply_inline_format("hello world", 6..11, InlineFormat::Underline);
        let (unwrapped, selection) =
            apply_inline_format(&wrapped, selection, InlineFormat::Underline);
        assert_eq!(unwrapped, "hello world");
        assert_eq!(selection, 6..11);
    }

    #[test]
    fn toggling_a_format_on_then_off_then_on_again_round_trips_cleanly() {
        let (once, selection) = apply_inline_format("hello world", 6..11, InlineFormat::Bold);
        let (twice, selection) = apply_inline_format(&once, selection, InlineFormat::Bold);
        let (thrice, selection) = apply_inline_format(&twice, selection, InlineFormat::Bold);
        assert_eq!(thrice, "hello **world**");
        assert_eq!(selection, 8..13);
    }

    /// Regression test: Bold's marker (`**`) is a run of the same character as Italic's (`*`), so
    /// naively checking "does the marker sit right outside the selection" would see Bold's `**`
    /// and mistake it for two copies of Italic's `*`, corrupting the bold markup instead of
    /// nesting italic inside it.
    #[test]
    fn a_different_format_nests_around_bold_text_instead_of_misreading_its_markers() {
        let (bold, selection) = apply_inline_format("hello world", 6..11, InlineFormat::Bold);
        assert_eq!(bold, "hello **world**");

        let (nested, selection) = apply_inline_format(&bold, selection, InlineFormat::Italic);
        assert_eq!(nested, "hello ***world***");
        assert_eq!(selection, 9..14);
    }

    /// Drives `draw_note_editor` through a real [`egui::Context`] across two frames: the first
    /// establishes focus and a text selection (standing in for the user clicking in and
    /// dragging), the second delivers `key` held with Ctrl (as a real Linux/Windows keypress
    /// would set both `ctrl` and `command`) and returns the resulting note source.
    ///
    /// This exists to catch a real regression: egui's `TextEdit` hard-codes Ctrl+K/Ctrl+U to
    /// delete text (see the comment on `draw_note_editor`), the same keys used here for
    /// Hyperlink and Underline. A plain unit test of `apply_inline_format` can't see that
    /// conflict — it only shows up once a real `TextEdit` widget processes the keypress — so this
    /// drives the actual widget instead.
    fn press_ctrl_key_in_note_editor(
        source: &str,
        selection: std::ops::Range<usize>,
        key: egui::Key,
    ) -> String {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx); // markdown::highlight needs the app's named font families
        let id = egui::Id::new("test_note_editor");
        let mut source = source.to_owned();

        // Frame 1: draw the editor once and grab focus, then set the selection the user is
        // assumed to have made — there's no synthetic mouse-drag to select text with here.
        ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.memory_mut(|memory| memory.request_focus(id));
            draw_note_editor(ui, &mut source, id, &EditPalette::default(), 14.0);
        })
        .drop_without_applying_deltas();
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(selection.start),
                egui::text::CCursor::new(selection.end),
            )));
        state.store(&ctx, id);

        // Frame 2: deliver the keypress.
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                }],
                ..Default::default()
            },
            |ui| {
                draw_note_editor(ui, &mut source, id, &EditPalette::default(), 14.0);
            },
        )
        .drop_without_applying_deltas();

        source
    }

    #[test]
    fn ctrl_u_underlines_the_selection_instead_of_deleting_it() {
        let source = press_ctrl_key_in_note_editor("hello world", 6..11, egui::Key::U);
        assert_eq!(source, "hello <u>world</u>");
    }

    #[test]
    fn ctrl_k_turns_the_selection_into_a_hyperlink_instead_of_deleting_the_rest_of_the_line() {
        let source = press_ctrl_key_in_note_editor("see also world", 9..14, egui::Key::K);
        assert_eq!(source, "see also [world]()");
    }
}
