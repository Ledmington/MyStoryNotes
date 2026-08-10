use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// A single note: a name and its markdown content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub name: String,
    pub source: String,
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

    pub fn create_note(&mut self, name: &str) -> io::Result<usize> {
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
        });

        self.notes.sort_by(|a, b| a.name.cmp(&b.name));

        self.notes
            .iter()
            .position(|note| note.name == name)
            .ok_or_else(|| io::Error::other("Created note could not be found"))
    }
}
