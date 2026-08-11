use eframe::egui;

use crate::settings::Settings;

/// Draws the Settings panel: theme picker, color/font-size rows for each of the three palettes,
/// and the graph physics sliders. Saves to `~/.my_story_notes` whenever a value changes.
/// `show_settings` is cleared when the panel's close button is clicked.
pub fn draw(ui: &mut egui::Ui, settings: &mut Settings, show_settings: &mut bool) {
    ui.horizontal(|ui| {
        ui.heading("Settings");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = crate::fonts::icon_label(ui, crate::fonts::icon::TIMES, "Close");

            if ui.small_button(label).clicked() {
                *show_settings = false;
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
                settings.apply_theme(&theme);
                changed = true;
            }
        }
    });

    ui.add_space(12.0);
    ui.label("Interface");
    changed |= font_size_row(ui, "Font size", &mut settings.font_size.ui);
    changed |= color_row(ui, "Window background", &mut settings.ui.window_background);
    changed |= color_row(ui, "Panel background", &mut settings.ui.panel_background);
    changed |= color_row(ui, "Text", &mut settings.ui.text);
    changed |= color_row(ui, "Accent", &mut settings.ui.accent);
    changed |= color_row(ui, "Hyperlinks", &mut settings.ui.hyperlink);

    ui.add_space(12.0);
    ui.label("Render mode");
    changed |= font_size_row(ui, "Font size", &mut settings.font_size.render);
    changed |= color_row(ui, "Heading", &mut settings.render.heading);
    changed |= color_row(ui, "Bold", &mut settings.render.bold);
    changed |= color_row(ui, "Code", &mut settings.render.code);
    changed |= color_row(ui, "Links", &mut settings.render.link);

    ui.add_space(12.0);
    ui.label("Edit mode");
    changed |= font_size_row(ui, "Font size", &mut settings.font_size.edit);
    changed |= color_row(ui, "Heading", &mut settings.edit.heading);
    changed |= color_row(ui, "Bold", &mut settings.edit.bold);
    changed |= color_row(ui, "Punctuation", &mut settings.edit.punctuation);
    changed |= color_row(ui, "Code", &mut settings.edit.code);
    changed |= color_row(ui, "Links", &mut settings.edit.link);

    ui.add_space(12.0);
    ui.label("Autosave");
    changed |= ui
        .checkbox(&mut settings.autosave.enabled, "Autosave open project")
        .changed();
    if settings.autosave.enabled {
        changed |= ui
            .add(
                egui::Slider::new(&mut settings.autosave.interval_minutes, 1..=60)
                    .text("Interval (minutes)"),
            )
            .changed();
    }

    ui.add_space(12.0);
    ui.label("Graph physics");
    changed |= simulation_param_row(
        ui,
        "Unconnected distance",
        "How far apart two notes with no link between them settle.",
        &mut settings.simulation.weak_distance,
        50.0..=500.0,
    );
    changed |= simulation_param_row(
        ui,
        "Unconnected strength",
        "How strongly unconnected notes resist moving away from that distance.",
        &mut settings.simulation.weak_strength,
        50.0..=2_000.0,
    );
    changed |= simulation_param_row(
        ui,
        "Linked distance",
        "How far apart two linked notes settle.",
        &mut settings.simulation.strong_distance,
        20.0..=300.0,
    );
    changed |= simulation_param_row(
        ui,
        "Linked strength",
        "How strongly linked notes resist moving away from that distance.",
        &mut settings.simulation.strong_strength,
        500.0..=20_000.0,
    );
    changed |= simulation_param_row(
        ui,
        "Angular spread",
        "How strongly a note's links fan out around it instead of bunching together.",
        &mut settings.simulation.angular_repulsion,
        0.0..=2_000.0,
    );
    changed |= simulation_param_row(
        ui,
        "Damping",
        "How quickly motion settles down; higher values are stiffer but settle faster.",
        &mut settings.simulation.damping,
        0.5..=15.0,
    );
    changed |= simulation_param_row(
        ui,
        "Centering",
        "How strongly the whole graph is pulled toward the center of the canvas.",
        &mut settings.simulation.centering,
        0.0..=2.0,
    );

    ui.add_space(12.0);

    if ui.button("Reset to Defaults").clicked() {
        *settings = Settings::default();
        changed = true;
    }

    if changed && let Err(error) = settings.save() {
        log::error!("Failed to save settings: {error}");
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
