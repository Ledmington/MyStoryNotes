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
    List(List),
}

impl Block {
    /// This block's plain text, flattened across every line, span, and (for a list) nested item
    /// and sublist, discarding all styling — e.g. used to read a heading's title text (see
    /// [`title`]).
    pub fn text(&self) -> String {
        match self {
            Block::Heading { lines, .. } | Block::Paragraph { lines } => lines_text(lines),
            Block::List(list) => list.text(),
        }
    }
}

/// A bullet or numbered list, holding its top-level [`ListItem`]s — nested sublists live inside
/// their parent item (see [`ListItem::sublists`]) rather than as their own top-level [`Block`].
pub struct List {
    /// `None` for a bullet list; `Some(n)` for a numbered list starting at `n` — CommonMark
    /// allows a numbered list to start at any number, e.g. `5. item`.
    pub ordered_start: Option<u64>,
    pub items: Vec<ListItem>,
}

impl List {
    fn text(&self) -> String {
        self.items.iter().map(ListItem::text).collect()
    }
}

/// One entry of a [`List`]: its own content, plus any sublist(s) nested directly inside it.
pub struct ListItem {
    pub lines: Vec<Vec<Span>>,
    pub sublists: Vec<List>,
}

impl ListItem {
    fn text(&self) -> String {
        let mut text = lines_text(&self.lines);
        for sublist in &self.sublists {
            text.push_str(&sublist.text());
        }
        text
    }
}

fn lines_text(lines: &[Vec<Span>]) -> String {
    lines
        .iter()
        .flatten()
        .map(|span| span.text.as_str())
        .collect()
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
            Block::Paragraph { .. } | Block::List(_) => None,
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

/// Which inline styling is currently open while [`collect_inline_lines`] or [`collect_item`]
/// walks a run of inline events — mirrors the nesting of `<strong>`/`<em>`/link/`<u>` tags in the
/// event stream, since they're always properly balanced within the block that opened them.
#[derive(Default)]
struct InlineState {
    bold: bool,
    italic: bool,
    underline: bool,
    link: Option<String>,
}

/// Parses `source` into a flat sequence of top-level blocks, resolving inline styling, links, and
/// (nested) list structure onto [`Span`]s and [`List`]/[`ListItem`]s so that a GUI frontend can
/// render each block as flowing text and indented items instead of one widget per markdown event.
pub fn collect_blocks(source: &str) -> Vec<Block> {
    let mut parser = Parser::new(source);
    let mut blocks = Vec::new();

    while let Some(event) = parser.next() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let lines = collect_inline_lines(&mut parser, is_end_heading);
                blocks.push(Block::Heading { level, lines });
            }

            Event::Start(Tag::Paragraph) => {
                let lines = collect_inline_lines(&mut parser, is_end_paragraph);
                blocks.push(Block::Paragraph { lines });
            }

            Event::Start(Tag::List(ordered_start)) => {
                blocks.push(Block::List(collect_list(&mut parser, ordered_start)));
            }

            _ => {}
        }
    }

    blocks
}

fn is_end_heading(event: &Event) -> bool {
    matches!(event, Event::End(TagEnd::Heading(_)))
}

fn is_end_paragraph(event: &Event) -> bool {
    matches!(event, Event::End(TagEnd::Paragraph))
}

/// Consumes a list's items up to (and including) its closing [`TagEnd::List`], having already
/// consumed the opening [`Tag::List`].
fn collect_list(parser: &mut Parser, ordered_start: Option<u64>) -> List {
    let mut items = Vec::new();

    while let Some(event) = parser.next() {
        match event {
            Event::Start(Tag::Item) => items.push(collect_item(parser)),
            Event::End(TagEnd::List(_)) => break,
            _ => {}
        }
    }

    List {
        ordered_start,
        items,
    }
}

/// Consumes one list item's content up to its closing [`TagEnd::Item`], having already consumed
/// the opening [`Tag::Item`]. A *tight* list (no blank lines between items) puts the item's
/// inline content directly under `Item`; a *loose* one wraps it in its own nested `Paragraph` —
/// either way it ends up in [`ListItem::lines`]. A nested `List` (sub-bullets) becomes one of
/// [`ListItem::sublists`] instead.
fn collect_item(parser: &mut Parser) -> ListItem {
    let mut lines: Vec<Vec<Span>> = vec![Vec::new()];
    let mut sublists = Vec::new();
    let mut inline = InlineState::default();

    while let Some(event) = parser.next() {
        match event {
            Event::End(TagEnd::Item) => break,

            Event::Start(Tag::Paragraph) => {
                let more = collect_inline_lines(parser, is_end_paragraph);
                if lines.iter().all(Vec::is_empty) {
                    lines = more;
                } else {
                    lines.extend(more);
                }
            }

            Event::Start(Tag::List(ordered_start)) => {
                sublists.push(collect_list(parser, ordered_start));
            }

            other => apply_inline_event(other, &mut lines, &mut inline),
        }
    }

    ListItem { lines, sublists }
}

/// Consumes inline events into lines of [`Span`]s until `is_end` matches, having already consumed
/// the block's opening tag. Shared by headings, paragraphs, and (via [`collect_item`]) list items.
fn collect_inline_lines(parser: &mut Parser, is_end: fn(&Event) -> bool) -> Vec<Vec<Span>> {
    let mut lines = vec![Vec::new()];
    let mut inline = InlineState::default();

    for event in parser.by_ref() {
        if is_end(&event) {
            break;
        }
        apply_inline_event(event, &mut lines, &mut inline);
    }

    lines
}

/// Applies one inline-level event (styling markers, text, code, breaks) to `lines`/`inline`.
fn apply_inline_event(event: Event, lines: &mut Vec<Vec<Span>>, inline: &mut InlineState) {
    match event {
        Event::Start(Tag::Strong) => inline.bold = true,
        Event::End(TagEnd::Strong) => inline.bold = false,
        Event::Start(Tag::Emphasis) => inline.italic = true,
        Event::End(TagEnd::Emphasis) => inline.italic = false,
        Event::Start(Tag::Link { dest_url, .. }) => inline.link = Some(dest_url.to_string()),
        Event::End(TagEnd::Link) => inline.link = None,

        Event::InlineHtml(html) => match html.trim() {
            "<u>" => inline.underline = true,
            "</u>" => inline.underline = false,
            _ => {}
        },

        Event::Text(text) => lines.last_mut().unwrap().push(Span {
            text: text.to_string(),
            bold: inline.bold,
            italic: inline.italic,
            underline: inline.underline,
            code: false,
            link: inline.link.clone(),
        }),

        Event::Code(code) => lines.last_mut().unwrap().push(Span {
            text: code.to_string(),
            bold: inline.bold,
            italic: inline.italic,
            underline: inline.underline,
            code: true,
            link: inline.link.clone(),
        }),

        Event::SoftBreak => lines.last_mut().unwrap().push(Span {
            text: " ".to_owned(),
            bold: false,
            italic: false,
            underline: false,
            code: false,
            link: None,
        }),

        Event::HardBreak => lines.push(Vec::new()),

        _ => {}
    }
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

    fn sole_list(source: &str) -> List {
        let mut blocks = collect_blocks(source);
        assert_eq!(blocks.len(), 1, "expected exactly one block");
        match blocks.remove(0) {
            Block::List(list) => list,
            _ => panic!("expected a list block"),
        }
    }

    #[test]
    fn collect_blocks_parses_a_bullet_list() {
        let list = sole_list("- a\n- b\n- c");

        assert_eq!(list.ordered_start, None);
        let texts: Vec<String> = list
            .items
            .iter()
            .map(|item| lines_text(&item.lines))
            .collect();
        assert_eq!(texts, vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    }

    #[test]
    fn collect_blocks_parses_a_numbered_list_honoring_its_start_number() {
        let list = sole_list("5. a\n6. b");

        assert_eq!(list.ordered_start, Some(5));
        assert_eq!(list.items.len(), 2);
        assert_eq!(lines_text(&list.items[1].lines), "b");
    }

    #[test]
    fn collect_blocks_nests_a_sublist_inside_its_parent_item() {
        let list = sole_list("- a\n  - nested\n- b");

        assert_eq!(list.items.len(), 2);
        assert_eq!(lines_text(&list.items[0].lines), "a");

        assert_eq!(list.items[0].sublists.len(), 1);
        let sublist = &list.items[0].sublists[0];
        assert_eq!(sublist.ordered_start, None);
        assert_eq!(sublist.items.len(), 1);
        assert_eq!(lines_text(&sublist.items[0].lines), "nested");

        assert!(list.items[1].sublists.is_empty());
    }

    #[test]
    fn collect_blocks_parses_a_loose_list_the_same_as_a_tight_one() {
        let list = sole_list("- a\n\n- b");

        assert_eq!(list.items.len(), 2);
        assert_eq!(lines_text(&list.items[0].lines), "a");
        assert_eq!(lines_text(&list.items[1].lines), "b");
    }

    #[test]
    fn title_ignores_a_list_and_still_finds_the_heading() {
        assert_eq!(title("- a\n- b\n\n# Heading"), Some("Heading".to_owned()));
    }
}
