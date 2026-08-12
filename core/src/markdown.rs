use pulldown_cmark::{Event, HeadingLevel, LinkType, Parser, Tag, TagEnd};

/// A run of text sharing the same inline styling, the smallest unit a GUI frontend's rendering
/// flows.
#[derive(Clone)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// Set by a `<u>...</u>` pair — CommonMark has no native underline syntax, so this is the
    /// one inline construct recognized as raw HTML rather than markdown proper.
    pub underline: bool,
    pub code: bool,
    pub link: Option<String>,
}

/// A block-level element, holding one or more lines of inline [`Span`]s. A block always starts
/// on its own line; a new line within a block is only forced by an explicit hard break.
pub enum Block {
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

    /// This block's plain text, flattened across every line and span, discarding all styling —
    /// e.g. used to read a heading's title text (see [`title`]) or to detect a paragraph that
    /// starts with "TODO" (see `crate::todo::is_todo`).
    pub fn text(&self) -> String {
        let lines = match self {
            Block::Heading { lines, .. } | Block::Paragraph { lines } => lines,
        };
        lines
            .iter()
            .flatten()
            .map(|span| span.text.as_str())
            .collect()
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

/// The plain text of `source`'s first heading, if it has one — e.g. `# Mira Solenne` gives
/// `Some("Mira Solenne")`. Used to tell whether a note's displayed title still matches its actual
/// (linking) name, since nothing keeps the two in sync automatically: a rename only updates the
/// name, not whatever heading text happens to be written in the note's own source.
pub fn title(source: &str) -> Option<String> {
    collect_blocks(source)
        .into_iter()
        .find_map(|block| match &block {
            Block::Heading { .. } => Some(block.text()),
            Block::Paragraph { .. } => None,
        })
}

/// Rewrites every inline markdown link in `source` whose destination is exactly `old_name` so it
/// points to `new_name` instead, leaving the link's visible label, every other link, and
/// everywhere else `old_name` might appear in plain text completely untouched. Used to keep a
/// note's incoming links pointing at it correctly when it's renamed.
pub fn rename_links(source: &str, old_name: &str, new_name: &str) -> String {
    let mut ranges: Vec<std::ops::Range<usize>> = Parser::new(source)
        .into_offset_iter()
        .filter_map(|(event, span)| match event {
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url,
                ..
            }) if dest_url.as_ref() == old_name => {
                // The event's span covers the whole `[label](destination)`, not just the
                // destination — but since inline links place the destination immediately before
                // the closing `)`, and a matching `dest_url` means it's the last thing in the
                // span that could possibly equal `old_name`, searching from the end finds exactly
                // it (however it's written: bare, or wrapped in `<...>`) without needing to
                // understand that syntax at all.
                let tag_text = &source[span.clone()];
                tag_text.rfind(old_name).map(|relative| {
                    let start = span.start + relative;
                    start..start + old_name.len()
                })
            }
            _ => None,
        })
        .collect();

    ranges.sort_by_key(|range| range.start);

    let mut result = source.to_owned();
    for range in ranges.into_iter().rev() {
        result.replace_range(range, new_name);
    }
    result
}

/// Parses `source` into a flat sequence of blocks, resolving inline styling and links onto
/// [`Span`]s so that a GUI frontend can render a block's content as flowing text instead of one
/// widget per markdown event.
pub fn collect_blocks(source: &str) -> Vec<Block> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_links_ignores_underline_markup() {
        let links = extract_links("plain [text](Target) and <u>underlined</u> text");
        assert_eq!(links, vec!["Target".to_owned()]);
    }

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

    #[test]
    fn rename_links_retargets_a_bare_destination_without_touching_the_label() {
        let renamed = rename_links("see [Bob](Bob) over there", "Bob", "Robert");
        assert_eq!(renamed, "see [Bob](Robert) over there");
    }

    #[test]
    fn rename_links_retargets_an_angle_bracketed_destination() {
        let renamed = rename_links(
            "her brother [Dorin Ashe](<Dorin Ashe>) escaped",
            "Dorin Ashe",
            "Dorin Vasse",
        );
        assert_eq!(renamed, "her brother [Dorin Ashe](<Dorin Vasse>) escaped");
    }

    #[test]
    fn rename_links_updates_every_matching_link_and_ignores_others() {
        let renamed = rename_links(
            "[Bob](Bob) and [Bob again](Bob) but not [Alice](Alice)",
            "Bob",
            "Robert",
        );
        assert_eq!(
            renamed,
            "[Bob](Robert) and [Bob again](Robert) but not [Alice](Alice)"
        );
    }

    #[test]
    fn rename_links_leaves_plain_text_mentions_of_the_name_alone() {
        let renamed = rename_links("Bob said hello. [Bob](Bob) waved back.", "Bob", "Robert");
        assert_eq!(renamed, "Bob said hello. [Bob](Robert) waved back.");
    }
}
