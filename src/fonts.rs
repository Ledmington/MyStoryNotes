use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Context, FontData, FontDefinitions, FontFamily, FontId, TextStyle, Ui, WidgetText};

const SERIF: &str = "serif";
const SERIF_BOLD: &str = "serif-bold";
const SERIF_ITALIC: &str = "serif-italic";
const SERIF_BOLD_ITALIC: &str = "serif-bold-italic";
const MONO: &str = "mono";
const MONO_BOLD: &str = "mono-bold";
const MONO_ITALIC: &str = "mono-italic";
const MONO_BOLD_ITALIC: &str = "mono-bold-italic";
const ICONS: &str = "icons";

/// Glyphs from the bundled Font Awesome 4 icon font, for use with [`icon_label`] and
/// [`icon_only`].
pub mod icon {
    pub const PENCIL: char = '\u{f040}';
    pub const COG: char = '\u{f013}';
    pub const TIMES: char = '\u{f00d}';
    pub const CHECK: char = '\u{f00c}';
    pub const SEARCH: char = '\u{f002}';
    pub const SEARCH_PLUS: char = '\u{f00e}';
    pub const SEARCH_MINUS: char = '\u{f010}';
    pub const CROSSHAIRS: char = '\u{f05b}';
    pub const BULLSEYE: char = '\u{f140}';
    pub const ARROW_UP: char = '\u{f062}';
    pub const ARROW_DOWN: char = '\u{f063}';
    pub const ARROW_LEFT: char = '\u{f060}';
    pub const ARROW_RIGHT: char = '\u{f061}';
    pub const BOOK: char = '\u{f02d}';
    pub const PENCIL_SQUARE: char = '\u{f044}';
    pub const TRASH: char = '\u{f1f8}';
    pub const TAGS: char = '\u{f02c}';
}

/// Registers the fonts this app bundles: a serif family (render mode) and a monospace family
/// (edit mode), each with regular, bold, italic and bold-italic weights, plus an icon font used
/// for button glyphs instead of emoji. Bundling our own fonts, rather than relying on egui's
/// default font, is what lets bold and italic text actually render as bold and italic instead of
/// merely changing color, and lets buttons use real icon glyphs instead of emoji characters.
pub fn install(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    let families: [(&str, &str, &[u8]); 9] = [
        (
            SERIF,
            "DejaVuSerif",
            include_bytes!("../assets/fonts/DejaVuSerif.ttf"),
        ),
        (
            SERIF_BOLD,
            "DejaVuSerif-Bold",
            include_bytes!("../assets/fonts/DejaVuSerif-Bold.ttf"),
        ),
        (
            SERIF_ITALIC,
            "DejaVuSerif-Italic",
            include_bytes!("../assets/fonts/DejaVuSerif-Italic.ttf"),
        ),
        (
            SERIF_BOLD_ITALIC,
            "DejaVuSerif-BoldItalic",
            include_bytes!("../assets/fonts/DejaVuSerif-BoldItalic.ttf"),
        ),
        (
            MONO,
            "DejaVuSansMono",
            include_bytes!("../assets/fonts/DejaVuSansMono.ttf"),
        ),
        (
            MONO_BOLD,
            "DejaVuSansMono-Bold",
            include_bytes!("../assets/fonts/DejaVuSansMono-Bold.ttf"),
        ),
        (
            MONO_ITALIC,
            "DejaVuSansMono-Oblique",
            include_bytes!("../assets/fonts/DejaVuSansMono-Oblique.ttf"),
        ),
        (
            MONO_BOLD_ITALIC,
            "DejaVuSansMono-BoldOblique",
            include_bytes!("../assets/fonts/DejaVuSansMono-BoldOblique.ttf"),
        ),
        (
            ICONS,
            "FontAwesome",
            include_bytes!("../assets/fonts/FontAwesome.ttf"),
        ),
    ];

    for (family, font_name, bytes) in families {
        fonts
            .font_data
            .insert(font_name.to_owned(), Arc::new(FontData::from_static(bytes)));
        fonts
            .families
            .insert(FontFamily::Name(family.into()), vec![font_name.to_owned()]);
    }

    ctx.set_fonts(fonts);
}

/// The serif font used to render notes, in the weight/style matching `bold`/`italic`.
pub fn serif(size: f32, bold: bool, italic: bool) -> FontId {
    FontId::new(
        size,
        FontFamily::Name(
            pick(
                SERIF,
                SERIF_BOLD,
                SERIF_ITALIC,
                SERIF_BOLD_ITALIC,
                bold,
                italic,
            )
            .into(),
        ),
    )
}

/// The monospace font used to edit notes' raw markdown, in the weight/style matching
/// `bold`/`italic`.
pub fn mono(size: f32, bold: bool, italic: bool) -> FontId {
    FontId::new(
        size,
        FontFamily::Name(pick(MONO, MONO_BOLD, MONO_ITALIC, MONO_BOLD_ITALIC, bold, italic).into()),
    )
}

/// A button label combining an [`icon`] glyph with text, e.g. `icon_label(ui, icon::PENCIL,
/// "Edit")`, so buttons can use real icons instead of emoji characters.
pub fn icon_label(ui: &Ui, icon: char, label: &str) -> WidgetText {
    let size = TextStyle::Button.resolve(ui.style()).size;
    let color = ui.visuals().text_color();

    let mut job = LayoutJob::default();
    job.append(
        &icon.to_string(),
        0.0,
        TextFormat {
            font_id: FontId::new(size, FontFamily::Name(ICONS.into())),
            color,
            ..Default::default()
        },
    );
    job.append(
        &format!(" {label}"),
        0.0,
        TextFormat {
            font_id: TextStyle::Button.resolve(ui.style()),
            color,
            ..Default::default()
        },
    );

    job.into()
}

/// A button label showing only an [`icon`] glyph and no text, e.g. `icon_only(ui,
/// icon::SEARCH_PLUS)`, for small square controls where a text label would not fit.
pub fn icon_only(ui: &Ui, icon: char) -> WidgetText {
    let size = TextStyle::Button.resolve(ui.style()).size;
    let color = ui.visuals().text_color();

    let mut job = LayoutJob::default();
    job.append(
        &icon.to_string(),
        0.0,
        TextFormat {
            font_id: FontId::new(size, FontFamily::Name(ICONS.into())),
            color,
            ..Default::default()
        },
    );

    job.into()
}

fn pick<'a>(
    regular: &'a str,
    bold_name: &'a str,
    italic_name: &'a str,
    bold_italic: &'a str,
    bold: bool,
    italic: bool,
) -> &'a str {
    match (bold, italic) {
        (false, false) => regular,
        (true, false) => bold_name,
        (false, true) => italic_name,
        (true, true) => bold_italic,
    }
}
