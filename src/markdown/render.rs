use egui::Ui;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

use crate::fonts;
use crate::settings::{self, RenderPalette};

/// A run of text sharing the same inline styling, the smallest unit rendering flows.
#[derive(Clone)]
struct Span {
    text: String,
    bold: bool,
    italic: bool,
    /// Set by a `<u>...</u>` pair — CommonMark has no native underline syntax, so this is the
    /// one inline construct recognized as raw HTML rather than markdown proper.
    underline: bool,
    code: bool,
    link: Option<String>,
}

/// A block-level element, holding one or more lines of inline [`Span`]s. A block always starts
/// on its own line; a new line within a block is only forced by an explicit hard break.
enum Block {
    Heading {
        level: HeadingLevel,
        lines: Vec<Vec<Span>>,
    },
    Paragraph {
        lines: Vec<Vec<Span>>,
    },
}

impl Block {
    fn lines_mut(&mut self) -> &mut Vec<Vec<Span>> {
        match self {
            Block::Heading { lines, .. } | Block::Paragraph { lines } => lines,
        }
    }
}

/// The plain text of `source`'s first heading, if it has one — e.g. `# Mira Solenne` gives
/// `Some("Mira Solenne")`. Used to tell whether a note's displayed title still matches its actual
/// (linking) name, since nothing keeps the two in sync automatically: a rename only updates the
/// name, not whatever heading text happens to be written in the note's own source.
pub fn title(source: &str) -> Option<String> {
    collect_blocks(source)
        .into_iter()
        .find_map(|block| match block {
            Block::Heading { lines, .. } => Some(
                lines
                    .iter()
                    .flatten()
                    .map(|span| span.text.as_str())
                    .collect(),
            ),
            Block::Paragraph { .. } => None,
        })
}

/// Renders `source` as markdown into `ui`, using `palette` for headings/bold/code/link colors
/// and `body_size` as the base font size (headings scale off of it).
pub fn render(
    ui: &mut Ui,
    source: &str,
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
                render_lines(ui, lines, Some(size), palette, body_size, &mut clicked_link);
                ui.add_space(4.0);
            }

            Block::Paragraph { lines } => {
                render_lines(ui, lines, None, palette, body_size, &mut clicked_link);
            }
        }
    }

    clicked_link
}

/// Parses `source` into a flat sequence of blocks, resolving inline styling and links onto
/// [`Span`]s so that a block's content can be rendered as flowing text instead of one widget
/// per markdown event.
fn collect_blocks(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current: Option<Block> = None;

    let mut bold = false;
    let mut italic = false;
    let mut underline = false;
    let mut link: Option<String> = None;

    for event in Parser::new(source) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some(Block::Heading {
                    level,
                    lines: vec![Vec::new()],
                });
            }

            Event::End(TagEnd::Heading(_)) => {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
            }

            Event::Start(Tag::Paragraph | Tag::Item) => {
                current = Some(Block::Paragraph {
                    lines: vec![Vec::new()],
                });
            }

            Event::End(TagEnd::Paragraph | TagEnd::Item) => {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
            }

            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::Link { dest_url, .. }) => link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => link = None,

            Event::InlineHtml(html) => match html.trim() {
                "<u>" => underline = true,
                "</u>" => underline = false,
                _ => {}
            },

            Event::Text(text) => {
                if let Some(block) = &mut current {
                    block.lines_mut().last_mut().unwrap().push(Span {
                        text: text.to_string(),
                        bold,
                        italic,
                        underline,
                        code: false,
                        link: link.clone(),
                    });
                }
            }

            Event::Code(code) => {
                if let Some(block) = &mut current {
                    block.lines_mut().last_mut().unwrap().push(Span {
                        text: code.to_string(),
                        bold,
                        italic,
                        underline,
                        code: true,
                        link: link.clone(),
                    });
                }
            }

            Event::SoftBreak => {
                if let Some(block) = &mut current {
                    block.lines_mut().last_mut().unwrap().push(Span {
                        text: " ".to_owned(),
                        bold: false,
                        italic: false,
                        underline: false,
                        code: false,
                        link: None,
                    });
                }
            }

            Event::HardBreak => {
                if let Some(block) = &mut current {
                    block.lines_mut().push(Vec::new());
                }
            }

            _ => {}
        }
    }

    if let Some(block) = current.take() {
        blocks.push(block);
    }

    blocks
}

fn render_lines(
    ui: &mut Ui,
    lines: &[Vec<Span>],
    heading_size: Option<f32>,
    palette: &RenderPalette,
    body_size: f32,
    clicked_link: &mut Option<String>,
) {
    for line in lines {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            for span in line {
                render_span(ui, span, heading_size, palette, body_size, clicked_link);
            }
        });
    }
}

fn render_span(
    ui: &mut Ui,
    span: &Span,
    heading_size: Option<f32>,
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
    } else if span.link.is_some() {
        Some(palette.link)
    } else if span.bold {
        Some(palette.bold)
    } else {
        None
    };

    if let Some(color) = color {
        rich_text = rich_text.color(settings::rgb(color));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_returns_the_first_heading_as_plain_text() {
        assert_eq!(
            title("# Mira Solenne\n\nSome body text."),
            Some("Mira Solenne".to_owned())
        );
    }

    #[test]
    fn title_flattens_inline_styling_in_the_heading() {
        assert_eq!(
            title("## The *Cartographer's* Debt\n\nBody."),
            Some("The Cartographer's Debt".to_owned())
        );
    }

    #[test]
    fn title_is_none_without_a_heading() {
        assert_eq!(title("just a paragraph, no heading"), None);
        assert_eq!(title(""), None);
    }
}
