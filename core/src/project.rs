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
    /// This note's category (see [`Project::categories`]), by name, if it has one — colors its
    /// node in the graph view. `None` (the field is omitted from the save file entirely) draws
    /// it with the graph view's default node color instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !value
}

/// A named color for grouping notes in the graph view, e.g. "Character", "Place", "Event" —
/// see [`Project::categories`]. Unlike [`crate::settings::UiPalette`] and friends, this is
/// per-project rather than a global app setting, since different projects want different
/// categories (or none at all).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    #[serde(with = "crate::hex_color")]
    pub color: [u8; 3],
}

/// On-disk representation of a project file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectFile {
    #[serde(default)]
    notes: Vec<Note>,
    #[serde(default)]
    categories: Vec<Category>,
}

/// A story project: a set of notes persisted to a single, human-readable file.
#[derive(Debug, Default)]
pub struct Project {
    /// The file this project was opened from or last saved to. `None` for a new, unsaved project.
    pub path: Option<PathBuf>,
    pub notes: Vec<Note>,
    /// The project's note categories, available to assign to any [`Note`] via
    /// [`Note::category`] — see [`Self::rename_category`] and [`Self::delete_category`] for
    /// the operations that also need to keep notes' assignments in sync.
    pub categories: Vec<Category>,
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
            categories: file.categories,
        })
    }

    /// Saves the project to `path`. Does not change [`Self::path`]; callers that want the
    /// project to remember this as its file should set it themselves after a successful save.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let file = ProjectFile {
            notes: self.notes.clone(),
            categories: self.categories.clone(),
        };

        let text = toml::to_string_pretty(&file).map_err(io::Error::other)?;

        log::debug!("Writing {} note(s) to {}", file.notes.len(), path.display());

        fs::write(path, text)
    }

    pub fn create_note(&mut self, name: &str) -> io::Result<NoteId> {
        let name = validate_name(
            name,
            self.notes.iter().map(|note| note.name.as_str()),
            "Note",
            "creation",
        )?;

        self.notes.push(Note {
            name: name.clone(),
            source: String::new(),
            is_manuscript: false,
            category: None,
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
        let old_name = self.notes[usize::from(id)].name.clone();

        let other_names = self
            .notes
            .iter()
            .enumerate()
            .filter(|&(index, _)| NoteId::from(index) != id)
            .map(|(_, note)| note.name.as_str());
        let new_name = validate_name(new_name, other_names, "Note", "rename")?;

        if new_name == old_name {
            return Ok(id);
        }

        self.notes[usize::from(id)].name = new_name.clone();

        for note in &mut self.notes {
            note.source = markdown::rename_links(&note.source, &old_name, &new_name);
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

    /// Adds a new category named `name` in `color`. Rejects a name already in use by another
    /// category, the same as [`Self::create_note`] does for note names.
    pub fn add_category(&mut self, name: &str, color: [u8; 3]) -> io::Result<()> {
        let name = validate_name(
            name,
            self.categories
                .iter()
                .map(|category| category.name.as_str()),
            "Category",
            "creation",
        )?;

        self.categories.push(Category { name, color });

        Ok(())
    }

    /// Renames the category `old_name` to `new_name`, updating every note assigned to it (see
    /// [`Note::category`]) so they stay assigned to the renamed category rather than being
    /// silently orphaned.
    pub fn rename_category(&mut self, old_name: &str, new_name: &str) -> io::Result<()> {
        let other_names = self
            .categories
            .iter()
            .filter(|category| category.name != old_name)
            .map(|category| category.name.as_str());
        let new_name = validate_name(new_name, other_names, "Category", "rename")?;

        if old_name == new_name {
            return Ok(());
        }

        let Some(category) = self
            .categories
            .iter_mut()
            .find(|category| category.name == old_name)
        else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "No such category"));
        };
        category.name = new_name.clone();

        for note in &mut self.notes {
            if note.category.as_deref() == Some(old_name) {
                note.category = Some(new_name.clone());
            }
        }

        Ok(())
    }

    /// Removes the category `name`, clearing it (back to no category) from every note currently
    /// assigned to it.
    pub fn delete_category(&mut self, name: &str) {
        self.categories.retain(|category| category.name != name);

        for note in &mut self.notes {
            if note.category.as_deref() == Some(name) {
                note.category = None;
            }
        }
    }

    /// The color of `note`'s assigned category (see [`Note::category`]), if it has one and that
    /// category still exists — used to color its node's border in the graph view, in place of
    /// the default border color; a node's fill and its connections' colors are never affected by
    /// its category.
    pub fn category_color(&self, note: &Note) -> Option<[u8; 3]> {
        let name = note.category.as_deref()?;
        self.categories
            .iter()
            .find(|category| category.name == name)
            .map(|category| category.color)
    }

    /// The color of the category assigned to the note named `name`, if a note by that name exists
    /// in the project and has a category assigned — used to color a rendered hyperlink by the
    /// category of the note it links to, the same way [`Self::category_color`] colors that note's
    /// own node border in the graph view.
    pub fn category_color_by_name(&self, name: &str) -> Option<[u8; 3]> {
        let note = self.notes.iter().find(|note| note.name == name)?;
        self.category_color(note)
    }
}

/// Trims `name` and validates it for [`Project::create_note`]/[`Project::rename_note`]/
/// [`Project::add_category`]/[`Project::rename_category`]: rejected (with a warning logged and
/// an `io::Error`) if empty, or if it collides with any of `existing_names` — callers doing a
/// rename should exclude the item's own current name from that iterator, so renaming something
/// to its own name isn't mistaken for a collision. `kind` (e.g. "Note", "Category") names the
/// item type for the error message, and `action` (e.g. "creation", "rename") the operation, for
/// the log line.
fn validate_name<'a>(
    name: &str,
    mut existing_names: impl Iterator<Item = &'a str>,
    kind: &str,
    action: &str,
) -> io::Result<String> {
    let name = name.trim();

    if name.is_empty() {
        log::warn!(
            "Rejected {} {action}: name cannot be empty",
            kind.to_lowercase()
        );

        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{kind} name cannot be empty"),
        ));
    }

    if existing_names.any(|existing| existing == name) {
        log::warn!(
            "Rejected {} {action}: '{name}' already exists",
            kind.to_lowercase()
        );

        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("A {} with this name already exists", kind.to_lowercase()),
        ));
    }

    Ok(name.to_owned())
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
                    category: None,
                },
                Note {
                    name: "Manuscript".to_owned(),
                    source: String::new(),
                    is_manuscript: true,
                    category: None,
                },
            ],
            categories: Vec::new(),
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

    #[test]
    fn add_category_rejects_a_name_already_in_use() {
        let mut project = Project::new();
        project.add_category("Character", [255, 0, 0]).unwrap();

        assert!(project.add_category("Character", [0, 255, 0]).is_err());
        assert_eq!(project.categories.len(), 1);
        assert_eq!(project.categories[0].color, [255, 0, 0]);
    }

    #[test]
    fn rename_category_updates_the_name_and_every_note_assigned_to_it() {
        let mut project = Project::new();
        project.add_category("Character", [255, 0, 0]).unwrap();
        let alice = project.create_note("Alice").unwrap();
        project.notes[usize::from(alice)].category = Some("Character".to_owned());

        project.rename_category("Character", "Person").unwrap();

        assert_eq!(project.categories[0].name, "Person");
        assert_eq!(
            project.notes[usize::from(alice)].category.as_deref(),
            Some("Person")
        );
    }

    #[test]
    fn rename_category_rejects_a_name_already_in_use() {
        let mut project = Project::new();
        project.add_category("Character", [255, 0, 0]).unwrap();
        project.add_category("Place", [0, 255, 0]).unwrap();

        assert!(project.rename_category("Character", "Place").is_err());
        assert_eq!(project.categories[0].name, "Character");
    }

    #[test]
    fn delete_category_removes_it_and_clears_it_from_every_note_assigned_to_it() {
        let mut project = Project::new();
        project.add_category("Character", [255, 0, 0]).unwrap();
        let alice = project.create_note("Alice").unwrap();
        project.notes[usize::from(alice)].category = Some("Character".to_owned());

        project.delete_category("Character");

        assert!(project.categories.is_empty());
        assert_eq!(project.notes[usize::from(alice)].category, None);
    }

    #[test]
    fn category_color_resolves_a_note_s_assigned_category() {
        let mut project = Project::new();
        project.add_category("Character", [255, 0, 0]).unwrap();
        let alice = project.create_note("Alice").unwrap();
        project.notes[usize::from(alice)].category = Some("Character".to_owned());
        let bob = project.create_note("Bob").unwrap();

        assert_eq!(
            project.category_color(&project.notes[usize::from(alice)]),
            Some([255, 0, 0])
        );
        assert_eq!(
            project.category_color(&project.notes[usize::from(bob)]),
            None
        );
    }

    #[test]
    fn category_color_by_name_resolves_a_note_s_assigned_category() {
        let mut project = Project::new();
        project.add_category("Character", [255, 0, 0]).unwrap();
        let alice = project.create_note("Alice").unwrap();
        project.notes[usize::from(alice)].category = Some("Character".to_owned());
        project.create_note("Bob").unwrap();

        assert_eq!(project.category_color_by_name("Alice"), Some([255, 0, 0]));
        assert_eq!(project.category_color_by_name("Bob"), None);
    }

    #[test]
    fn category_color_by_name_returns_none_for_a_name_with_no_matching_note() {
        let project = Project::new();

        assert_eq!(project.category_color_by_name("Nobody"), None);
    }
}
