/// The five inline markdown constructs a selection can be wrapped in from the note editor.
/// Underline has no native CommonMark syntax, so it's the one represented with raw HTML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineFormat {
    Bold,
    Italic,
    Underline,
    Verbatim,
    Hyperlink,
}

impl InlineFormat {
    /// The markup placed before and after the wrapped text.
    pub fn markers(self) -> (&'static str, &'static str) {
        match self {
            Self::Bold => ("**", "**"),
            Self::Italic => ("*", "*"),
            Self::Underline => ("<u>", "</u>"),
            Self::Verbatim => ("`", "`"),
            Self::Hyperlink => ("[", "]()"),
        }
    }
}

/// Toggles `format`'s markup around `source[selection]` (a *character*, not byte, range): if the
/// selection is already immediately wrapped in `format`'s markers, they're stripped; otherwise
/// they're added, or, if `selection` is empty, inserted as an empty pair at the cursor. Returns
/// the new source and the character range the caller should re-select afterward — the wrapped or
/// unwrapped text, or, for an empty selection, the point between the two markers (so hyperlink
/// lands the cursor inside `[]`, ready to type the link text).
pub fn apply_inline_format(
    source: &str,
    selection: std::ops::Range<usize>,
    format: InlineFormat,
) -> (String, std::ops::Range<usize>) {
    let (prefix, suffix) = format.markers();

    let start = char_to_byte_index(source, selection.start);
    let end = char_to_byte_index(source, selection.end);

    if marker_ends_at(source, start, prefix) && marker_starts_at(source, end, suffix) {
        let unwrap_start = start - prefix.len();
        let unwrap_end = end + suffix.len();

        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..unwrap_start]);
        new_source.push_str(&source[start..end]);
        new_source.push_str(&source[unwrap_end..]);

        let prefix_len = prefix.chars().count();
        let new_selection = (selection.start - prefix_len)..(selection.end - prefix_len);

        (new_source, new_selection)
    } else {
        let mut new_source = String::with_capacity(source.len() + prefix.len() + suffix.len());
        new_source.push_str(&source[..start]);
        new_source.push_str(prefix);
        new_source.push_str(&source[start..end]);
        new_source.push_str(suffix);
        new_source.push_str(&source[end..]);

        let prefix_len = prefix.chars().count();
        let new_selection = (selection.start + prefix_len)..(selection.end + prefix_len);

        (new_source, new_selection)
    }
}

/// Whether `marker` sits immediately before byte offset `boundary` in `source` — and, when
/// `marker` is a run of one repeated character (as Bold's `**` and Italic's `*` both are), isn't
/// merely the tail of a *longer* run of that character. Without that check, reselecting bold text
/// and pressing Italic would see Bold's `**` and mistake it for two copies of Italic's `*`.
fn marker_ends_at(source: &str, boundary: usize, marker: &str) -> bool {
    if !source[..boundary].ends_with(marker) {
        return false;
    }

    match repeated_char(marker) {
        Some(c) => !source[..boundary - marker.len()].ends_with(c),
        None => true,
    }
}

/// The mirror of [`marker_ends_at`]: whether `marker` sits immediately after byte offset
/// `boundary`, and isn't the head of a longer run of the same repeated character.
fn marker_starts_at(source: &str, boundary: usize, marker: &str) -> bool {
    if !source[boundary..].starts_with(marker) {
        return false;
    }

    match repeated_char(marker) {
        Some(c) => !source[boundary + marker.len()..].starts_with(c),
        None => true,
    }
}

/// If `s` is one or more repetitions of a single character, that character.
fn repeated_char(s: &str) -> Option<char> {
    let mut chars = s.chars();
    let first = chars.next()?;
    chars.all(|c| c == first).then_some(first)
}

/// The byte offset of the `char_index`-th character in `s`, or `s.len()` if `char_index` is at or
/// past the end — `str` indexing needs byte offsets, but a text cursor typically counts
/// characters.
pub fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map_or(s.len(), |(byte_index, _)| byte_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_a_selection_in_markers_and_reselects_the_wrapped_text() {
        let (source, selection) = apply_inline_format("hello world", 6..11, InlineFormat::Bold);
        assert_eq!(source, "hello **world**");
        assert_eq!(selection, 8..13);
        assert_eq!(
            &source[char_to_byte_index(&source, selection.start)..],
            "world**"
        );
    }

    #[test]
    fn empty_selection_inserts_an_empty_pair_and_lands_the_cursor_between_them() {
        let (source, selection) = apply_inline_format("hello ", 6..6, InlineFormat::Italic);
        assert_eq!(source, "hello **");
        assert_eq!(selection, 7..7);
    }

    #[test]
    fn hyperlink_lands_the_cursor_inside_the_brackets_when_nothing_is_selected() {
        let (source, selection) = apply_inline_format("see also ", 9..9, InlineFormat::Hyperlink);
        assert_eq!(source, "see also []()");
        assert_eq!(selection, 10..10);
    }

    #[test]
    fn underline_wraps_with_html_since_commonmark_has_no_native_syntax() {
        let (source, selection) = apply_inline_format("plain text", 0..5, InlineFormat::Underline);
        assert_eq!(source, "<u>plain</u> text");
        assert_eq!(selection, 3..8);
    }

    #[test]
    fn char_to_byte_index_accounts_for_multi_byte_characters() {
        let s = "héllo";
        assert_eq!(char_to_byte_index(s, 0), 0);
        assert_eq!(char_to_byte_index(s, 1), 1);
        // 'é' is 2 bytes, so the 3rd character ('l') starts at byte 3, not 2.
        assert_eq!(char_to_byte_index(s, 2), 3);
        assert_eq!(char_to_byte_index(s, 100), s.len());
    }

    /// Regression test: applying the same format to an already-wrapped selection used to wrap it
    /// a second time ("**hello**" -> "****hello****") instead of removing the markup.
    #[test]
    fn pressing_the_same_format_twice_toggles_it_back_off() {
        let (wrapped, selection) = apply_inline_format("hello world", 6..11, InlineFormat::Bold);
        assert_eq!(wrapped, "hello **world**");

        let (unwrapped, selection) = apply_inline_format(&wrapped, selection, InlineFormat::Bold);
        assert_eq!(unwrapped, "hello world");
        assert_eq!(selection, 6..11);
    }

    #[test]
    fn toggling_off_verbatim_and_underline_also_round_trips() {
        let (wrapped, selection) =
            apply_inline_format("hello world", 6..11, InlineFormat::Verbatim);
        let (unwrapped, selection) =
            apply_inline_format(&wrapped, selection, InlineFormat::Verbatim);
        assert_eq!(unwrapped, "hello world");
        assert_eq!(selection, 6..11);

        let (wrapped, selection) =
            apply_inline_format("hello world", 6..11, InlineFormat::Underline);
        let (unwrapped, selection) =
            apply_inline_format(&wrapped, selection, InlineFormat::Underline);
        assert_eq!(unwrapped, "hello world");
        assert_eq!(selection, 6..11);
    }

    #[test]
    fn toggling_a_format_on_then_off_then_on_again_round_trips_cleanly() {
        let (once, selection) = apply_inline_format("hello world", 6..11, InlineFormat::Bold);
        let (twice, selection) = apply_inline_format(&once, selection, InlineFormat::Bold);
        let (thrice, selection) = apply_inline_format(&twice, selection, InlineFormat::Bold);
        assert_eq!(thrice, "hello **world**");
        assert_eq!(selection, 8..13);
    }

    /// Regression test: Bold's marker (`**`) is a run of the same character as Italic's (`*`), so
    /// naively checking "does the marker sit right outside the selection" would see Bold's `**`
    /// and mistake it for two copies of Italic's `*`, corrupting the bold markup instead of
    /// nesting italic inside it.
    #[test]
    fn a_different_format_nests_around_bold_text_instead_of_misreading_its_markers() {
        let (bold, selection) = apply_inline_format("hello world", 6..11, InlineFormat::Bold);
        assert_eq!(bold, "hello **world**");

        let (nested, selection) = apply_inline_format(&bold, selection, InlineFormat::Italic);
        assert_eq!(nested, "hello ***world***");
        assert_eq!(selection, 9..14);
    }
}
