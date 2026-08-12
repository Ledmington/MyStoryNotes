use eframe::egui;

use my_story_notes_core::graph;
use my_story_notes_core::project::{NoteId, Project};

/// How the note list sidebar orders its notes. Clicking a sort button cycles its two variants
/// (ascending, then descending) before landing back on [`Self::Unsorted`] — see
/// [`NoteSort::cycle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoteSort {
    /// The order notes are stored in the project file, unchanged.
    Unsorted,
    NameAscending,
    NameDescending,
    ConnectionsAscending,
    ConnectionsDescending,
}

impl NoteSort {
    /// The state after clicking a sort button whose two directions are `ascending`/`descending`:
    /// `ascending` if neither is currently active, `descending` if `ascending` is active, or
    /// [`Self::Unsorted`] if `descending` is active — so repeated clicks cycle ascending →
    /// descending → unsorted → ascending → ...
    pub(super) fn cycle(self, ascending: Self, descending: Self) -> Self {
        if self == ascending {
            descending
        } else if self == descending {
            Self::Unsorted
        } else {
            ascending
        }
    }
}

/// The indices into `project.notes`, ordered per `sort` — the save file's own order for
/// [`NoteSort::Unsorted`]. Ties within the two connections orderings break alphabetically, for a
/// stable, predictable order.
pub(super) fn sorted_note_indices(project: &Project, sort: NoteSort) -> Vec<NoteId> {
    let mut order: Vec<NoteId> = (0..project.notes.len()).map(NoteId::from).collect();

    match sort {
        NoteSort::Unsorted => {}
        NoteSort::NameAscending => {
            order.sort_by(|&a, &b| {
                project.notes[usize::from(a)]
                    .name
                    .cmp(&project.notes[usize::from(b)].name)
            });
        }
        NoteSort::NameDescending => {
            order.sort_by(|&a, &b| {
                project.notes[usize::from(b)]
                    .name
                    .cmp(&project.notes[usize::from(a)].name)
            });
        }
        NoteSort::ConnectionsAscending | NoteSort::ConnectionsDescending => {
            let counts = graph::connection_counts(project);
            let ascending = sort == NoteSort::ConnectionsAscending;

            order.sort_by(|&a, &b| {
                let by_count = if ascending {
                    counts[usize::from(a)].cmp(&counts[usize::from(b)])
                } else {
                    counts[usize::from(b)].cmp(&counts[usize::from(a)])
                };
                by_count.then_with(|| {
                    project.notes[usize::from(a)]
                        .name
                        .cmp(&project.notes[usize::from(b)].name)
                })
            });
        }
    }

    order
}

/// A sort-toggle button: `label` plus an up/down arrow when `current` is `ascending` or
/// `descending`, highlighted while either is active. Returns whether it was clicked this frame.
pub(super) fn sort_button(
    ui: &mut egui::Ui,
    label: &str,
    current: NoteSort,
    ascending: NoteSort,
    descending: NoteSort,
) -> bool {
    let text = if current == ascending {
        crate::fonts::icon_label(ui, crate::fonts::icon::ARROW_UP, label)
    } else if current == descending {
        crate::fonts::icon_label(ui, crate::fonts::icon::ARROW_DOWN, label)
    } else {
        label.into()
    };

    let active = current == ascending || current == descending;
    ui.selectable_label(active, text).clicked()
}
