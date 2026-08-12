mod highlight;
mod render;

use pulldown_cmark::{Event, LinkType, Parser, Tag};

pub use highlight::highlight;
pub use render::{render, title};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_links_ignores_underline_markup() {
        let links = extract_links("plain [text](Target) and <u>underlined</u> text");
        assert_eq!(links, vec!["Target".to_owned()]);
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
