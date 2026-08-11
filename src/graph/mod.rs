mod layout;
mod simulation;
mod view;

use crate::markdown;
use crate::project::{NoteId, Project};

pub use simulation::{Simulation, settle};
pub use view::{NoteHighlight, View, draw};

/// An edge's position in the resolved edge list, for tracking which one (if any) the mouse is
/// hovering. A thin `usize` wrapper so it can't be confused with a [`NoteId`], even though both
/// are array indices under the hood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConnectionId(usize);

impl From<usize> for ConnectionId {
    fn from(index: usize) -> Self {
        Self(index)
    }
}

impl From<ConnectionId> for usize {
    fn from(id: ConnectionId) -> Self {
        id.0
    }
}

/// A markdown link from one note to another, resolved to indices into [`Project::notes`].
struct Edge {
    from: NoteId,
    to: NoteId,
}

/// Resolves every note's markdown links into [`Edge`]s indexing into `project.notes`.
fn resolve_edges(project: &Project) -> Vec<Edge> {
    project
        .notes
        .iter()
        .enumerate()
        .flat_map(|(from, note)| {
            let from = NoteId::from(from);

            markdown::extract_links(&note.source)
                .into_iter()
                .filter_map(move |target| project.notes.iter().position(|note| note.name == target))
                .map(move |to| Edge {
                    from,
                    to: NoteId::from(to),
                })
        })
        .collect()
}

/// The number of graph edges touching each note — its own markdown links to other notes, plus
/// other notes' links to it — in `project.notes` order. Self-links don't count. Exposed for the
/// note list sidebar's "sort by connections" option.
pub fn connection_counts(project: &Project) -> Vec<usize> {
    let edges = resolve_edges(project);

    (0..project.notes.len())
        .map(NoteId::from)
        .map(|id| {
            edges
                .iter()
                .filter(|edge| edge.from != edge.to && (edge.from == id || edge.to == id))
                .count()
        })
        .collect()
}
