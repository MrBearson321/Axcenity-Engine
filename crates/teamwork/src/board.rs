//! The board: every item in `team/board/`, read from and written to the
//! project folder.
//!
//! The files are the board. Nothing else keeps it, so a Collaborate
//! session's mirror carries a changed item to everyone like any other
//! saved file, and the project's repository keeps its history. When two
//! people change the same item at once, the session keeps the losing save
//! beside it as `<id>.conflict-<who>.json` and the repository's Keep both
//! as `<id>.remote-<commit>.json`; [`Board::load`] merges such copies into
//! the item ([`crate::item::merge`]) and removes them.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::item::{Item, Kind, Status, same_person};
use crate::time::Date;
use crate::{BOARD_FOLDER, is_id};

/// Every item on the board.
#[derive(Clone, Debug, Default)]
pub struct Board {
    items: BTreeMap<String, Item>,
    /// Files that couldn't be read, and why.
    pub problems: Vec<String>,
}

/// How [`Board::load`] treats the copies two people's changes leave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Copies {
    /// Merge each into its item and remove it.
    Merge,
    /// Leave them be (the repository is still deciding a conflict, or
    /// another computer in the session will merge them).
    Leave,
}

/// What [`Board::load`] did besides reading.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Loaded {
    /// Items whose copies were merged in (and written).
    pub merged: Vec<String>,
}

/// The item a file in `team/board/` holds or is a copy of, if any: `<id>`
/// for `<id>.json`, and with `true` for `<id>.<anything>.json`.
fn item_of(name: &str) -> Option<(&str, bool)> {
    let stem = name.strip_suffix(".json")?;
    match stem.split_once('.') {
        None => is_id(stem).then_some((stem, false)),
        Some((id, rest)) => (is_id(id) && !rest.is_empty()).then_some((id, true)),
    }
}

/// `team/board/<id>.json`.
pub fn item_path(id: &str) -> String {
    format!("{BOARD_FOLDER}/{id}.json")
}

/// True for a path the board reads (an item or a copy of one).
pub fn is_board_path(path: &str) -> bool {
    path.strip_prefix(BOARD_FOLDER)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|name| !name.contains('/') && item_of(name).is_some())
}

fn read_item(path: &Path) -> Result<Item, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Writes `item` to its file through a temporary file, so a half-written
/// item is never read (and the session's mirror never shares one).
fn write_item(root: &Path, item: &Item) -> io::Result<()> {
    let path = root.join(item_path(&item.id));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(item).map_err(io::Error::other)?;
    text.push('\n');
    if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, &path)
}

impl Board {
    /// Reads the board in the project folder `root` (empty when there's
    /// none yet).
    pub fn load(root: &Path, copies: Copies) -> (Board, Loaded) {
        let mut board = Board::default();
        let mut loaded = Loaded::default();
        let folder = root.join(BOARD_FOLDER);
        let Ok(entries) = std::fs::read_dir(&folder) else {
            return (board, loaded);
        };
        let mut copy_files: Vec<(String, PathBuf)> = Vec::new();
        let mut names: Vec<(String, PathBuf)> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect();
        names.sort();
        for (name, path) in names {
            match item_of(&name) {
                Some((id, true)) => copy_files.push((id.to_owned(), path)),
                Some((id, false)) => match read_item(&path) {
                    Ok(item) if item.id == id => {
                        board.items.insert(id.to_owned(), item);
                    }
                    Ok(_) => board
                        .problems
                        .push(format!("{BOARD_FOLDER}/{name}: its id isn't its name")),
                    Err(error) => board
                        .problems
                        .push(format!("{BOARD_FOLDER}/{name}: {error}")),
                },
                None => {}
            }
        }
        if copies == Copies::Merge {
            for (id, path) in copy_files {
                let Ok(copy) = read_item(&path) else {
                    continue;
                };
                if copy.id != id {
                    continue;
                }
                match board.items.get_mut(&id) {
                    Some(item) => crate::item::merge(item, copy),
                    // The item itself was deleted, or never arrived: the
                    // copy is all there is.
                    None => {
                        board.items.insert(id.clone(), copy);
                    }
                }
                let item = &board.items[&id];
                if write_item(root, item).is_ok() && std::fs::remove_file(&path).is_ok() {
                    loaded.merged.push(id);
                }
            }
            loaded.merged.sort();
            loaded.merged.dedup();
        }
        (board, loaded)
    }

    pub fn get(&self, id: &str) -> Option<&Item> {
        self.items.get(id)
    }

    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.items.values()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Writes `item` (new or changed) to the board in `root`.
    pub fn save(&mut self, root: &Path, item: Item) -> io::Result<()> {
        write_item(root, &item)?;
        self.items.insert(item.id.clone(), item);
        Ok(())
    }

    /// Deletes item `id` and its file.
    pub fn delete(&mut self, root: &Path, id: &str) -> io::Result<()> {
        if !is_id(id) {
            return Ok(());
        }
        match std::fs::remove_file(root.join(item_path(id))) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.items.remove(id);
        Ok(())
    }

    /// The items `filter` lets through, in board order: status, then
    /// priority, then due date (soonest first, none last), then newest.
    pub fn filtered<'a>(&'a self, filter: &Filter) -> Vec<&'a Item> {
        let mut items: Vec<&Item> = self.items.values().filter(|i| filter.lets(i)).collect();
        items.sort_by(|a, b| {
            let due = |i: &Item| i.due.map_or(i64::MAX, Date::days);
            (
                a.status,
                a.priority,
                due(a),
                std::cmp::Reverse(a.created_at),
                &a.id,
            )
                .cmp(&(
                    b.status,
                    b.priority,
                    due(b),
                    std::cmp::Reverse(b.created_at),
                    &b.id,
                ))
        });
        items
    }

    /// Everyone the board names (assignees, authors, commenters) and
    /// `others` (the session's people, this computer's name), once each,
    /// sorted.
    pub fn people(&self, others: &[String]) -> Vec<String> {
        let mut people: Vec<String> = Vec::new();
        let mut add = |name: &str| {
            let name = name.trim();
            if !name.is_empty() && !people.iter().any(|p| same_person(p, name)) {
                people.push(name.to_owned());
            }
        };
        for name in others {
            add(name);
        }
        for item in self.items.values() {
            add(&item.created_by);
            for person in &item.assignees {
                add(person);
            }
            for comment in &item.comments {
                add(&comment.author);
            }
        }
        people.sort_by_key(|p| p.to_lowercase());
        people
    }

    /// Every label in use, sorted.
    pub fn labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = self
            .items
            .values()
            .flat_map(|i| i.labels.iter().cloned())
            .collect();
        labels.sort_by_key(|l| l.to_lowercase());
        labels.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        labels
    }

    /// How many unfinished items are assigned to `person`, and how many of
    /// those are overdue on `today`.
    pub fn open_for(&self, person: &str, today: Date) -> (usize, usize) {
        let mine = self
            .items
            .values()
            .filter(|i| !i.status.is_finished() && i.is_assigned(person));
        mine.fold((0, 0), |(open, late), i| {
            (open + 1, late + usize::from(i.is_overdue(today)))
        })
    }
}

/// Whose items a [`Filter`] shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Whose {
    #[default]
    Everyone,
    /// Assigned to this person.
    Person(String),
    Unassigned,
}

/// Which items the board shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub whose: Whose,
    /// Only these kinds (all when empty).
    pub kinds: Vec<Kind>,
    /// Only items with this label.
    pub label: Option<String>,
    /// Words that must all be in the title, description, labels or short ID.
    pub text: String,
    /// Show done and closed items too.
    pub finished: bool,
}

impl Filter {
    pub fn lets(&self, item: &Item) -> bool {
        let whose = match &self.whose {
            Whose::Everyone => true,
            Whose::Person(person) => item.is_assigned(person),
            Whose::Unassigned => item.assignees.is_empty(),
        };
        let kind = self.kinds.is_empty() || self.kinds.contains(&item.kind);
        let label = self
            .label
            .as_ref()
            .is_none_or(|label| item.labels.iter().any(|l| l.eq_ignore_ascii_case(label)));
        let finished = self.finished || !item.status.is_finished();
        whose && kind && label && finished && self.matches_text(item)
    }

    fn matches_text(&self, item: &Item) -> bool {
        let words: Vec<String> = self
            .text
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        if words.is_empty() {
            return true;
        }
        let haystack = format!(
            "{} {} {} {}",
            item.title,
            item.description,
            item.labels.join(" "),
            item.short_id()
        )
        .to_lowercase();
        words.iter().all(|w| haystack.contains(w.as_str()))
    }
}

/// The statuses that get a column on the board, in order (Won't do
/// shares Done's column).
pub fn columns() -> [Status; 4] {
    [
        Status::Todo,
        Status::InProgress,
        Status::InReview,
        Status::Done,
    ]
}
