//! Each person's history (T4 of `docs/research/teamwork.md`): what this
//! editor records into `team/history/`, and the People window that reads
//! it back.
//!
//! - In a Collaborate session the hosting editor records every member's
//!   transactions from the host's log (who, what, which Instances and
//!   files); members record nothing, so nothing is written twice.
//! - Outside a session this editor records its own person: each Save (what
//!   changed, from the version it keeps) and each Team Board change.
//!
//! Lines go to this computer's own files (named by its Collaborate key), so
//! two computers never append to the same file.

use std::path::PathBuf;

use axcenity_collab::Op;
use axcenity_teamwork::history::{self, Entry, History, Recorder, What};
use axcenity_teamwork::{Item, Link, board, time};
use eframe::egui;

use super::EditorApp;
use crate::people_history::{PeopleAction, PeopleWindow};

/// How often the host's log is read.
const HARVEST_SECONDS: u64 = 2;

/// What the history needs between frames.
#[derive(Default)]
pub(crate) struct PeopleHistory {
    pub(crate) window: PeopleWindow,
    /// Lines from the host's log still folding.
    recorder: Recorder,
    /// The host run (its epoch) and the newest transaction recorded.
    seen: Option<(u64, u64)>,
    /// When the host's log was last read.
    pub(crate) harvested: u64,
    /// This computer's code in file names (its Collaborate key), read once.
    pub(crate) computer: Option<String>,
    /// The project folder the history was read from, and what it says.
    loaded: Option<(PathBuf, History)>,
    /// A history file changed since it was read.
    pub(crate) stale: bool,
}

impl EditorApp {
    /// This computer's code in history file names: its Collaborate key's
    /// short fingerprint ("ab12-cd34").
    fn history_computer(&mut self) -> String {
        if let Some(code) = &self.team_board.history.computer {
            return code.clone();
        }
        let code = super::team::load_own_identity()
            .map(|identity| identity.public_key().fingerprint().short())
            .unwrap_or_else(|_| "this-computer".to_owned());
        self.team_board.history.computer = Some(code.clone());
        code
    }

    /// Records this editor's own lines now, unless it's in a session (where
    /// the host records everyone).
    pub(in crate::app) fn record_own_history(&mut self, entries: Vec<Entry>) {
        if self.team.is_some() || entries.is_empty() {
            return;
        }
        self.write_history(&entries);
    }

    fn write_history(&mut self, entries: &[Entry]) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let computer = self.history_computer();
        match history::append(&folder, &computer, entries) {
            Ok(()) => self.team_board.history.stale = true,
            Err(error) => self
                .output
                .warning(format!("Couldn't keep the project's history: {error}")),
        }
    }

    /// A Save's line: what changed since the version before, as kept by
    /// Version History.
    pub(in crate::app) fn record_save_history(&mut self, folder: &std::path::Path) {
        if self.team.is_some() {
            return;
        }
        let versions = axcenity_project::list_versions(folder);
        let Some(newest) = versions.first() else {
            return;
        };
        let me = self.board_me();
        if versions.len() == 1 {
            // Nothing before it: every file would read as added.
            let entry = Entry::new(time::now(), &me, What::Save, "Saved the first version");
            self.record_own_history(vec![entry]);
            return;
        }
        let Ok(changes) = axcenity_project::version_changes(folder, &newest.id) else {
            return;
        };
        if changes.is_empty() {
            return;
        }
        let sentences = changes.sentences();
        let text = match sentences.as_slice() {
            [one] => one.clone(),
            many => format!("Saved {} changes", many.len()),
        };
        let mut links = Vec::new();
        for file in &changes.files {
            if file.instances.is_empty() {
                if !history::is_history_path(&file.path) {
                    links.push(Link::File {
                        path: file.path.clone(),
                        line: None,
                    });
                }
                continue;
            }
            for instance in &file.instances {
                let parent = instance.parent.replace('/', ".");
                links.push(Link::Instance(if parent.is_empty() {
                    instance.name.clone()
                } else {
                    format!("{parent}.{}", instance.name)
                }));
            }
        }
        let entry = Entry::new(time::now(), &me, What::Save, &text)
            .with_links(links)
            .with_details(if sentences.len() > 1 {
                sentences
            } else {
                Vec::new()
            });
        self.record_own_history(vec![entry]);
    }

    /// A board change's line (`before` is `None` for a new item).
    pub(in crate::app) fn record_board_history(&mut self, before: Option<&Item>, after: &Item) {
        let me = self.board_me();
        let entry = history::board_entry(before, after, &me, time::now());
        self.record_own_history(vec![entry]);
    }

    /// Each frame: the host's new transactions, lines done folding, and the
    /// People window.
    pub(in crate::app) fn people_frame(&mut self, ctx: &egui::Context) {
        let now = time::now();
        if now >= self.team_board.history.harvested + HARVEST_SECONDS {
            self.team_board.history.harvested = now;
            self.harvest_host_log(now);
        }
        let ready = self.team_board.history.recorder.take(now, false);
        if !ready.is_empty() {
            self.write_history(&ready);
        }
        if self.team_board.panel.people {
            self.people_window(ctx);
        }
    }

    /// Writes every line still folding (the window is closing).
    pub(crate) fn flush_history(&mut self) {
        let ready = self.team_board.history.recorder.take(0, true);
        if !ready.is_empty() {
            self.write_history(&ready);
        }
    }

    /// The hosting editor's part: a line per transaction in the host's log
    /// it hasn't recorded, by its author.
    pub(in crate::app) fn harvest_host_log(&mut self, now: u64) {
        let Some(hosting) = self.team.as_ref().and_then(|team| team.hosting()) else {
            self.team_board.history.seen = None;
            return;
        };
        let mut entries = Vec::new();
        let seen = {
            let session = hosting.lock();
            let host = session.host();
            let (epoch, after) = match self.team_board.history.seen {
                Some((epoch, seq)) if epoch == host.epoch() => (epoch, seq),
                _ => (host.epoch(), 0),
            };
            let roster = host.roster();
            let world = host.world();
            for logged in host.log().filter(|l| l.seq > after) {
                let who = roster
                    .iter()
                    .find(|c| c.member == logged.author)
                    .map_or_else(|| "Someone".to_owned(), |c| c.profile.name.clone());
                entries.extend(transaction_entries(
                    &who,
                    now,
                    &logged.label,
                    &logged.ops,
                    |id| world.full_name(id),
                    self.team_board.board(),
                ));
            }
            (epoch, host.seq())
        };
        self.team_board.history.seen = Some(seen);
        for entry in entries {
            self.team_board.history.recorder.note(entry);
        }
    }

    /// The history as read from the project, read again when it changed.
    fn load_history(&mut self) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            self.team_board.history.loaded = None;
            return;
        };
        let fresh = self
            .team_board
            .history
            .loaded
            .as_ref()
            .is_some_and(|(loaded, _)| *loaded == folder);
        if fresh && !self.team_board.history.stale {
            return;
        }
        self.team_board.history.stale = false;
        let history = History::load(&folder);
        self.team_board.history.loaded = Some((folder, history));
    }

    /// The People window: each person's changes by day.
    fn people_window(&mut self, ctx: &egui::Context) {
        self.load_history();
        self.load_board();
        let me = self.board_me();
        let mut window = std::mem::take(&mut self.team_board.history.window);
        let empty = History::default();
        let history = self
            .team_board
            .history
            .loaded
            .as_ref()
            .map_or(&empty, |(_, history)| history);
        let mut open = true;
        let actions = window.show(
            ctx,
            &mut open,
            history,
            self.team_board.board(),
            &me,
            time::now(),
        );
        self.team_board.history.window = window;
        if !open {
            self.team_board.panel.people = false;
        }
        for action in actions {
            match action {
                PeopleAction::SelectInstance(path) => {
                    self.apply_board_actions(vec![crate::team_board::BoardAction::SelectInstance(
                        path,
                    )]);
                }
                PeopleAction::OpenFile { path, line } => {
                    self.apply_board_actions(vec![crate::team_board::BoardAction::OpenFile {
                        path,
                        line,
                    }]);
                }
                PeopleAction::OpenItem(id) => {
                    if let Some(item) = self.team_board.board().and_then(|b| b.get(&id)).cloned() {
                        self.team_board.panel.open_item(&item);
                    }
                }
                PeopleAction::Refresh => self.team_board.history.stale = true,
            }
        }
    }
}

/// The lines for one transaction by `who`: its World changes as one line
/// (named by its label, linking what it touched), each board file as a
/// board line, and other files as a file line.
pub(crate) fn transaction_entries(
    who: &str,
    now: u64,
    label: &str,
    ops: &[Op],
    path_of: impl Fn(axcenity_core::InstanceId) -> Option<String>,
    board: Option<&axcenity_teamwork::Board>,
) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut instances = Vec::new();
    let mut files = Vec::new();
    for op in ops {
        let ids: Vec<axcenity_core::InstanceId> = match op {
            Op::CreateTree { nodes } => nodes.iter().map(|n| n.id).collect(),
            Op::Reparent { id, .. }
            | Op::SetProps { id, .. }
            | Op::SetAttribute { id, .. }
            | Op::Tag { id, .. } => vec![*id],
            Op::FileVersion { path, new, .. } => {
                if history::is_history_path(path) {
                    continue;
                }
                if let Some(id) = board::item_id(path) {
                    let title = board.and_then(|b| b.get(&id)).map(|i| i.title.clone());
                    let text = match (new, title) {
                        (None, Some(title)) => format!("Deleted “{title}”"),
                        (None, None) => "Deleted an item".to_owned(),
                        (Some(_), Some(title)) => format!("“{title}”: changed it"),
                        (Some(_), None) => "Changed an item".to_owned(),
                    };
                    entries.push(Entry::new(now, who, What::Board, &text).with_item(&id));
                } else {
                    files.push(path.clone());
                }
                continue;
            }
        };
        for id in ids {
            if let Some(path) = path_of(id) {
                instances.push(Link::Instance(path));
            }
        }
    }
    let label = if label.trim().is_empty() {
        "Edit"
    } else {
        label
    };
    if !instances.is_empty() {
        let mut entry = Entry::new(now, who, What::Edit, label).with_links(instances);
        entry.links.extend(
            files
                .drain(..)
                .map(|path| Link::File { path, line: None })
                .take(history::MAX_LINKS.saturating_sub(entry.links.len())),
        );
        entries.insert(0, entry);
    } else if !files.is_empty() {
        let text = match files.as_slice() {
            [one] => format!("Saved {one}"),
            many => format!("Saved {} files", many.len()),
        };
        let links = files
            .into_iter()
            .map(|path| Link::File { path, line: None });
        entries.insert(0, Entry::new(now, who, What::File, &text).with_links(links));
    }
    entries
}
