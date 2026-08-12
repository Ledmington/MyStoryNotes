use eframe::egui;

use my_story_notes_core::inline_format::{InlineFormat, apply_inline_format};
use my_story_notes_core::settings::EditPalette;

use crate::markdown;

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
fn consume_pressed(ui: &egui::Ui) -> Option<InlineFormat> {
    ui.input_mut(|input| {
        if input.consume_shortcut(&BOLD_SHORTCUT) {
            Some(InlineFormat::Bold)
        } else if input.consume_shortcut(&ITALIC_SHORTCUT) {
            Some(InlineFormat::Italic)
        } else if input.consume_shortcut(&UNDERLINE_SHORTCUT) {
            Some(InlineFormat::Underline)
        } else if input.consume_shortcut(&VERBATIM_SHORTCUT) {
            Some(InlineFormat::Verbatim)
        } else if input.consume_shortcut(&HYPERLINK_SHORTCUT) {
            Some(InlineFormat::Hyperlink)
        } else {
            None
        }
    })
}

/// Switches back to render mode without inserting a newline. Consumed before `TextEdit::show()`
/// for the same reason as the format shortcuts above — egui's multiline `TextEdit` otherwise
/// treats a plain Enter as "insert newline", and Ctrl+Enter would insert one too if the widget saw
/// it first. `pub(crate)` so [`crate::app`] can show it as a hover hint on the "Done" button.
pub(crate) const SWITCH_TO_RENDER_SHORTCUT: egui::KeyboardShortcut =
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
        && let Some(format) = consume_pressed(ui)
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
