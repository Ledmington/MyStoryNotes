use std::{
    fs, io,
    path::{Path, PathBuf},
};

use egui::{Color32, Visuals};
use serde::{Deserialize, Serialize};

/// Converts a stored `[r, g, b]` triple into the color type egui actually paints with.
pub fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}

/// Colors for the app's chrome (panels, buttons, selection, hyperlinks), applied on top of
/// egui's light or dark theme (matching this palette's own brightness — see
/// [`UiPalette::to_visuals`]) every frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPalette {
    pub window_background: [u8; 3],
    pub panel_background: [u8; 3],
    pub text: [u8; 3],
    pub accent: [u8; 3],
    pub hyperlink: [u8; 3],
}

impl Default for UiPalette {
    fn default() -> Self {
        Self {
            window_background: [27, 27, 27],
            panel_background: [27, 27, 27],
            text: [140, 140, 140],
            accent: [0, 92, 128],
            hyperlink: [90, 170, 255],
        }
    }
}

impl UiPalette {
    pub fn to_visuals(&self) -> Visuals {
        // Buttons and other widget chrome come from egui's own light/dark preset (`self` only
        // overrides window/panel/text/selection/hyperlink below) — picking the preset that
        // matches this palette's own brightness keeps that chrome legible instead of always
        // rendering dark-gray buttons, even under a light custom theme.
        let mut visuals = if is_light(self.panel_background) {
            Visuals::light()
        } else {
            Visuals::dark()
        };
        visuals.window_fill = rgb(self.window_background);
        visuals.panel_fill = rgb(self.panel_background);
        visuals.override_text_color = Some(rgb(self.text));
        visuals.selection.bg_fill = rgb(self.accent);
        visuals.hyperlink_color = rgb(self.hyperlink);
        visuals
    }
}

/// Whether `color` reads as light overall, by the standard luma formula.
fn is_light(color: [u8; 3]) -> bool {
    let luma =
        0.299 * f32::from(color[0]) + 0.587 * f32::from(color[1]) + 0.114 * f32::from(color[2]);
    luma > 128.0
}

/// Base font sizes, in points: the app's chrome (buttons, labels, panels), a rendered note's
/// body text (headings scale off of it), and a note's raw markdown source while editing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FontSizes {
    pub ui: f32,
    pub render: f32,
    pub edit: f32,
}

impl Default for FontSizes {
    fn default() -> Self {
        Self {
            ui: 13.0,
            render: 14.0,
            edit: 13.0,
        }
    }
}

impl FontSizes {
    /// Resizes `style`'s text styles around [`Self::ui`]: headings and small text (e.g. the "x"
    /// on notification popups) scale with it rather than staying fixed.
    pub fn apply_to_style(&self, style: &mut egui::Style) {
        use egui::TextStyle;

        let sizes = [
            (TextStyle::Small, self.ui * 0.75),
            (TextStyle::Body, self.ui),
            (TextStyle::Button, self.ui),
            (TextStyle::Heading, self.ui * 1.4),
            (TextStyle::Monospace, self.ui),
        ];

        for (text_style, size) in sizes {
            if let Some(font_id) = style.text_styles.get_mut(&text_style) {
                font_id.size = size;
            }
        }
    }
}

/// Colors for a rendered note: headings, bold text, inline code and links. Plain text uses the
/// app's [`UiPalette::text`] rather than a color of its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderPalette {
    pub heading: [u8; 3],
    pub bold: [u8; 3],
    pub code: [u8; 3],
    pub link: [u8; 3],
}

impl Default for RenderPalette {
    fn default() -> Self {
        Self {
            heading: [0x56, 0x9C, 0xD6],
            bold: [0xE0, 0xE0, 0xE0],
            code: [0xCE, 0x91, 0x78],
            link: [0x5A, 0xAA, 0xFF],
        }
    }
}

/// Colors for a note's raw markdown source while editing: headings, bold text, markup
/// punctuation (`#`, `**`, `` ` ``, `[]()`), inline code and links. Plain text uses the app's
/// [`UiPalette::text`] rather than a color of its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EditPalette {
    pub heading: [u8; 3],
    pub bold: [u8; 3],
    pub punctuation: [u8; 3],
    pub code: [u8; 3],
    pub link: [u8; 3],
}

impl Default for EditPalette {
    fn default() -> Self {
        Self {
            heading: [0x56, 0x9C, 0xD6],
            bold: [0xFF, 0xFF, 0xFF],
            punctuation: [0x80, 0x80, 0x80],
            code: [0xCE, 0x91, 0x78],
            link: [0x5A, 0xAA, 0xFF],
        }
    }
}

/// Tunable parameters for the graph view's force-directed layout, editable from the Settings
/// panel. Notes push and pull on each other like a Lennard-Jones potential: closer than their
/// equilibrium distance they repel, farther they attract, and the "strength" fields set how hard
/// that pull/push resists being moved away from equilibrium.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulationSettings {
    /// Equilibrium distance between two notes with no link between them.
    pub weak_distance: f32,
    /// How strongly unconnected notes resist drifting away from [`Self::weak_distance`].
    pub weak_strength: f32,
    /// Equilibrium distance between two linked notes.
    pub strong_distance: f32,
    /// How strongly linked notes resist drifting away from [`Self::strong_distance`].
    pub strong_strength: f32,
    /// How quickly motion settles down; higher values reach a resting layout faster but can look
    /// stiffer while animating. Also what keeps a densely-interconnected cluster of notes (which
    /// can't simultaneously satisfy every pairwise equilibrium distance in 2D, e.g. four notes
    /// all linked to each other) from gently drifting between near-equivalent shapes forever —
    /// high enough damping dissipates that residual motion fast enough to lock into one of them.
    pub damping: f32,
    /// How strongly the whole graph is pulled toward the center, keeping it from drifting off
    /// (or expanding off) the canvas.
    pub centering: f32,
}

impl Default for SimulationSettings {
    fn default() -> Self {
        Self {
            weak_distance: 200.0,
            weak_strength: 600.0,
            strong_distance: 100.0,
            strong_strength: 6_000.0,
            damping: 11.0,
            centering: 0.4,
        }
    }
}

/// How often the currently open project is saved automatically, editable from the Settings
/// panel. Only takes effect for a project that has already been saved once (and so has a file to
/// autosave to) — a brand-new, never-saved project is never autosaved out from under the user
/// without their having picked a location for it first.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutosaveSettings {
    pub enabled: bool,
    pub interval_minutes: u32,
}

impl Default for AutosaveSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_minutes: 5,
        }
    }
}

/// How many entries [`Settings::record_recent_project`] keeps in [`Settings::recent_projects`].
const RECENT_PROJECTS_LIMIT: usize = 10;

/// A minimal, non-photographic pattern drawn behind the graph view, purely so an empty or sparse
/// canvas doesn't feel quite so bare — see [`GraphBackground`]. Anchored in world space, so it
/// pans and zooms along with the graph rather than staying fixed on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GraphPattern {
    #[default]
    None,
    SquareGrid,
    TriangularGrid,
    Rays,
    Spiral,
}

impl GraphPattern {
    /// Every variant, in the order offered in the Settings panel.
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::SquareGrid,
        Self::TriangularGrid,
        Self::Rays,
        Self::Spiral,
    ];

    /// A short, human-readable label for the Settings panel.
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::SquareGrid => "Square grid",
            Self::TriangularGrid => "Triangular grid",
            Self::Rays => "Rays",
            Self::Spiral => "Spiral",
        }
    }
}

/// The graph view's background: a base color, editable directly (unlike every other UI color,
/// this one isn't derived from [`UiPalette::panel_background`], so the canvas can be tuned
/// independently of the rest of the chrome) plus an optional [`GraphPattern`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphBackground {
    pub color: [u8; 3],
    pub pattern: GraphPattern,
}

impl Default for GraphBackground {
    fn default() -> Self {
        Self {
            color: default_graph_background_color(UiPalette::default().panel_background),
            pattern: GraphPattern::default(),
        }
    }
}

/// A default graph-canvas color derived from `panel_background`: slightly darker, so nodes and
/// edges (both only subtly lighter than the panel) have something to stand out against. Used for
/// [`GraphBackground::default`] and to keep the canvas looking consistent whenever
/// [`Settings::apply_theme`] switches themes — `graph_background.color` is a plain, directly
/// editable field otherwise, so this is only ever a *starting point* the user can still tweak
/// away from afterward.
fn default_graph_background_color(panel_background: [u8; 3]) -> [u8; 3] {
    let darken = |channel: u8| (f32::from(channel) * 0.88).round() as u8;
    [
        darken(panel_background[0]),
        darken(panel_background[1]),
        darken(panel_background[2]),
    ]
}

/// The app's persisted preferences: the three color palettes, font sizes, graph physics
/// parameters, autosave interval, recently opened projects, and graph background, all editable
/// (or, for the recent-projects list, at least clickable) from the main window. Stored as a
/// single human-readable TOML file at `~/.my_story_notes`, independent of any story project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ui: UiPalette,
    pub render: RenderPalette,
    pub edit: EditPalette,
    pub font_size: FontSizes,
    pub simulation: SimulationSettings,
    pub autosave: AutosaveSettings,
    /// Most-recently-used project paths first, for the "Open Recent Project" menu.
    pub recent_projects: Vec<PathBuf>,
    pub graph_background: GraphBackground,
}

impl Settings {
    /// Records `path` as the most recently used project: moves it to the front if already
    /// present rather than duplicating it, and drops the oldest entry once there are more than
    /// `RECENT_PROJECTS_LIMIT`.
    pub fn record_recent_project(&mut self, path: PathBuf) {
        self.recent_projects.retain(|existing| existing != &path);
        self.recent_projects.insert(0, path);
        self.recent_projects.truncate(RECENT_PROJECTS_LIMIT);
    }

    /// Drops `path` from the recent-projects list, e.g. after it failed to open because the file
    /// has since moved or been deleted.
    pub fn forget_recent_project(&mut self, path: &Path) {
        self.recent_projects.retain(|existing| existing != path);
    }
}

impl Settings {
    /// Loads settings from `~/.my_story_notes/settings.toml`, falling back to defaults if the
    /// file is missing, unreadable, or invalid.
    pub fn load() -> Self {
        let Some(dir) = config_dir() else {
            log::warn!("Could not determine the home directory; settings will not persist");
            return Self::default();
        };

        migrate_legacy_file(&dir);

        let Ok(text) = fs::read_to_string(dir.join(SETTINGS_FILE)) else {
            log::debug!("No settings file at {}; using defaults", dir.display());
            return Self::default();
        };

        match toml::from_str(&text) {
            Ok(settings) => settings,
            Err(error) => {
                log::warn!("Ignoring invalid settings file: {error}");
                Self::default()
            }
        }
    }

    /// Saves settings to `~/.my_story_notes/settings.toml`, creating the directory if needed.
    pub fn save(&self) -> io::Result<()> {
        let dir = config_dir()
            .ok_or_else(|| io::Error::other("Could not determine the home directory"))?;

        fs::create_dir_all(&dir)?;

        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;

        fs::write(dir.join(SETTINGS_FILE), text)
    }
}

const SETTINGS_FILE: &str = "settings.toml";

fn config_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".my_story_notes"))
}

/// `~/.my_story_notes` used to be the settings file itself, before it became a directory holding
/// `settings.toml`. If that legacy file is still there, move its contents into the new layout.
fn migrate_legacy_file(dir: &std::path::Path) {
    if !dir.is_file() {
        return;
    }

    let Ok(text) = fs::read_to_string(dir) else {
        return;
    };

    if fs::remove_file(dir).is_ok() {
        match fs::create_dir_all(dir).and_then(|()| fs::write(dir.join(SETTINGS_FILE), &text)) {
            Ok(()) => log::info!(
                "Migrated legacy settings file into {}/{SETTINGS_FILE}",
                dir.display()
            ),
            Err(error) => log::warn!("Failed to migrate legacy settings file: {error}"),
        }
    }
}

/// A named, pre-built set of all three palettes, offered in the Settings panel as a starting
/// point that a color can still be tweaked away from afterward.
pub struct Theme {
    pub name: &'static str,
    pub ui: UiPalette,
    pub render: RenderPalette,
    pub edit: EditPalette,
}

/// The built-in themes offered in the Settings panel: at least one light and one dark, each
/// with a distinct, colorful accent palette rather than plain grayscale.
pub fn themes() -> Vec<Theme> {
    vec![
        Theme {
            name: "Dusk",
            ui: UiPalette {
                window_background: [24, 24, 32],
                panel_background: [30, 30, 40],
                text: [220, 220, 230],
                accent: [123, 97, 255],
                hyperlink: [139, 233, 253],
            },
            render: RenderPalette {
                heading: [255, 121, 198],
                bold: [241, 250, 140],
                code: [80, 250, 123],
                link: [139, 233, 253],
            },
            edit: EditPalette {
                heading: [255, 121, 198],
                bold: [248, 248, 242],
                punctuation: [98, 114, 164],
                code: [80, 250, 123],
                link: [139, 233, 253],
            },
        },
        Theme {
            name: "Midnight Forest",
            ui: UiPalette {
                window_background: [16, 26, 24],
                panel_background: [22, 34, 31],
                text: [214, 224, 216],
                accent: [255, 170, 66],
                hyperlink: [102, 217, 197],
            },
            render: RenderPalette {
                heading: [102, 217, 197],
                bold: [255, 213, 128],
                code: [255, 170, 66],
                link: [140, 209, 255],
            },
            edit: EditPalette {
                heading: [102, 217, 197],
                bold: [237, 245, 225],
                punctuation: [90, 122, 111],
                code: [255, 170, 66],
                link: [140, 209, 255],
            },
        },
        Theme {
            name: "Daybreak",
            ui: UiPalette {
                window_background: [250, 248, 240],
                panel_background: [240, 236, 224],
                text: [40, 38, 35],
                accent: [255, 140, 66],
                hyperlink: [35, 120, 190],
            },
            render: RenderPalette {
                heading: [176, 58, 110],
                bold: [20, 20, 20],
                code: [178, 89, 0],
                link: [35, 120, 190],
            },
            edit: EditPalette {
                heading: [176, 58, 110],
                bold: [20, 20, 20],
                punctuation: [150, 140, 120],
                code: [178, 89, 0],
                link: [35, 120, 190],
            },
        },
        Theme {
            name: "Meadow",
            ui: UiPalette {
                window_background: [246, 250, 240],
                panel_background: [234, 242, 224],
                text: [34, 46, 34],
                accent: [46, 139, 87],
                hyperlink: [30, 110, 160],
            },
            render: RenderPalette {
                heading: [46, 139, 87],
                bold: [25, 25, 20],
                code: [176, 108, 0],
                link: [30, 110, 160],
            },
            edit: EditPalette {
                heading: [46, 139, 87],
                bold: [25, 25, 20],
                punctuation: [130, 148, 120],
                code: [176, 108, 0],
                link: [30, 110, 160],
            },
        },
    ]
}

impl Settings {
    /// Replaces all three palettes with `theme`'s. The result is a plain copy, so any of its
    /// colors can still be tweaked individually afterward.
    pub fn apply_theme(&mut self, theme: &Theme) {
        self.ui = theme.ui.clone();
        self.render = theme.render.clone();
        self.edit = theme.edit.clone();
        self.graph_background.color = default_graph_background_color(self.ui.panel_background);
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
        assert!(dark.to_visuals().dark_mode);

        let light = UiPalette {
            panel_background: [240, 236, 224],
            ..UiPalette::default()
        };
        assert!(!light.to_visuals().dark_mode);
    }

    #[test]
    fn every_built_in_theme_s_widgets_match_its_own_panel_brightness() {
        for theme in themes() {
            let visuals = theme.ui.to_visuals();
            assert_eq!(
                visuals.dark_mode,
                !is_light(theme.ui.panel_background),
                "theme '{}' has a panel background that doesn't match its widget chrome's \
                 light/dark mode",
                theme.name
            );
        }
    }

    #[test]
    fn record_recent_project_moves_an_existing_entry_to_the_front_instead_of_duplicating_it() {
        let mut settings = Settings::default();
        settings.record_recent_project(PathBuf::from("a.mystorynotes"));
        settings.record_recent_project(PathBuf::from("b.mystorynotes"));
        settings.record_recent_project(PathBuf::from("a.mystorynotes"));

        assert_eq!(
            settings.recent_projects,
            vec![
                PathBuf::from("a.mystorynotes"),
                PathBuf::from("b.mystorynotes"),
            ]
        );
    }

    #[test]
    fn record_recent_project_drops_the_oldest_entry_past_the_limit() {
        let mut settings = Settings::default();

        for i in 0..RECENT_PROJECTS_LIMIT + 1 {
            settings.record_recent_project(PathBuf::from(format!("{i}.mystorynotes")));
        }

        assert_eq!(settings.recent_projects.len(), RECENT_PROJECTS_LIMIT);
        assert!(
            !settings
                .recent_projects
                .contains(&PathBuf::from("0.mystorynotes"))
        );
        assert_eq!(
            settings.recent_projects[0],
            PathBuf::from(format!("{RECENT_PROJECTS_LIMIT}.mystorynotes"))
        );
    }

    #[test]
    fn forget_recent_project_removes_only_the_matching_entry() {
        let mut settings = Settings::default();
        settings.record_recent_project(PathBuf::from("a.mystorynotes"));
        settings.record_recent_project(PathBuf::from("b.mystorynotes"));

        settings.forget_recent_project(&PathBuf::from("a.mystorynotes"));

        assert_eq!(
            settings.recent_projects,
            vec![PathBuf::from("b.mystorynotes")]
        );
    }
}
