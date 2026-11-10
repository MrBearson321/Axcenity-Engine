//! Find in Scripts' search over the open project, going to a match, and
//! Replace All (unified Phase 70; the search and window are
//! `find_in_scripts.rs`). Each script is searched as the editor has it:
//! a tab's unsaved code, a block script's unsaved blocks. Replace All
//! writes every script it changes and is one undo step, which puts the
//! files, the open tabs and the block scripts back.

use std::fs;
use std::sync::Arc;

use crate::find_in_scripts::{self, Hit, Matcher, Spot};
use crate::history::BlobChange;

use super::*;
use axcenity_project as project;

/// One script as the editor has it.
enum Source {
    Code(String),
    Blocks(BlockScript),
}

impl EditorApp {
    /// Every script and block script, by path, as the editor has it now. A
    /// UI Event Graph's LocalScript is made from its graph, so it's left out.
    fn script_sources(&self) -> Vec<(String, Source)> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let mut sources = Vec::new();
        for script in &project.scripts {
            if axcenity_blocks::ui_graph::is_ui_graph(&script.path) {
                continue;
            }
            if project::is_block_script(&script.path) {
                if let Some(document) = project
                    .block_scripts
                    .iter()
                    .find(|document| document.path == script.path)
                {
                    sources.push((script.path.clone(), Source::Blocks(document.script.clone())));
                }
                continue;
            }
            let text = match self.tabs.get(&script.path) {
                Some(tab) => tab.text.clone(),
                None => script.source.clone(),
            };
            sources.push((script.path.clone(), Source::Code(text)));
        }
        sources
    }

    fn find_matcher(&self) -> Result<Matcher, String> {
        Matcher::new(&self.find.query, self.find.options)
    }

    /// Searches again for what Find in Scripts has typed.
    pub(super) fn search_scripts(&mut self) {
        self.find.results = self.find_matcher().map(|matcher| {
            let mut hits = Vec::new();
            for (path, source) in self.script_sources() {
                match source {
                    Source::Code(text) => {
                        hits.extend(find_in_scripts::search_code(&path, &text, &matcher));
                    }
                    Source::Blocks(script) => {
                        hits.extend(find_in_scripts::search_blocks(&path, &script, &matcher));
                    }
                }
            }
            hits
        });
    }

    /// Opens a match: the script with the cursor on its line, or the block
    /// script with its block highlighted.
    pub(super) fn go_to_hit(&mut self, hit: &Hit) {
        match hit.spot {
            Spot::Line { line, .. } => self.open_script(&hit.path, Some(line)),
            Spot::Block(block) => self.open_block(&hit.path, block),
        }
    }

    /// Opens the block script at `path` with `block` highlighted.
    pub(super) fn open_block(&mut self, path: &str, block: Option<u64>) {
        self.open_script(path, None);
        if let Some(document) = self.project.as_mut().and_then(|project| {
            project
                .block_scripts
                .iter_mut()
                .find(|document| document.path == path)
        }) {
            document.highlighted = block;
        }
    }

    /// Replaces every match in every script, writing each script it
    /// changes, as one undo step. Not during Play.
    pub(super) fn replace_all_in_scripts(&mut self) {
        if self.playing() {
            self.output.error("Press Stop to replace in scripts.");
            return;
        }
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let matcher = match self.find_matcher() {
            Ok(matcher) => matcher,
            Err(err) => {
                self.output.error(format!("Find in Scripts: {err}."));
                return;
            }
        };
        let replacement = self.find.replacement.clone();
        let mut blobs = Vec::new();
        let mut total = 0;
        for (path, source) in self.script_sources() {
            let (after, count) = match source {
                Source::Code(text) => matcher.replace(&text, &replacement),
                Source::Blocks(script) => {
                    match find_in_scripts::replace_blocks(&script, &matcher, &replacement) {
                        Ok((_, 0)) => continue,
                        Ok((script, count)) => match serde_json::to_string_pretty(&script) {
                            Ok(text) => (text, count),
                            Err(err) => {
                                self.output
                                    .error(format!("Could not replace in `{path}`: {err}"));
                                return;
                            }
                        },
                        Err(err) => {
                            self.output.error(format!(
                                "Could not replace in `{path}`: the blocks wouldn't be blocks \
                                 any more ({err})."
                            ));
                            return;
                        }
                    }
                }
            };
            if count == 0 {
                continue;
            }
            total += count;
            blobs.push(BlobChange {
                before: fs::read(folder.join(&path)).ok().map(Arc::new),
                after: Some(Arc::new(after.into_bytes())),
                path,
            });
        }
        if blobs.is_empty() {
            return;
        }
        let files = blobs.len();
        let written = self.write_package_files(&blobs);
        let blobs: Vec<BlobChange> = blobs.into_iter().take(written).collect();
        let label = format!(
            "Replace {total} {} of `{}`",
            if total == 1 { "match" } else { "matches" },
            self.find.query
        );
        self.output.info(format!(
            "{label} in {files} {}.",
            if files == 1 { "script" } else { "scripts" }
        ));
        self.history.record_blobs(&mut self.world, label, blobs);
        self.search_scripts();
    }

    /// The script at `path` now has `text` on disk, written by the editor
    /// (Replace All, or its undo): the project's copy, its block script
    /// and its tab take it, without saying it changed outside the editor.
    pub(super) fn take_script_text(&mut self, path: &str, text: &str) {
        let source = if project::is_block_script(path) {
            let Ok(script) = serde_json::from_str::<BlockScript>(text) else {
                return;
            };
            axcenity_blocks::reserve_block_ids(&script);
            let Ok(translation) = translate(&script) else {
                return;
            };
            if let Some(document) = self.project.as_mut().and_then(|project| {
                project
                    .block_scripts
                    .iter_mut()
                    .find(|document| document.path == path)
            }) {
                document.script = script.clone();
                document.saved = script;
                document.undo.clear();
            }
            translation.source
        } else {
            text.to_owned()
        };
        if let Some(script) = self.project.as_mut().and_then(|project| {
            project
                .scripts
                .iter_mut()
                .find(|script| script.path == path)
        }) {
            script.source = source.clone();
        }
        if let Some(tab) = self.tabs.get_mut(path) {
            tab.text = source;
            tab.mark_saved();
        }
        self.reload_in_play(path);
    }
}

/// Whether `path` is a script file (Lua or blocks) under `scripts/`.
pub(super) fn is_script_file(path: &str) -> bool {
    path.starts_with(&format!("{}/", project::SCRIPTS_FOLDER))
        && (path.ends_with(".lua") || project::is_block_script(path))
}
