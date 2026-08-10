use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Stroke, Ui};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

use crate::fonts;
use crate::settings::{self, EditPalette, RenderPalette};

/// A run of text sharing the same inline styling, the smallest unit rendering flows.
#[derive(Clone)]
struct Span {
    text: String,
    bold: bool,
    italic: bool,
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

/// Extracts every link destination in `source`, in document order (including duplicates and
/// destinations that aren't other notes' names). Used to build the graph view's edges.
pub fn extract_links(source: &str) -> Vec<String> {
    Parser::new(source)
        .filter_map(|event| match event {
            Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.to_string()),
            _ => None,
        })
        .collect()
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

            Event::Text(text) => {
                if let Some(block) = &mut current {
                    block.lines_mut().last_mut().unwrap().push(Span {
                        text: text.to_string(),
                        bold,
                        italic,
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

/// Resolved colors and sizes for [`highlight`]: the configurable [`EditPalette`] plus the app's
/// ambient text color (for plain text, which has no dedicated palette entry) and the configured
/// font size (used for both body text and inline code, matching a plain-text editor).
struct Palette {
    default: Color32,
    bold: Color32,
    punctuation: Color32,
    heading: Color32,
    code: Color32,
    link: Color32,
    body_size: f32,
    code_size: f32,
}

impl Palette {
    fn from_ui(ui: &Ui, edit: &EditPalette, size: f32) -> Self {
        Self {
            default: ui.visuals().text_color(),
            bold: settings::rgb(edit.bold),
            punctuation: settings::rgb(edit.punctuation),
            heading: settings::rgb(edit.heading),
            code: settings::rgb(edit.code),
            link: settings::rgb(edit.link),
            body_size: size,
            code_size: size,
        }
    }
}

/// Which inline/block constructs the current cursor position in [`highlight`] is nested inside.
#[derive(Default)]
struct Depths {
    heading: u32,
    bold: u32,
    italic: u32,
    link: u32,
}

/// Lays out `source` for the raw-text editor, coloring markdown syntax the way VS Code colors
/// markdown source: heading lines and their `#` markers in blue, emphasis/code/link markup
/// characters dimmed, and emphasized/code/link content styled to match.
pub fn highlight(ui: &Ui, source: &str, edit: &EditPalette, size: f32) -> LayoutJob {
    let palette = Palette::from_ui(ui, edit, size);
    let mut job = LayoutJob::default();
    let mut cursor = 0usize;
    let mut depths = Depths::default();

    for (event, range) in Parser::new(source).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                append_upto(
                    &mut job,
                    &mut cursor,
                    source,
                    range.start,
                    gap_format(&depths, &palette),
                );

                match tag {
                    Tag::Heading { .. } => depths.heading += 1,
                    Tag::Strong => depths.bold += 1,
                    Tag::Emphasis => depths.italic += 1,
                    Tag::Link { .. } => depths.link += 1,
                    _ => {}
                }
            }

            Event::End(TagEnd::Link) => {
                close_link(&mut job, &mut cursor, source, range.end, &depths, &palette);
                depths.link -= 1;
            }

            Event::End(tag_end) => {
                append_upto(
                    &mut job,
                    &mut cursor,
                    source,
                    range.end,
                    gap_format(&depths, &palette),
                );

                match tag_end {
                    TagEnd::Heading(_) => depths.heading -= 1,
                    TagEnd::Strong => depths.bold -= 1,
                    TagEnd::Emphasis => depths.italic -= 1,
                    _ => {}
                }
            }

            Event::Text(_) => {
                append_upto(
                    &mut job,
                    &mut cursor,
                    source,
                    range.start,
                    gap_format(&depths, &palette),
                );
                append_upto(
                    &mut job,
                    &mut cursor,
                    source,
                    range.end,
                    content_format(&depths, false, &palette),
                );
            }

            Event::Code(code) => {
                // The event's range spans the backtick delimiters too, but its text does not;
                // split the delimiters off evenly so only the inner content gets the code color.
                let delimiters = (range.end - range.start).saturating_sub(code.len());
                let leading = delimiters / 2;

                append_upto(
                    &mut job,
                    &mut cursor,
                    source,
                    range.start + leading,
                    gap_format(&depths, &palette),
                );
                append_upto(
                    &mut job,
                    &mut cursor,
                    source,
                    range.end - (delimiters - leading),
                    content_format(&depths, true, &palette),
                );
                append_upto(
                    &mut job,
                    &mut cursor,
                    source,
                    range.end,
                    gap_format(&depths, &palette),
                );
            }

            _ => {
                append_upto(
                    &mut job,
                    &mut cursor,
                    source,
                    range.end,
                    gap_format(&depths, &palette),
                );
            }
        }
    }

    append_upto(
        &mut job,
        &mut cursor,
        source,
        source.len(),
        gap_format(&depths, &palette),
    );

    job
}

/// Appends `source[cursor..end]` to `job` with `format`, advancing `cursor`. A no-op if `end`
/// does not lie past `cursor`, which happens often since most markdown constructs leave no gap
/// between their start and their first piece of content.
fn append_upto(
    job: &mut LayoutJob,
    cursor: &mut usize,
    source: &str,
    end: usize,
    format: TextFormat,
) {
    if end > *cursor {
        job.append(&source[*cursor..end], 0.0, format);
        *cursor = end;
    }
}

/// The style for markdown syntax characters (`#`, `**`, `` ` ``, `[]()`, ...) and for any other
/// text not carried by an [`Event::Text`] or [`Event::Code`], such as blank lines between blocks.
/// A heading's own `#` marker is bold, matching its content; other markup (`**`, `*`, `[]()`) is
/// left at regular weight, since its dimmed color already sets it apart.
fn gap_format(depths: &Depths, palette: &Palette) -> TextFormat {
    let dimmed = depths.bold > 0 || depths.italic > 0 || depths.link > 0;

    let color = if dimmed {
        palette.punctuation
    } else if depths.heading > 0 {
        palette.heading
    } else {
        palette.default
    };

    let bold = !dimmed && depths.heading > 0;

    TextFormat {
        font_id: fonts::mono(palette.body_size, bold, false),
        color,
        ..Default::default()
    }
}

/// The style for the actual text content of a span, as opposed to its surrounding markup.
fn content_format(depths: &Depths, code: bool, palette: &Palette) -> TextFormat {
    if code {
        return TextFormat {
            font_id: fonts::mono(palette.code_size, false, false),
            color: palette.code,
            ..Default::default()
        };
    }

    let color = if depths.heading > 0 {
        palette.heading
    } else if depths.link > 0 {
        palette.link
    } else if depths.bold > 0 {
        palette.bold
    } else {
        palette.default
    };

    let bold = depths.heading > 0 || depths.bold > 0;
    let italic = depths.italic > 0;

    TextFormat {
        font_id: fonts::mono(palette.body_size, bold, italic),
        color,
        ..Default::default()
    }
}

/// Closes out a link the way VS Code does: the `[link text]` was already colored as it was
/// appended as this link's content, so here only the trailing `](destination)` remains, of
/// which the destination between the parentheses is underlined and the rest left as markup.
fn close_link(
    job: &mut LayoutJob,
    cursor: &mut usize,
    source: &str,
    end: usize,
    depths: &Depths,
    palette: &Palette,
) {
    let remainder = &source[*cursor..end];

    let parens = remainder
        .find('(')
        .zip(remainder.rfind(')'))
        .filter(|(open, close)| open < close);

    let Some((open, close)) = parens else {
        append_upto(job, cursor, source, end, gap_format(depths, palette));
        return;
    };

    let open = *cursor + open;
    let close = *cursor + close;

    append_upto(job, cursor, source, open + 1, gap_format(depths, palette));
    append_upto(job, cursor, source, close, url_format(palette));
    append_upto(job, cursor, source, end, gap_format(depths, palette));
}

/// The style for the destination inside a link's `(...)`, underlined per VS Code's markdown
/// styling of `markup.underline.link`.
fn url_format(palette: &Palette) -> TextFormat {
    TextFormat {
        font_id: fonts::mono(palette.body_size, false, false),
        color: palette.default,
        underline: Stroke::new(1.0_f32, palette.default),
        ..Default::default()
    }
}
