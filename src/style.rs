//! Converts `my_story_notes_core::settings` data into the types egui actually paints with. Kept
//! separate from that data (rather than as inherent methods on it) since core has no egui
//! dependency at all — these can only exist on this side of the crate boundary.

use eframe::egui;

use my_story_notes_core::settings::{FontSizes, UiPalette, is_light};

/// Converts a stored `[r, g, b]` triple into the color type egui actually paints with.
pub fn rgb(color: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(color[0], color[1], color[2])
}

/// Builds egui visuals from `palette`, applied on top of egui's own light or dark theme (matching
/// the palette's own brightness) every frame.
pub fn to_visuals(palette: &UiPalette) -> egui::Visuals {
    // Buttons and other widget chrome come from egui's own light/dark preset (`palette` only
    // overrides window/panel/text/selection/hyperlink below) — picking the preset that matches
    // the palette's own brightness keeps that chrome legible instead of always rendering
    // dark-gray buttons, even under a light custom theme.
    let mut visuals = if is_light(palette.panel_background) {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    visuals.window_fill = rgb(palette.window_background);
    visuals.panel_fill = rgb(palette.panel_background);
    visuals.override_text_color = Some(rgb(palette.text));
    visuals.selection.bg_fill = rgb(palette.accent);
    visuals.hyperlink_color = rgb(palette.hyperlink);
    visuals
}

/// Resizes `style`'s text styles around `sizes.ui`: headings and small text (e.g. the "x" on
/// notification popups) scale with it rather than staying fixed.
pub fn apply_font_sizes(style: &mut egui::Style, sizes: &FontSizes) {
    use egui::TextStyle;

    let scales = [
        (TextStyle::Small, sizes.ui * 0.75),
        (TextStyle::Body, sizes.ui),
        (TextStyle::Button, sizes.ui),
        (TextStyle::Heading, sizes.ui * 1.4),
        (TextStyle::Monospace, sizes.ui),
    ];

    for (text_style, size) in scales {
        if let Some(font_id) = style.text_styles.get_mut(&text_style) {
            font_id.size = size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test: `to_visuals` used to build every palette on top of `Visuals::dark()`
    /// unconditionally, so a light custom theme's buttons and other widget chrome still rendered
    /// with dark-gray fills instead of matching the palette.
    #[test]
    fn to_visuals_picks_dark_mode_or_light_mode_widgets_to_match_the_palette() {
        let dark = UiPalette {
            panel_background: [27, 27, 27],
            ..UiPalette::default()
        };
        assert!(to_visuals(&dark).dark_mode);

        let light = UiPalette {
            panel_background: [240, 236, 224],
            ..UiPalette::default()
        };
        assert!(!to_visuals(&light).dark_mode);
    }

    #[test]
    fn every_built_in_theme_s_widgets_match_its_own_panel_brightness() {
        for theme in my_story_notes_core::settings::themes() {
            let visuals = to_visuals(&theme.ui);
            assert_eq!(
                visuals.dark_mode,
                !is_light(theme.ui.panel_background),
                "theme '{}' has a panel background that doesn't match its widget chrome's \
                 light/dark mode",
                theme.name
            );
        }
    }
}
