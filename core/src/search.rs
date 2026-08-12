use crate::project::{NoteId, Project};

/// State for the project-wide note search window (opened with Ctrl+F).
#[derive(Default)]
pub struct Search {
    open: bool,
    pub query: String,
    request_focus: bool,
}

impl Search {
    /// Opens the search window, focusing its query field. Reopening an already-open window just
    /// refocuses it, leaving whatever the user already typed in place.
    pub fn open(&mut self) {
        if !self.open {
            self.query.clear();
        }
        self.open = true;
        self.request_focus = true;
    }

    /// Whether the search window is currently showing.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Closes the search window without clearing the query.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Whether the query field should grab keyboard focus this frame — true only once, the first
    /// time this is called after [`Self::open`].
    pub fn take_request_focus(&mut self) -> bool {
        std::mem::replace(&mut self.request_focus, false)
    }
}

/// The indices into `project.notes`, in project order, whose name or content contains `query`
/// (case-insensitive). Empty for a blank or whitespace-only `query`, rather than matching every
/// note.
pub fn search_notes(project: &Project, query: &str) -> Vec<NoteId> {
    let query = query.trim();

    if query.is_empty() {
        return Vec::new();
    }

    let query = query.to_lowercase();

    (0..project.notes.len())
        .map(NoteId::from)
        .filter(|&index| {
            let note = &project.notes[usize::from(index)];
            note.name.to_lowercase().contains(&query) || note.source.to_lowercase().contains(&query)
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
    fn search_notes_matches_name_or_content_case_insensitively() {
        let project = Project {
            path: None,
            notes: vec![
                note("Alice", "Lives in the old lighthouse."),
                note("Bob", "Alice's brother."),
                note("Lighthouse", "A landmark on the cliffs."),
            ],
            categories: Vec::new(),
        };

        assert_eq!(
            search_notes(&project, "alice"),
            vec![NoteId::from(0), NoteId::from(1)]
        );
        assert_eq!(
            search_notes(&project, "LIGHTHOUSE"),
            vec![NoteId::from(0), NoteId::from(2)]
        );
        assert_eq!(search_notes(&project, "brother"), vec![NoteId::from(1)]);
        assert!(search_notes(&project, "nonexistent").is_empty());
    }

    #[test]
    fn search_notes_treats_a_blank_query_as_no_matches() {
        let project = Project {
            path: None,
            notes: vec![note("Alice", "")],
            categories: Vec::new(),
        };

        assert!(search_notes(&project, "").is_empty());
        assert!(search_notes(&project, "   ").is_empty());
    }
}
