//! An open project: where it lives, and its undo history.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::song::project::{FILE, Project, RECOVERY_DIR};

/// Undo steps kept.
const HISTORY: usize = 200;

pub struct Session {
    pub project: Project,
    pub dir: PathBuf,
    /// Changed since it was opened or saved.
    pub edited: bool,
    /// Its file exists: it was opened from one, or has been saved.
    pub on_disk: bool,
    /// The edits are kept in the recovery folder, as they are now.
    pub kept: bool,
    /// `edited` before the last checkpoint, for `cancel`.
    was_edited: bool,
    // ponytail: whole-project snapshots; store diffs if projects get big enough to notice.
    undo: Vec<Project>,
    redo: Vec<Project>,
}

impl Session {
    /// Opens the project in `dir`, or starts a new one there if it has none.
    pub fn open(dir: &Path) -> Result<Session, String> {
        let on_disk = dir.join(FILE).exists();
        let project = if on_disk {
            Project::load(dir)?
        } else {
            Project::new(48_000)
        };
        Ok(Session {
            project,
            dir: dir.to_path_buf(),
            edited: false,
            on_disk,
            kept: false,
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
        self.kept = false;
    }

    /// Whether nothing's changed since the last checkpoint.
    pub fn unchanged(&self) -> bool {
        self.undo.last() == Some(&self.project)
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
        self.kept = false;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(p) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.project, p));
        self.edited = true;
        self.kept = false;
        true
    }

    pub fn save(&mut self) -> Result<(), String> {
        self.project.save(&self.dir)?;
        self.edited = false;
        self.on_disk = true;
        Ok(())
    }

    /// The folder unsaved edits are kept in: a project whose files are the real one's.
    pub fn recovery(&self) -> PathBuf {
        self.dir.join(RECOVERY_DIR)
    }

    /// Keeps `project` (the edits, with its plugins' settings in the recovery folder) for if
    /// Pina Studio stops before they're saved.
    pub fn keep(&mut self, project: &Project) -> Result<(), String> {
        project.save(&self.recovery())?;
        self.kept = true;
        Ok(())
    }

    /// When the edits in the recovery folder were kept, if there are any: Pina Studio stopped
    /// before saving or discarding them.
    pub fn kept_at(&self) -> Option<SystemTime> {
        std::fs::metadata(self.recovery().join(FILE))
            .ok()?
            .modified()
            .ok()
    }

    /// Opens the kept edits in place of the saved project, which undo goes back to.
    pub fn restore(&mut self) -> Result<(), String> {
        let kept = Project::load(&self.recovery())?;
        self.checkpoint();
        self.project = kept;
        self.kept = true;
        Ok(())
    }

    /// Drops the kept edits: they were saved, or aren't wanted.
    pub fn forget_kept(&mut self) {
        let _ = std::fs::remove_dir_all(self.recovery());
        self.kept = false;
    }
}

#[cfg(test)]
mod tests;
