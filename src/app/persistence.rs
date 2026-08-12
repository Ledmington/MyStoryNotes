use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;

use my_story_notes_core::project::Project;

use super::App;

const FILE_EXTENSION: &str = "mystorynotes";

/// How long the "Saved in …" popup stays up after a save finishes before disappearing on its
/// own.
const SAVE_STATUS_DISPLAY_DURATION: Duration = Duration::from_secs(3);

/// Whether a save was requested by the user or fired on its own from the autosave timer —
/// [`App::draw_save_status`] labels the popup differently for each.
#[derive(Clone, Copy)]
pub(super) enum SaveKind {
    Manual,
    Auto,
}

/// What [`App::draw_save_status`] shows in the corner popup. `Saved` remembers when it was set
/// (rather than the draw function taking a snapshot) so the popup's remaining lifetime survives
/// across frames instead of resetting on every repaint.
pub(super) enum SaveStatus {
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

impl App {
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

    pub(super) fn new_project(&mut self) {
        log::info!("Created a new, unsaved project");
        self.set_project(Project::new());
    }

    pub(super) fn open_project(&mut self) {
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
    pub(super) fn open_project_from(&mut self, path: PathBuf) {
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

    pub(super) fn save_project(&mut self) {
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
    pub(super) fn check_autosave(&mut self, ctx: &egui::Context) {
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
    pub(super) fn process_pending_save(&mut self) {
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
    pub(super) fn draw_save_status(&mut self, ctx: &egui::Context) {
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
}
