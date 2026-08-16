use super::{EditPalette, RenderPalette, UiPalette};

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
