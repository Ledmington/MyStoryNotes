use egui::Ui;
use pulldown_cmark::HeadingLevel;

use my_story_notes_core::markdown::{Block, List, Span, collect_blocks};
use my_story_notes_core::project::Project;
use my_story_notes_core::settings::RenderPalette;

use crate::fonts;
use crate::style;

/// Renders `source` as markdown into `ui`, using `palette` for headings/bold/code/link colors
/// and `body_size` as the base font size (headings scale off of it). A link to another note in
/// `project` that has a category assigned is colored with that category's color instead of
/// `palette.link`, the same category color a reader would see on that note's node in the graph
/// view — so a hyperlink hints at what kind of note it leads to before it's clicked.
pub fn render(
    ui: &mut Ui,
    source: &str,
    project: &Project,
    palette: &RenderPalette,
    body_size: f32,
) -> Option<String> {
    let blocks = collect_blocks(source);
    let mut clicked_link = None;

    for block in &blocks {
        match block {
            Block::Heading { level, lines } => {
                let size = match level {
                    HeadingLevel::H1 => body_size * 2.0,
                    HeadingLevel::H2 => body_size * 1.7,
                    HeadingLevel::H3 => body_size * 1.4,
                    _ => body_size * 1.25,
                };

                ui.add_space(4.0);
                render_lines(
                    ui,
                    lines,
                    Some(size),
                    project,
                    palette,
                    body_size,
                    &mut clicked_link,
                );
                ui.add_space(4.0);
            }

            Block::Paragraph { lines } => {
                render_lines(
                    ui,
                    lines,
                    None,
                    project,
                    palette,
                    body_size,
                    &mut clicked_link,
                );
            }

            Block::List(list) => {
                render_list(ui, list, project, palette, body_size, &mut clicked_link);
            }
        }
    }

    clicked_link
}

/// Renders a list's items each as a bullet (`•`) or, for an ordered list, a number counting up
/// from [`List::ordered_start`] — followed by the item's own text. A sub-list nested inside an
/// item (see [`my_story_notes_core::markdown::ListItem::sublists`]) recurses into the item's own
/// indented column, so nesting depth falls out of the recursion itself rather than being tracked
/// explicitly.
fn render_list(
    ui: &mut Ui,
    list: &List,
    project: &Project,
    palette: &RenderPalette,
    body_size: f32,
    clicked_link: &mut Option<String>,
) {
    for (index, item) in list.items.iter().enumerate() {
        let marker = match list.ordered_start {
            Some(start) => format!("{}.", start + index as u64),
            None => "•".to_owned(),
        };

        ui.horizontal_top(|ui| {
            ui.label(egui::RichText::new(marker).size(body_size));
            ui.vertical(|ui| {
                render_lines(
                    ui,
                    &item.lines,
                    None,
                    project,
                    palette,
                    body_size,
                    clicked_link,
                );

                for sublist in &item.sublists {
                    render_list(ui, sublist, project, palette, body_size, clicked_link);
                }
            });
        });
    }
}

fn render_lines(
    ui: &mut Ui,
    lines: &[Vec<Span>],
    heading_size: Option<f32>,
    project: &Project,
    palette: &RenderPalette,
    body_size: f32,
    clicked_link: &mut Option<String>,
) {
    for line in lines {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            for span in line {
                render_span(
                    ui,
                    span,
                    heading_size,
                    project,
                    palette,
                    body_size,
                    clicked_link,
                );
            }
        });
    }
}

fn render_span(
    ui: &mut Ui,
    span: &Span,
    heading_size: Option<f32>,
    project: &Project,
    palette: &RenderPalette,
    body_size: f32,
    clicked_link: &mut Option<String>,
) {
    let bold = span.bold || heading_size.is_some();
    let size = heading_size.unwrap_or(body_size);
    let mut rich_text = egui::RichText::new(&span.text);

    if span.code {
        rich_text = rich_text.font(fonts::mono(size, bold, span.italic));
    } else {
        rich_text = rich_text.font(fonts::serif(size, bold, span.italic));
    }

    // Priority mirrors the edit-mode highlighter: code beats heading beats link beats bold.
    let color = if span.code {
        Some(palette.code)
    } else if heading_size.is_some() {
        Some(palette.heading)
    } else if let Some(target) = &span.link {
        Some(
            project
                .category_color_by_name(target)
                .unwrap_or(palette.link),
        )
    } else if span.bold {
        Some(palette.bold)
    } else {
        None
    };

    if let Some(color) = color {
        rich_text = rich_text.color(style::rgb(color));
    }

    if span.underline {
        rich_text = rich_text.underline();
    }

    if let Some(target) = &span.link {
        rich_text = rich_text.underline();

        let response = ui
            .add(egui::Label::new(rich_text).sense(egui::Sense::click()))
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if response.clicked() {
            *clicked_link = Some(target.clone());
        }
    } else {
        ui.label(rich_text);
    }
}
