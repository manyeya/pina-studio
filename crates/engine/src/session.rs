//! An open project: where it lives, and its undo history.

use std::path::{Path, PathBuf};

use crate::project::{FILE, Project};

/// Undo steps kept.
const HISTORY: usize = 200;

pub struct Session {
    pub project: Project,
    pub dir: PathBuf,
    /// Changed since it was opened or saved.
    pub edited: bool,
    /// `edited` before the last checkpoint, for `cancel`.
    was_edited: bool,
    // ponytail: whole-project snapshots; store diffs if projects get big enough to notice.
    undo: Vec<Project>,
    redo: Vec<Project>,
}

impl Session {
    /// Opens the project in `dir`, or starts a new one there if it has none.
    pub fn open(dir: &Path) -> Result<Session, String> {
        let project = if dir.join(FILE).exists() {
            Project::load(dir)?
        } else {
            Project::new(48_000)
        };
        Ok(Session {
            project,
            dir: dir.to_path_buf(),
            edited: false,
            was_edited: false,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// Remembers the project as it is, for undo. Call before each edit; a drag is one edit.
    pub fn checkpoint(&mut self) {
        if self.undo.len() == HISTORY {
            self.undo.remove(0);
        }
        self.undo.push(self.project.clone());
        self.redo.clear();
        self.was_edited = self.edited;
        self.edited = true;
    }

    /// Forgets the last checkpoint, for an edit that turned out to change nothing.
    pub fn cancel(&mut self) {
        self.undo.pop();
        self.edited = self.was_edited;
    }

    pub fn undo(&mut self) -> bool {
        let Some(p) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.project, p));
        self.edited = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(p) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.project, p));
        self.edited = true;
        true
    }

    pub fn save(&mut self) -> Result<(), String> {
        self.project.save(&self.dir)?;
        self.edited = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo() {
        let mut s = Session::open(Path::new("/nonexistent")).unwrap();
        s.checkpoint();
        s.project.tempo = 90.0;
        s.checkpoint();
        s.project.tempo = 100.0;
        assert!(s.undo());
        assert_eq!(s.project.tempo, 90.0);
        assert!(s.undo());
        assert_eq!(s.project.tempo, 120.0);
        assert!(!s.undo());
        assert!(s.redo());
        assert_eq!(s.project.tempo, 90.0);
        s.checkpoint(); // a new edit drops what could be redone
        assert!(!s.redo());

        let mut s = Session::open(Path::new("/nonexistent")).unwrap();
        s.checkpoint();
        s.cancel(); // an edit that changed nothing
        assert!(!s.edited && !s.undo());
    }
}
