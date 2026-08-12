use crate::markdown::{Block, collect_blocks};
use crate::project::{NoteId, Project};

/// One TODO found in a project: which note it's in, and the todo paragraph's full text
/// (including its "TODO" marker, exactly as written — see [`is_todo`]).
pub struct Todo {
    pub note: NoteId,
    pub text: String,
}

/// Whether `text` (typically a paragraph's flattened plain text — see [`Block::text`]) starts
/// with a "TODO" marker: the word "todo", matched case-insensitively, immediately followed by a
/// word boundary (anything other than a letter/digit, or the end of the text) so a paragraph
/// starting with e.g. "Todoist" isn't mistaken for one. Leading whitespace is ignored. Shared by
/// [`find_todos`] and the GUI's note renderer, so the two can never disagree about what counts.
pub fn is_todo(text: &str) -> bool {
    let mut chars = text.trim_start().chars();
    let prefix: String = chars.by_ref().take(4).collect();

    prefix.eq_ignore_ascii_case("todo") && !chars.next().is_some_and(char::is_alphanumeric)
}

/// Every TODO paragraph across every note in `project`, in `project.notes` order and, within a
/// note, document order. Recomputed fresh every call — a TODO isn't stored anywhere except as
/// plain text in its note's own source, the same way [`crate::search::search_notes`] and
/// [`crate::graph::resolve_edges`] derive their results from [`Project`] without any cached
/// state. Includes the manuscript note like `search_notes` does (unlike `graph::resolve_edges`,
/// which excludes it for an unrelated reason — it would dominate the link graph).
pub fn find_todos(project: &Project) -> Vec<Todo> {
    project
        .notes
        .iter()
        .enumerate()
        .flat_map(|(index, note)| {
            let note_id = NoteId::from(index);
            collect_blocks(&note.source)
                .into_iter()
                .filter_map(move |block| match block {
                    Block::Paragraph { .. } => {
                        let text = block.text();
                        is_todo(&text).then_some(Todo {
                            note: note_id,
                            text,
                        })
                    }
                    Block::Heading { .. } => None,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(name: &str, source: &str) -> crate::project::Note {
        crate::project::Note {
            name: name.to_owned(),
            source: source.to_owned(),
            is_manuscript: false,
            category: None,
        }
    }

    #[test]
    fn is_todo_matches_the_word_todo_at_the_start_regardless_of_case_or_leading_whitespace() {
        assert!(is_todo("TODO: fix this"));
        assert!(is_todo("todo fix this"));
        assert!(is_todo("  TODO: leading whitespace"));
        assert!(is_todo("Todo"));
    }

    #[test]
    fn is_todo_rejects_todo_as_a_prefix_of_a_longer_word() {
        assert!(!is_todo("TODOs: plural"));
        assert!(!is_todo("Todoist app mention"));
    }

    #[test]
    fn is_todo_rejects_todo_that_is_not_at_the_start() {
        assert!(!is_todo("This has TODO in the middle"));
        assert!(!is_todo(""));
    }

    #[test]
    fn find_todos_collects_only_todo_paragraphs_across_every_note_in_order() {
        let project = Project {
            path: None,
            notes: vec![
                note(
                    "Alice",
                    "Just a regular paragraph.\n\nTODO: flesh out backstory.\n\nAnother regular one.\n\nTODO: pick a surname.",
                ),
                note("Bob", "Nothing to do here."),
            ],
            categories: Vec::new(),
        };

        let todos = find_todos(&project);

        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].note, NoteId::from(0));
        assert_eq!(todos[0].text, "TODO: flesh out backstory.");
        assert_eq!(todos[1].note, NoteId::from(0));
        assert_eq!(todos[1].text, "TODO: pick a surname.");
    }

    #[test]
    fn find_todos_includes_the_manuscript_note() {
        let mut manuscript = note("Manuscript", "TODO: rewrite this chapter.");
        manuscript.is_manuscript = true;
        let project = Project {
            path: None,
            notes: vec![manuscript],
            categories: Vec::new(),
        };

        let todos = find_todos(&project);

        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].text, "TODO: rewrite this chapter.");
    }
}
