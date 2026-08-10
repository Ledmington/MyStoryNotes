use eframe::egui;

use crate::{graph, logging::Notifications, markdown, project::Project, settings::Settings};

const FILE_EXTENSION: &str = "mystorynotes";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellMode {
    Rendered,
    Editing,
}

struct Cell {
    note_index: usize,
    mode: CellMode,
}

pub struct App {
    project: Option<Project>,
    open_cell: Option<Cell>,
    new_note_dialog: bool,
    new_note_name: String,
    settings: Settings,
    show_settings: bool,
    notifications: Notifications,
    graph_sim: graph::Simulation,
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
            settings: Settings::load(),
            show_settings: false,
            notifications,
            graph_sim: graph::Simulation::new(),
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

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New Project").clicked() {
                    self.new_project();
                }

                if ui.button("Open Project").clicked() {
                    self.open_project();
                }

                if self.project.is_some() {
                    if ui.button("Save Project").clicked() {
                        self.save_project();
                    }

                    ui.separator();

                    if ui.button("+ New Note").clicked() {
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
                } else {
                    for (index, note) in project.notes.iter().enumerate() {
                        let is_open = self
                            .open_cell
                            .as_ref()
                            .is_some_and(|cell| cell.note_index == index);

                        if ui.selectable_label(is_open, &note.name).clicked() {
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
                    open_note,
                    &self.settings.ui,
                    &mut self.graph_sim,
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
                        open_note,
                        &self.settings.ui,
                        &mut self.graph_sim,
                    ) {
                        cell.note_index = clicked;
                        cell.mode = CellMode::Rendered;
                    }
                });

            draw_cell(ui, project, cell, &self.settings);
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
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    let label = crate::fonts::icon_label(ui, crate::fonts::icon::PENCIL, "Edit");

                    if ui.small_button(label).clicked() {
                        cell.mode = CellMode::Editing;
                    }
                });

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
                let Some(note) = project.notes.get_mut(cell.note_index) else {
                    return;
                };

                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                    let mut layout_job = markdown::highlight(
                        ui,
                        buf.as_str(),
                        &settings.edit,
                        settings.font_size.edit,
                    );
                    layout_job.wrap.max_width = wrap_width;
                    ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
                };

                let response = ui.add(
                    egui::TextEdit::multiline(&mut note.source)
                        .desired_width(f32::INFINITY)
                        .desired_rows(15)
                        .layouter(&mut layouter),
                );

                if response.lost_focus() {
                    cell.mode = CellMode::Rendered;
                }
            }
        })
        .response;

    if !link_clicked && cell.mode == CellMode::Rendered && response.clicked() {
        cell.mode = CellMode::Editing;
    }
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
