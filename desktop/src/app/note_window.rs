use eframe::egui;

use my_story_notes_core::project::{NoteId, Project};
use my_story_notes_core::settings::Settings;

use crate::{markdown, note_editor};

use super::{Cell, CellMode, DELETE_NOTE_SHORTCUTS};

/// An action requested from a note's cell, for the caller to act on — each one touches
/// [`crate::app::App`] state that `draw_cell` doesn't have access to (`draw_cell` only has the one
/// note it's drawing, not the whole project's UI state), so it's handed back up rather than
/// handled here.
pub(super) enum CellAction {
    Rename,
    Delete,
    Close,
}

/// Draws the currently open note as a floating, resizable, movable window on top of the graph
/// view (which otherwise always occupies the whole central area — see [`eframe::App::ui`]),
/// rather than splitting the graph into a side panel to make room for it. The window keeps a
/// fixed [`egui::Id`] rather than one derived from its title, so resizing or moving it persists
/// across switching to a different note (e.g. by clicking a link) instead of resetting. Its
/// native title-bar close button reports [`CellAction::Close`], same as the in-cell "Close"
/// button used to.
pub(super) fn draw_note_window(
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
    // Separate from the `CellAction::Close` the caller acts on: `Window::open` needs its own
    // `&mut bool` borrow for the whole `show` call below, which would conflict with also setting
    // `action` from inside the content closure.
    let mut still_open = true;

    egui::Window::new(title)
        .id(egui::Id::new("note_window"))
        .open(&mut still_open)
        .resizable(true)
        .collapsible(false)
        .default_size([420.0, 520.0])
        .min_size([280.0, 200.0])
        .show(ctx, |ui| {
            action = draw_cell(ui, project, cell, settings);
        });

    if !still_open {
        action = Some(CellAction::Close);
    }

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
        });

        if let Some(note) = project.notes.get(usize::from(cell.note_index))
            && my_story_notes_core::markdown::title(&note.source).as_deref()
                != Some(note.name.as_str())
        {
            ui.label(
                egui::RichText::new(format!("Saved as \"{}\"", note.name))
                    .italics()
                    .weak(),
            );
        }

        // The manuscript note isn't drawn as a node in the graph view at all (see
        // `my_story_notes_core::graph::resolve_edges`), so a category assigned to it would have
        // nothing to color.
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
                            project,
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
