use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Stroke, Ui};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::fonts;
use crate::settings::{self, EditPalette};

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
    underline: u32,
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

            Event::InlineHtml(html) => match html.trim() {
                "<u>" => {
                    append_upto(
                        &mut job,
                        &mut cursor,
                        source,
                        range.start,
                        gap_format(&depths, &palette),
                    );
                    depths.underline += 1;
                }
                "</u>" => {
                    append_upto(
                        &mut job,
                        &mut cursor,
                        source,
                        range.end,
                        gap_format(&depths, &palette),
                    );
                    depths.underline = depths.underline.saturating_sub(1);
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
            },

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
    let dimmed = depths.bold > 0 || depths.italic > 0 || depths.underline > 0 || depths.link > 0;

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
    let underline = if depths.underline > 0 {
        Stroke::new(1.0, color)
    } else {
        Stroke::NONE
    };

    TextFormat {
        font_id: fonts::mono(palette.body_size, bold, italic),
        color,
        underline,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_underlines_only_the_content_between_u_tags() {
        egui::__run_test_ui(|ui| {
            let job = highlight(
                ui,
                "before <u>under</u> after",
                &EditPalette::default(),
                14.0,
            );

            let format_of = |needle: &str| {
                job.sections
                    .iter()
                    .find(|section| {
                        let range = section.byte_range.start.0..section.byte_range.end.0;
                        job.text[range] == *needle
                    })
                    .unwrap_or_else(|| panic!("no section for {needle:?}"))
                    .format
                    .clone()
            };

            assert_ne!(format_of("under").underline, Stroke::NONE);
            assert_eq!(format_of("before ").underline, Stroke::NONE);
            assert_eq!(format_of(" after").underline, Stroke::NONE);
        });
    }
}
