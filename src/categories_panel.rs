use eframe::egui;

use my_story_notes_core::project::{Category, Project};

/// A freshly picked default color for a newly added category, distinct enough from the graph
/// view's own default (unassigned) node color to be visibly a "real" category right away.
const NEW_CATEGORY_COLOR: [u8; 3] = [120, 120, 200];

/// Caps the window's height at this fraction of the app window's own height, so it grows to fit
/// however many categories there are but a long list scrolls within the window rather than the
/// window growing to fill the screen.
const MAX_HEIGHT_FRACTION: f32 = 0.5;

/// Draws the "Note Categories" window: add, recolor, rename and delete the project's categories
/// (see [`my_story_notes_core::project::Category`]), which color notes in the graph view.
/// Per-project rather than an app-wide setting (unlike [`crate::settings_panel`]) — categories are
/// read from and written straight to `project.categories`, saved along with everything else the
/// next time the project itself is saved. `show` is cleared when the window's native title-bar
/// close button is clicked, or Escape is pressed while it's open.
pub fn draw(ctx: &egui::Context, project: &mut Project, show: &mut bool) {
    if !*show {
        return;
    }

    let mut rename: Option<(String, String)> = None;
    let mut delete: Option<String> = None;
    let mut add: Option<String> = None;
    let mut escape_pressed = false;

    // A separate flag for `Window::open` rather than `show` itself: `open` needs its own `&mut
    // bool` borrow for the whole `show` call below, which would conflict with also mutating
    // `*show` for the Escape check inside the content closure.
    let mut still_open = true;

    let max_height = ctx.input(|input| input.viewport_rect().height()) * MAX_HEIGHT_FRACTION;

    egui::Window::new("Note Categories")
        .open(&mut still_open)
        .collapsible(false)
        .resizable(true)
        .default_width(340.0)
        .min_size([260.0, 160.0])
        .max_height(max_height)
        .show(ctx, |ui| {
            escape_pressed = ui.input(|input| input.key_pressed(egui::Key::Escape));

            if project.categories.is_empty() {
                ui.label("No categories yet.");
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                for category in &mut project.categories {
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgb(&mut category.color);

                        if let Some(new_name) = rename_field(ui, category) {
                            rename = Some((category.name.clone(), new_name));
                        }

                        let delete_label =
                            crate::fonts::icon_label(ui, crate::fonts::icon::TRASH, "Delete");
                        if ui.small_button(delete_label).clicked() {
                            delete = Some(category.name.clone());
                        }
                    });
                }
            });

            ui.separator();
            add = add_category_row(ui);
        });

    *show = still_open && !escape_pressed;

    if let Some((old_name, new_name)) = rename
        && let Err(error) = project.rename_category(&old_name, &new_name)
    {
        log::warn!("Failed to rename category: {error}");
    }

    if let Some(name) = delete {
        log::info!("Deleted category '{name}'");
        project.delete_category(&name);
    }

    if let Some(name) = add
        && let Err(error) = project.add_category(&name, NEW_CATEGORY_COLOR)
    {
        log::warn!("Failed to add category: {error}");
    }
}

/// An existing category's name field, for [`draw`]'s per-category rows. Edits go into a buffer
/// kept in [`egui::Context::data`], separate from `category.name`, for as long as the field has
/// focus — so intermediate keystrokes (including a transient empty string while the user
/// backspaces the whole name before retyping it) are never themselves validated or committed.
/// Only once the field loses focus does it return the trimmed result, if it actually changed and
/// isn't empty, for the caller to commit via [`Project::rename_category`]. Without this
/// deferral, committing (and so validating) on every keystroke made backspacing a name all the
/// way out repeatedly fail with a "name cannot be empty" error, one per keystroke, and snap the
/// field's visible text back to the last valid value on the very next frame.
fn rename_field(ui: &mut egui::Ui, category: &Category) -> Option<String> {
    let id = ui.make_persistent_id(("category_name", &category.name));
    let has_focus = ui.memory(|memory| memory.has_focus(id));

    let mut buffer = if has_focus {
        ui.ctx()
            .data(|data| data.get_temp::<String>(id))
            .unwrap_or_else(|| category.name.clone())
    } else {
        category.name.clone()
    };

    let response = ui.add(egui::TextEdit::singleline(&mut buffer).id(id));

    if response.has_focus() {
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, buffer.clone()));
    }

    if response.lost_focus() {
        ui.ctx().data_mut(|data| data.remove_temp::<String>(id));

        let trimmed = buffer.trim();
        if !trimmed.is_empty() && trimmed != category.name {
            return Some(trimmed.to_owned());
        }
    }

    None
}

/// The "add a category" row: an empty text field with hint text (rather than a pre-filled
/// placeholder name the user has to notice and clear first) plus an "Add" button. Commits — and
/// clears the field — only on that button or Enter, unlike [`rename_field`]'s commit-on-any-blur:
/// simply clicking elsewhere in the window (e.g. a different row's Delete button) shouldn't
/// silently create a category out of whatever was left half-typed here. Returns the trimmed name
/// once committed, if non-empty.
fn add_category_row(ui: &mut egui::Ui) -> Option<String> {
    let id = ui.make_persistent_id("new_category_name");
    let mut buffer = ui
        .ctx()
        .data(|data| data.get_temp::<String>(id))
        .unwrap_or_default();

    let mut commit = false;

    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut buffer)
                .id(id)
                .hint_text("New category name"),
        );

        commit |= response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));

        let add_label = crate::fonts::icon_label(ui, crate::fonts::icon::CHECK, "Add");
        commit |= ui.small_button(add_label).clicked();
    });

    if commit {
        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            None
        } else {
            ui.ctx().data_mut(|data| data.remove_temp::<String>(id));
            Some(trimmed.to_owned())
        }
    } else {
        ui.ctx().data_mut(|data| data.insert_temp(id, buffer));
        None
    }
}
