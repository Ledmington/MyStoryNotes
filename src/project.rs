use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::markdown;

/// A note's position in [`Project::notes`]. A thin `usize` wrapper so a note's identity can't be
/// silently mixed up with an edge's, a byte offset, or any other plain integer — not persisted:
/// the save file stores notes as a plain list, and a note's index can shift (e.g. on creation,
/// which keeps the list sorted by name), so an id only makes sense for the lifetime of a
/// `Project` value in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId(usize);

impl From<usize> for NoteId {
    fn from(index: usize) -> Self {
        Self(index)
    }
}

impl From<NoteId> for usize {
    fn from(id: NoteId) -> Self {
        id.0
    }
}

impl NoteId {
    /// How this id changes once the note at `removed` is deleted from [`Project::notes`]:
    /// `None` if this *was* `removed` (that note no longer exists), otherwise `self` shifted
    /// down by one if it came after `removed` in the list, or left unchanged if it came before.
    pub fn after_removing(self, removed: NoteId) -> Option<NoteId> {
        match self.cmp(&removed) {
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(NoteId::from(usize::from(self) - 1)),
            std::cmp::Ordering::Less => Some(self),
        }
    }
}

/// A single note: a name and its markdown content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub name: String,
    pub source: String,
    /// Whether this is the project's manuscript note — see [`Project::manuscript`]. There is
    /// only ever one per project. Omitted from the save file entirely for every other note,
    /// rather than writing out `is_manuscript = false` on every single one of them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_manuscript: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

/// On-disk representation of a project file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectFile {
    #[serde(default)]
    notes: Vec<Note>,
}

/// A story project: a set of notes persisted to a single, human-readable file.
#[derive(Debug, Default)]
pub struct Project {
    /// The file this project was opened from or last saved to. `None` for a new, unsaved project.
    pub path: Option<PathBuf>,
    pub notes: Vec<Note>,
}

impl Project {
    /// Creates an empty project that has not been saved to a file yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads a project from its save file.
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let text = fs::read_to_string(&path)?;

        let file: ProjectFile = toml::from_str(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

        log::debug!(
            "Parsed {} note(s) from {}",
            file.notes.len(),
            path.display()
        );

        Ok(Self {
            path: Some(path),
            notes: file.notes,
        })
    }

    /// Saves the project to `path`. Does not change [`Self::path`]; callers that want the
    /// project to remember this as its file should set it themselves after a successful save.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let file = ProjectFile {
            notes: self.notes.clone(),
        };

        let text = toml::to_string_pretty(&file).map_err(io::Error::other)?;

        log::debug!("Writing {} note(s) to {}", file.notes.len(), path.display());

        fs::write(path, text)
    }

    pub fn create_note(&mut self, name: &str) -> io::Result<NoteId> {
        let name = name.trim();

        if name.is_empty() {
            log::warn!("Rejected note creation: name cannot be empty");

            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Note name cannot be empty",
            ));
        }

        if self.notes.iter().any(|note| note.name == name) {
            log::warn!("Rejected note creation: '{name}' already exists");

            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "A note with this name already exists",
            ));
        }

        self.notes.push(Note {
            name: name.to_owned(),
            source: String::new(),
            is_manuscript: false,
        });

        self.notes.sort_by(|a, b| a.name.cmp(&b.name));

        self.notes
            .iter()
            .position(|note| note.name == name)
            .map(NoteId::from)
            .ok_or_else(|| io::Error::other("Created note could not be found"))
    }

    /// Renames the note at `id` to `new_name`, rewriting every other note's links to it (see
    /// [`markdown::rename_links`]) so the graph stays intact, and returns its id afterward (the
    /// rename may change its sorted position). Renaming a note to its current name is a no-op.
    pub fn rename_note(&mut self, id: NoteId, new_name: &str) -> io::Result<NoteId> {
        let new_name = new_name.trim();

        if new_name.is_empty() {
            log::warn!("Rejected note rename: name cannot be empty");

            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Note name cannot be empty",
            ));
        }

        let old_name = self.notes[usize::from(id)].name.clone();

        if new_name == old_name {
            return Ok(id);
        }

        if self.notes.iter().any(|note| note.name == new_name) {
            log::warn!("Rejected note rename: '{new_name}' already exists");

            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "A note with this name already exists",
            ));
        }

        self.notes[usize::from(id)].name = new_name.to_owned();

        for note in &mut self.notes {
            note.source = markdown::rename_links(&note.source, &old_name, new_name);
        }

        self.notes.sort_by(|a, b| a.name.cmp(&b.name));

        self.notes
            .iter()
            .position(|note| note.name == new_name)
            .map(NoteId::from)
            .ok_or_else(|| io::Error::other("Renamed note could not be found"))
    }

    /// Deletes the note at `id`. Other notes' links to it are left as-is (dangling, the same as
    /// a link to a note that never existed) rather than rewritten — consistent with how the app
    /// already tolerates links that don't resolve to anything.
    pub fn delete_note(&mut self, id: NoteId) {
        self.notes.remove(usize::from(id));
    }

    /// The project's manuscript note, if it has one.
    pub fn manuscript(&self) -> Option<NoteId> {
        self.notes
            .iter()
            .position(|note| note.is_manuscript)
            .map(NoteId::from)
    }

    /// Returns the project's manuscript note, creating it (named "Manuscript", or "Manuscript
    /// (2)" etc. if that name is already taken by an unrelated note) if it doesn't exist yet.
    /// There is only ever one per project.
    pub fn get_or_create_manuscript(&mut self) -> io::Result<NoteId> {
        if let Some(id) = self.manuscript() {
            return Ok(id);
        }

        let mut name = "Manuscript".to_owned();
        let mut suffix = 2;
        while self.notes.iter().any(|note| note.name == name) {
            name = format!("Manuscript ({suffix})");
            suffix += 1;
        }

        let id = self.create_note(&name)?;
        self.notes[usize::from(id)].is_manuscript = true;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_manuscript_is_omitted_from_the_save_file_unless_true() {
        let file = ProjectFile {
            notes: vec![
                Note {
                    name: "Alice".to_owned(),
                    source: String::new(),
                    is_manuscript: false,
                },
                Note {
                    name: "Manuscript".to_owned(),
                    source: String::new(),
                    is_manuscript: true,
                },
            ],
        };

        let text = toml::to_string_pretty(&file).unwrap();

        assert!(
            !text.contains("is_manuscript = false"),
            "ordinary notes shouldn't carry an explicit is_manuscript key:\n{text}"
        );
        assert!(
            text.contains("is_manuscript = true"),
            "the manuscript note should still be marked:\n{text}"
        );
    }

    #[test]
    fn get_or_create_manuscript_creates_it_once_and_reuses_it_after() {
        let mut project = Project::new();

        let first = project.get_or_create_manuscript().unwrap();
        assert_eq!(project.notes[usize::from(first)].name, "Manuscript");
        assert!(project.notes[usize::from(first)].is_manuscript);

        let second = project.get_or_create_manuscript().unwrap();
        assert_eq!(first, second);
        assert_eq!(project.notes.len(), 1);
    }

    #[test]
    fn get_or_create_manuscript_picks_a_free_name_if_manuscript_is_taken() {
        let mut project = Project::new();
        project.create_note("Manuscript").unwrap();

        let id = project.get_or_create_manuscript().unwrap();

        assert_eq!(project.notes[usize::from(id)].name, "Manuscript (2)");
        assert!(project.notes[usize::from(id)].is_manuscript);
        assert_eq!(
            project
                .notes
                .iter()
                .filter(|note| note.is_manuscript)
                .count(),
            1
        );
    }

    #[test]
    fn manuscript_returns_none_on_a_fresh_project() {
        let project = Project::new();
        assert_eq!(project.manuscript(), None);
    }

    #[test]
    fn rename_note_updates_the_name_and_every_link_to_it() {
        let mut project = Project::new();
        project.create_note("Bob").unwrap();
        project.create_note("Alice").unwrap();

        let find = |project: &Project, name: &str| {
            project
                .notes
                .iter()
                .position(|note| note.name == name)
                .map(NoteId::from)
                .unwrap()
        };

        let bob = find(&project, "Bob");
        project.notes[usize::from(bob)].source = "friends with [Alice](Alice)".to_owned();
        let alice = find(&project, "Alice");
        project.notes[usize::from(alice)].source = "waves at [Bob](Bob)".to_owned();

        let new_id = project.rename_note(bob, "Robert").unwrap();

        assert_eq!(project.notes[usize::from(new_id)].name, "Robert");
        assert_eq!(
            project.notes[usize::from(new_id)].source,
            "friends with [Alice](Alice)"
        );
        let alice_after = project
            .notes
            .iter()
            .find(|note| note.name == "Alice")
            .unwrap();
        assert_eq!(alice_after.source, "waves at [Bob](Robert)");
    }

    #[test]
    fn rename_note_to_its_own_name_is_a_no_op() {
        let mut project = Project::new();
        let bob = project.create_note("Bob").unwrap();

        let id = project.rename_note(bob, "Bob").unwrap();

        assert_eq!(id, bob);
        assert_eq!(project.notes.len(), 1);
    }

    #[test]
    fn rename_note_rejects_a_name_already_in_use() {
        let mut project = Project::new();
        project.create_note("Bob").unwrap();
        project.create_note("Alice").unwrap();
        let bob = project
            .notes
            .iter()
            .position(|note| note.name == "Bob")
            .map(NoteId::from)
            .unwrap();

        assert!(project.rename_note(bob, "Alice").is_err());
        assert_eq!(project.notes[usize::from(bob)].name, "Bob");
    }

    #[test]
    fn delete_note_removes_it_and_leaves_dangling_links_untouched() {
        let mut project = Project::new();
        let alice = project.create_note("Alice").unwrap();
        let bob = project.create_note("Bob").unwrap();
        project.notes[usize::from(bob)].source = "friends with [Alice](Alice)".to_owned();

        project.delete_note(alice);

        assert_eq!(project.notes.len(), 1);
        assert_eq!(project.notes[0].name, "Bob");
        assert_eq!(project.notes[0].source, "friends with [Alice](Alice)");
    }

    #[test]
    fn note_id_after_removing() {
        let removed = NoteId::from(1);

        assert_eq!(NoteId::from(1).after_removing(removed), None);
        assert_eq!(
            NoteId::from(0).after_removing(removed),
            Some(NoteId::from(0))
        );
        assert_eq!(
            NoteId::from(2).after_removing(removed),
            Some(NoteId::from(1))
        );
        assert_eq!(
            NoteId::from(5).after_removing(removed),
            Some(NoteId::from(4))
        );
    }
}
