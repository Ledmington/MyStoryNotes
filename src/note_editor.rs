use eframe::egui;

use crate::{markdown, settings::EditPalette};

/// The five inline markdown constructs a selection can be wrapped in from the note editor.
/// Underline has no native CommonMark syntax, so it's the one represented with raw HTML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineFormat {
    Bold,
    Italic,
    Underline,
    Verbatim,
    Hyperlink,
}

impl InlineFormat {
    // egui's `TextEdit` hard-codes Ctrl+H/K/U/W to delete text (previous char, to-end-of-line,
    // to-start-of-line, previous word — see `check_for_mutating_key_press` in egui's
    // `text_edit/builder.rs`) on every platform where `Modifiers::COMMAND` is Ctrl (i.e. not
    // macOS). Since Underline and Hyperlink use two of those letters, they can only be handled by
    // consuming the keypress *before* `TextEdit::show()` ever sees it — see the comment on
    // [`draw_note_editor`] for how that's arranged.
    const BOLD_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::B);
    const ITALIC_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::I);
    const UNDERLINE_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::U);
    const VERBATIM_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::E);
    const HYPERLINK_SHORTCUT: egui::KeyboardShortcut =
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::K);

    /// The shortcut key press that was consumed this frame, if any, checked in priority order
    /// (irrelevant here since the five shortcuts share no keys, but kept explicit for clarity).
    fn consume_pressed(ui: &egui::Ui) -> Option<Self> {
        ui.input_mut(|input| {
            if input.consume_shortcut(&Self::BOLD_SHORTCUT) {
                Some(Self::Bold)
            } else if input.consume_shortcut(&Self::ITALIC_SHORTCUT) {
                Some(Self::Italic)
            } else if input.consume_shortcut(&Self::UNDERLINE_SHORTCUT) {
                Some(Self::Underline)
            } else if input.consume_shortcut(&Self::VERBATIM_SHORTCUT) {
                Some(Self::Verbatim)
            } else if input.consume_shortcut(&Self::HYPERLINK_SHORTCUT) {
                Some(Self::Hyperlink)
            } else {
                None
            }
        })
    }

    /// The markup placed before and after the wrapped text.
    fn markers(self) -> (&'static str, &'static str) {
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
fn apply_inline_format(
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
/// past the end — `str` indexing needs byte offsets, but egui's text cursors count characters.
fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map_or(s.len(), |(byte_index, _)| byte_index)
}

/// Switches back to render mode without inserting a newline. Consumed before `TextEdit::show()`
/// for the same reason as the format shortcuts above — egui's multiline `TextEdit` otherwise
/// treats a plain Enter as "insert newline", and Ctrl+Enter would insert one too if the widget saw
/// it first.
const SWITCH_TO_RENDER_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter);

/// Draws the raw-source editor for a note's `source` at the given persistent `id`.
///
/// Applies any pending `InlineFormat` shortcut *before* handing input to [`egui::TextEdit`],
/// rather than after (as would be the natural order, since we need the widget's own cursor state
/// to know the selection). `TextEdit` hard-codes Ctrl+K/Ctrl+U to delete text — the same keys used
/// here for Hyperlink and Underline — so if the widget saw the keypress first, it would delete the
/// selection *in addition to* whatever we did with it. Consuming the shortcut first removes the
/// event from the input queue, so by the time `TextEdit::show()` runs, the key press is already
/// gone and its built-in binding never fires. Returns whether the widget lost focus this frame, or
/// Ctrl+Enter was pressed — either way, the caller should switch back to render mode.
pub fn draw_note_editor(
    ui: &mut egui::Ui,
    source: &mut String,
    id: egui::Id,
    edit: &EditPalette,
    edit_size: f32,
) -> bool {
    let has_focus = ui.memory(|memory| memory.has_focus(id));

    if has_focus
        && let Some(format) = InlineFormat::consume_pressed(ui)
        && let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ui.ctx(), id)
        && let Some(cursor_range) = state.cursor.char_range()
    {
        let selection = cursor_range.as_sorted_char_range();
        let (new_source, new_selection) = apply_inline_format(
            source,
            usize::from(selection.start)..usize::from(selection.end),
            format,
        );
        *source = new_source;

        let new_range = egui::text::CCursorRange::two(
            egui::text::CCursor::new(new_selection.start),
            egui::text::CCursor::new(new_selection.end),
        );
        state.cursor.set_char_range(Some(new_range));
        state.store(ui.ctx(), id);
    }

    let switch_to_render =
        has_focus && ui.input_mut(|input| input.consume_shortcut(&SWITCH_TO_RENDER_SHORTCUT));

    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
        let mut layout_job = markdown::highlight(ui, buf.as_str(), edit, edit_size);
        layout_job.wrap.max_width = wrap_width;
        ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
    };

    let output = egui::TextEdit::multiline(source)
        .id(id)
        .desired_width(f32::INFINITY)
        .desired_rows(15)
        .layouter(&mut layouter)
        .show(ui);

    output.response.lost_focus() || switch_to_render
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

    /// Drives `draw_note_editor` through a real [`egui::Context`] across two frames: the first
    /// establishes focus and a text selection (standing in for the user clicking in and
    /// dragging), the second delivers `key` held with Ctrl (as a real Linux/Windows keypress
    /// would set both `ctrl` and `command`) and returns the resulting note source.
    ///
    /// This exists to catch a real regression: egui's `TextEdit` hard-codes Ctrl+K/Ctrl+U to
    /// delete text (see the comment on `draw_note_editor`), the same keys used here for
    /// Hyperlink and Underline. A plain unit test of `apply_inline_format` can't see that
    /// conflict — it only shows up once a real `TextEdit` widget processes the keypress — so this
    /// drives the actual widget instead.
    fn press_ctrl_key_in_note_editor(
        source: &str,
        selection: std::ops::Range<usize>,
        key: egui::Key,
    ) -> String {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx); // markdown::highlight needs the app's named font families
        let id = egui::Id::new("test_note_editor");
        let mut source = source.to_owned();

        // Frame 1: draw the editor once and grab focus, then set the selection the user is
        // assumed to have made — there's no synthetic mouse-drag to select text with here.
        run_note_editor_frame(&ctx, &mut source, id, 0.0, None);
        set_note_editor_selection(&ctx, id, selection);

        // Frame 2: deliver the keypress.
        run_note_editor_frame(&ctx, &mut source, id, 0.0, Some(ctrl_key_event(key, false)));

        source
    }

    #[test]
    fn ctrl_u_underlines_the_selection_instead_of_deleting_it() {
        let source = press_ctrl_key_in_note_editor("hello world", 6..11, egui::Key::U);
        assert_eq!(source, "hello <u>world</u>");
    }

    #[test]
    fn ctrl_enter_reports_done_without_inserting_a_newline() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let id = egui::Id::new("test_ctrl_enter_note_editor");
        let mut source = "hello world".to_owned();

        run_note_editor_frame(&ctx, &mut source, id, 0.0, None);
        let done = run_note_editor_frame(
            &ctx,
            &mut source,
            id,
            0.0,
            Some(ctrl_key_event(egui::Key::Enter, false)),
        );

        assert!(done, "Ctrl+Enter should report the editor as done");
        assert_eq!(source, "hello world");
    }

    #[test]
    fn plain_enter_inserts_a_newline_instead_of_reporting_done() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let id = egui::Id::new("test_plain_enter_note_editor");
        let mut source = "hello world".to_owned();

        run_note_editor_frame(&ctx, &mut source, id, 0.0, None);
        set_note_editor_selection(&ctx, id, 11..11);
        let done = run_note_editor_frame(
            &ctx,
            &mut source,
            id,
            0.0,
            Some(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            }),
        );

        assert!(!done, "plain Enter should not report the editor as done");
        assert_eq!(source, "hello world\n");
    }

    #[test]
    fn ctrl_k_turns_the_selection_into_a_hyperlink_instead_of_deleting_the_rest_of_the_line() {
        let source = press_ctrl_key_in_note_editor("see also world", 9..14, egui::Key::K);
        assert_eq!(source, "see also [world]()");
    }

    /// egui's `TextEdit` has its own built-in undo/redo (Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y), keyed
    /// off whatever text and cursor it sees when it draws — which, since `draw_note_editor`
    /// mutates `source` (for our format shortcuts) *before* the widget ever runs, it picks up
    /// automatically. These tests exist to confirm that actually holds, not to implement undo
    /// ourselves: an applied format is just another edit as far as the widget's undo history is
    /// concerned. `Undoer` only turns a change into its own undo point once the state has stayed
    /// put for one second (see `egui::util::undoer::Settings::stable_time`), so these frames
    /// advance simulated time rather than wall-clock time to make that deterministic.
    #[test]
    fn ctrl_z_undoes_an_applied_format_once_it_settles() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let id = egui::Id::new("test_undo_note_editor");
        let mut source = "hello world".to_owned();

        run_note_editor_frame(&ctx, &mut source, id, 0.0, None); // first frame: seeds undo point 1
        set_note_editor_selection(&ctx, id, 6..11);

        run_note_editor_frame(
            &ctx,
            &mut source,
            id,
            0.0,
            Some(ctrl_key_event(egui::Key::B, false)),
        );
        assert_eq!(source, "hello **world**");

        run_note_editor_frame(&ctx, &mut source, id, 2.0, None); // let the format settle
        run_note_editor_frame(
            &ctx,
            &mut source,
            id,
            2.0,
            Some(ctrl_key_event(egui::Key::Z, false)),
        );

        assert_eq!(source, "hello world");
    }

    #[test]
    fn ctrl_shift_z_redoes_after_an_undo() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let id = egui::Id::new("test_redo_note_editor");
        let mut source = "hello world".to_owned();

        run_note_editor_frame(&ctx, &mut source, id, 0.0, None);
        set_note_editor_selection(&ctx, id, 6..11);
        run_note_editor_frame(
            &ctx,
            &mut source,
            id,
            0.0,
            Some(ctrl_key_event(egui::Key::B, false)),
        );
        run_note_editor_frame(&ctx, &mut source, id, 2.0, None);
        run_note_editor_frame(
            &ctx,
            &mut source,
            id,
            2.0,
            Some(ctrl_key_event(egui::Key::Z, false)),
        );
        assert_eq!(source, "hello world");

        run_note_editor_frame(
            &ctx,
            &mut source,
            id,
            2.0,
            Some(ctrl_key_event(egui::Key::Z, true)),
        );
        assert_eq!(source, "hello **world**");
    }

    /// A Ctrl(+Shift)+`key` event, matching what a real Linux/Windows keypress reports (both
    /// `ctrl` and `command` set — see the doc comment on [`press_ctrl_key_in_note_editor`]).
    fn ctrl_key_event(key: egui::Key, shift: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                command: true,
                shift,
                ..Default::default()
            },
        }
    }

    /// Runs one frame of [`draw_note_editor`] against a persistent, focused widget `id`, at
    /// simulated `time` (seconds), optionally delivering `event`. Returns whatever
    /// `draw_note_editor` returned for that frame.
    fn run_note_editor_frame(
        ctx: &egui::Context,
        source: &mut String,
        id: egui::Id,
        time: f64,
        event: Option<egui::Event>,
    ) -> bool {
        let raw_input = egui::RawInput {
            time: Some(time),
            events: event.into_iter().collect(),
            ..Default::default()
        };

        let mut done = false;

        ctx.run_ui(raw_input, |ui| {
            ui.memory_mut(|memory| memory.request_focus(id));
            done = draw_note_editor(ui, source, id, &EditPalette::default(), 14.0);
        })
        .drop_without_applying_deltas();

        done
    }

    /// Directly overwrites the persisted selection for `id`, standing in for a mouse drag.
    fn set_note_editor_selection(
        ctx: &egui::Context,
        id: egui::Id,
        selection: std::ops::Range<usize>,
    ) {
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(selection.start),
                egui::text::CCursor::new(selection.end),
            )));
        state.store(ctx, id);
    }
}
