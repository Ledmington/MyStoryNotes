mod collision;
mod layout;
mod simulation;

use crate::markdown;
use crate::project::{NoteId, Project};

pub use simulation::{Simulation, settle};

/// An edge's position in the resolved edge list, for tracking which one (if any) the mouse is
/// hovering. A thin `usize` wrapper so it can't be confused with a [`NoteId`], even though both
/// are array indices under the hood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionId(usize);

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
pub struct Edge {
    pub from: NoteId,
    pub to: NoteId,
}

/// Resolves every note's markdown links into [`Edge`]s indexing into `project.notes`, excluding
/// the manuscript note (see [`crate::project::Note::is_manuscript`]) on either end — it would
/// otherwise dominate the graph, being linked from just about every other note. A link to or from
/// it simply doesn't become an edge, the same as a link to a note that doesn't exist.
pub fn resolve_edges(project: &Project) -> Vec<Edge> {
    project
        .notes
        .iter()
        .enumerate()
        .filter(|(_, note)| !note.is_manuscript)
        .flat_map(|(from, note)| {
            let from = NoteId::from(from);

            markdown::extract_links(&note.source)
                .into_iter()
                .filter_map(move |target| {
                    project
                        .notes
                        .iter()
                        .position(|note| note.name == target && !note.is_manuscript)
                })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Note;

    fn note(name: &str, source: &str, is_manuscript: bool) -> Note {
        Note {
            name: name.to_owned(),
            source: source.to_owned(),
            is_manuscript,
            category: None,
        }
    }

    #[test]
    fn resolve_edges_excludes_the_manuscript_in_either_direction() {
        let project = Project {
            path: None,
            notes: vec![
                note(
                    "Alice",
                    "Linked to [Manuscript](Manuscript) and [Bob](Bob).",
                    false,
                ),
                note("Bob", "", false),
                note("Manuscript", "Mentions [Alice](Alice).", true),
            ],
            categories: Vec::new(),
        };

        let edges = resolve_edges(&project);

        assert_eq!(
            edges.len(),
            1,
            "only the Alice -> Bob link should resolve to an edge"
        );
        assert_eq!(edges[0].from, NoteId::from(0));
        assert_eq!(edges[0].to, NoteId::from(1));
    }

    #[test]
    fn connection_counts_ignores_manuscript_links() {
        let project = Project {
            path: None,
            notes: vec![
                note("Alice", "[Manuscript](Manuscript)", false),
                note("Manuscript", "[Alice](Alice)", true),
            ],
            categories: Vec::new(),
        };

        assert_eq!(connection_counts(&project), vec![0, 0]);
    }
}
