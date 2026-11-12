//! Files changed both here and in the project's repository (Collaborate
//! 2.0, P5; `axcenity_repo::conflicts`). A sync pass that finds them stops
//! with nothing overwritten and the repository's version written beside
//! each; the Console's `conflicts` lists them and `resolve` chooses (the
//! panel's Resolve window is P6). Once every file has a choice the merge is
//! made, recorded, and uploaded by the next pass.

use std::path::PathBuf;

use axcenity_repo::conflicts::{self, Choice, Conflict, ResolveError};

use super::*;

impl EditorApp {
    /// The project folder and branch, when this computer may change the
    /// repository now (linked, the one that commits, no pass running).
    fn conflict_repo(&mut self) -> Result<(PathBuf, String), String> {
        let folder = self
            .project
            .as_ref()
            .map(|p| p.folder.clone())
            .ok_or("no project is open")?;
        let link = self
            .repo
            .link
            .clone()
            .ok_or("this project isn't linked to a repository")?;
        if !self.repo_commits_here() {
            return Err("in a session the host handles the repository".into());
        }
        if self.repo.schedule.is_running() {
            return Err("a sync is running; try again in a moment".into());
        }
        Ok((folder, link.branch))
    }

    /// The files in conflict now, worked out from the repository.
    pub(crate) fn repo_conflicts(&mut self) -> Result<Vec<Conflict>, String> {
        let (folder, branch) = self.conflict_repo()?;
        let git = self.repo.manager().git.clone();
        conflicts::list(&git, &folder, &branch).map_err(|e| e.to_string())
    }

    /// The Console's `conflicts`.
    pub(super) fn conflicts_command(&mut self) {
        let found = match self.repo_conflicts() {
            Ok(found) => found,
            Err(why) => {
                self.output.info(format!("Collaborate: {why}."));
                return;
            }
        };
        if found.is_empty() {
            self.output
                .info("No conflicts: the project and its repository agree.");
            return;
        }
        let mut lines = vec![format!(
            "{} changed both here and in the repository:",
            plural(found.len(), "file")
        )];
        for conflict in &found {
            let chosen = self
                .repo
                .choices
                .iter()
                .find(|(path, _)| *path == conflict.path)
                .map(|(_, choice)| format!(" (chosen: {})", choice.word()))
                .unwrap_or_default();
            lines.push(format!(
                "  {}{chosen}: here {}, in the repository {}{}",
                conflict.path,
                side(&conflict.mine),
                side(&conflict.theirs),
                match &conflict.copy {
                    Some(copy) => format!("; theirs is beside it as {copy}"),
                    None => String::new(),
                }
            ));
        }
        lines.push(
            "Choose with `resolve mine|theirs|both [file]` (both keeps theirs as the copy)."
                .to_owned(),
        );
        self.output.info(lines.join("\n"));
    }

    /// The Console's `resolve <mine|theirs|both> [file]`.
    pub(super) fn resolve_command(&mut self, rest: &str) {
        let (word, path) = rest
            .split_once(char::is_whitespace)
            .map_or((rest, ""), |(w, p)| (w, p.trim()));
        let Some(choice) = Choice::parse(word) else {
            self.output
                .warning("Collaborate: say `resolve mine`, `resolve theirs` or `resolve both`.");
            return;
        };
        match self.choose_resolution(choice, (!path.is_empty()).then_some(path)) {
            Ok(message) => self.output.info(format!("Collaborate: {message}")),
            Err(why) => self.output.warning(format!("Collaborate: {why}")),
        }
    }

    /// Chooses `choice` for `path` (every file when `None`), and merges
    /// once every conflicting file has a choice. Says what happened.
    pub(crate) fn choose_resolution(
        &mut self,
        choice: Choice,
        path: Option<&str>,
    ) -> Result<String, String> {
        let found = self.repo_conflicts()?;
        if found.is_empty() {
            self.repo.choices.clear();
            return Err("there's nothing to resolve: the project and its repository agree".into());
        }
        // Choices for files no longer in conflict go.
        self.repo
            .choices
            .retain(|(p, _)| found.iter().any(|c| c.path == *p));
        let paths: Vec<String> = match path {
            None => found.iter().map(|c| c.path.clone()).collect(),
            Some(path) => {
                let path = path.replace('\\', "/");
                if !found.iter().any(|c| c.path == path) {
                    return Err(format!(
                        "`{path}` isn't in conflict (`conflicts` lists them)"
                    ));
                }
                vec![path]
            }
        };
        for path in paths {
            self.repo.choices.retain(|(p, _)| *p != path);
            self.repo.choices.push((path, choice));
        }
        let waiting = found.len() - self.repo.choices.len();
        if waiting > 0 {
            return Ok(format!(
                "noted; {} still to choose",
                plural(waiting, "file")
            ));
        }
        let (folder, branch) = self.conflict_repo()?;
        let author = self.repo_author(&folder);
        let git = self.repo.manager().git.clone();
        let choices = std::mem::take(&mut self.repo.choices);
        match conflicts::resolve(&git, &folder, &branch, &choices, &author) {
            Ok(resolved) => {
                self.repo.schedule.resolved();
                Ok(format!(
                    "resolved {}; uploading the merge",
                    plural(resolved.entries.len(), "file")
                ))
            }
            Err(ResolveError::Undecided(file)) => {
                // Something new conflicts since: keep what was chosen.
                self.repo.choices = choices;
                Err(format!("{file} changed in the meantime; choose for it too"))
            }
            Err(error) => {
                self.repo.choices = choices;
                Err(error.to_string())
            }
        }
    }
}

fn plural(n: usize, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

/// One side of a conflict in words: who changed it, or that it's gone.
fn side(version: &conflicts::Version) -> String {
    if version.deleted {
        return match &version.author {
            Some(who) => format!("deleted by {who}"),
            None => "deleted".to_owned(),
        };
    }
    let who = version.author.as_deref().unwrap_or("someone");
    match version.size {
        Some(size) => format!("changed by {who} ({size} bytes)"),
        None => format!("changed by {who}"),
    }
}
