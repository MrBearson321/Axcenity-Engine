//! Each person's history (T4, `docs/research/teamwork.md` §6): every
//! change someone made to the project, as lines of JSON in
//! `team/history/<person>/<YYYY-MM>.<computer>.jsonl`.
//!
//! Each computer appends only to its own files (the computer's code is in
//! the name), so two computers never change the same file and Git never
//! has to merge history. In a Collaborate session the host records what
//! every member does; outside one, each editor records its own person.
//! [`Recorder`] folds a run of the same change (a drag sends dozens) into
//! one line with a count.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::item::{Item, Link, same_person, tidy};
use crate::time::Date;

/// The folder that holds everyone's history.
pub const HISTORY_FOLDER: &str = "team/history";

/// Seconds within which the same change by the same person folds into one
/// line.
pub const FOLD_SECONDS: u64 = 60;

/// Most details a line keeps (a save's sentences).
pub const MAX_DETAILS: usize = 50;

/// Most links a line keeps.
pub const MAX_LINKS: usize = 20;

/// What kind of change a line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum What {
    /// The World changed in a session (moved, made, deleted, set).
    Edit,
    /// A project file changed in a session (a script, an asset).
    File,
    /// The project was saved outside a session.
    Save,
    /// Something on the Team Board.
    Board,
}

impl What {
    pub const ALL: [What; 4] = [What::Edit, What::File, What::Save, What::Board];

    pub fn name(self) -> &'static str {
        match self {
            What::Edit => "World",
            What::File => "Files",
            What::Save => "Saves",
            What::Board => "Board",
        }
    }
}

/// One line of someone's history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Seconds since 1970, when it started.
    pub at: u64,
    /// The person, by their Collaborate name.
    pub who: String,
    pub what: What,
    /// "Move Pad", "Saved 3 changes", "Moved “Door opens” to Done".
    pub text: String,
    /// What it touched.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    /// The board item it's about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// Each change, for a save ("Moved Part “Block” in Workspace").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    /// How many changes the line stands for.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub count: u32,
}

fn one() -> u32 {
    1
}

fn is_one(count: &u32) -> bool {
    *count == 1
}

impl Entry {
    /// A line for one change; text is tidied, links and details capped.
    pub fn new(at: u64, who: &str, what: What, text: &str) -> Entry {
        Entry {
            at,
            who: tidy(who, 80),
            what,
            text: tidy(text, crate::item::MAX_TITLE).replace(['\n', '\t'], " "),
            links: Vec::new(),
            item: None,
            details: Vec::new(),
            count: 1,
        }
    }

    pub fn with_links(mut self, links: impl IntoIterator<Item = Link>) -> Entry {
        for link in links {
            if self.links.len() >= MAX_LINKS {
                break;
            }
            if !self.links.contains(&link) {
                self.links.push(link);
            }
        }
        self
    }

    pub fn with_item(mut self, id: &str) -> Entry {
        self.item = Some(id.to_owned());
        self
    }

    pub fn with_details(mut self, details: impl IntoIterator<Item = String>) -> Entry {
        self.details = details
            .into_iter()
            .take(MAX_DETAILS)
            .map(|d| tidy(&d, crate::item::MAX_TITLE))
            .collect();
        self
    }

    /// The same kind of change by the same person, with the same text.
    fn folds_with(&self, other: &Entry) -> bool {
        self.what == other.what
            && self.text == other.text
            && self.item == other.item
            && same_person(&self.who, &other.who)
    }
}

/// The line for `who`'s change to a board item (`before` is `None` for a
/// new one): what their newest event says, or that they commented or
/// changed it.
pub fn board_entry(before: Option<&Item>, after: &Item, who: &str, at: u64) -> Entry {
    let kind = after.kind.name().to_lowercase();
    let text = match before {
        None => format!("Created {kind} “{}”", after.title),
        Some(before) => {
            let newest = after
                .events
                .iter()
                .rev()
                .find(|e| same_person(&e.who, who) && !before.events.contains(e));
            if let Some(event) = newest {
                format!("“{}”: {}", after.title, event.action(after.kind))
            } else if after.comments.len() > before.comments.len() {
                format!("“{}”: commented", after.title)
            } else {
                format!("“{}”: changed it", after.title)
            }
        }
    };
    Entry::new(at, who, What::Board, &text).with_item(&after.id)
}

/// The line for `who` deleting a board item.
pub fn board_deleted(item: &Item, who: &str, at: u64) -> Entry {
    let kind = item.kind.name().to_lowercase();
    Entry::new(
        at,
        who,
        What::Board,
        &format!("Deleted {kind} “{}”", item.title),
    )
}

/// A name as a folder name: lower case letters and digits, other runs as
/// one `-` ("Paul J." is `paul-j`).
pub fn person_folder(person: &str) -> String {
    let mut out = String::new();
    for c in person.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out
        .trim_end_matches('-')
        .chars()
        .take(60)
        .collect::<String>();
    if out.is_empty() {
        "someone".to_owned()
    } else {
        out
    }
}

/// Where `computer` writes `person`'s lines for the month of `at`, relative
/// to the project.
pub fn file_for(person: &str, at: u64, computer: &str) -> String {
    let date = Date::of(at);
    format!(
        "{HISTORY_FOLDER}/{}/{:04}-{:02}.{}.jsonl",
        person_folder(person),
        date.year,
        date.month,
        person_folder(computer)
    )
}

/// True for a path inside the history folder.
pub fn is_history_path(path: &str) -> bool {
    path.strip_prefix(HISTORY_FOLDER)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Appends `entries` to their files under `root`, written by `computer`.
pub fn append(root: &Path, computer: &str, entries: &[Entry]) -> io::Result<()> {
    let mut by_file: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
    for entry in entries {
        by_file
            .entry(file_for(&entry.who, entry.at, computer))
            .or_default()
            .push(entry);
    }
    for (path, entries) in by_file {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut text = String::new();
        for entry in entries {
            text.push_str(&serde_json::to_string(entry).map_err(io::Error::other)?);
            text.push('\n');
        }
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(text.as_bytes())?;
    }
    Ok(())
}

/// Everyone's history as read from a project.
#[derive(Clone, Debug, Default)]
pub struct History {
    /// Oldest first.
    entries: Vec<Entry>,
    /// Lines that didn't read, by file.
    pub problems: Vec<String>,
}

impl History {
    /// Reads every history file under `root`. A damaged line is skipped
    /// (and counted in `problems`); the same line in two files (a copied
    /// file) is kept once.
    pub fn load(root: &Path) -> History {
        let mut history = History::default();
        let mut seen = std::collections::HashSet::new();
        let mut files = Vec::new();
        collect(&root.join(HISTORY_FOLDER), &mut files);
        files.sort();
        for file in files {
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };
            let mut bad = 0;
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                match serde_json::from_str::<Entry>(line) {
                    // The same line in two files (a copied file) counts once.
                    Ok(entry) if seen.insert(line.trim().to_owned()) => history.entries.push(entry),
                    Ok(_) => {}
                    Err(_) => bad += 1,
                }
            }
            if bad > 0 {
                let name = file
                    .strip_prefix(root)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                history
                    .problems
                    .push(format!("{name}: {bad} lines didn't read"));
            }
        }
        // Stable: lines of the same second keep the order they were written.
        history.entries.sort_by_key(|e| e.at);
        history
    }

    /// Every line, oldest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Everyone with a line, once each, the most recently active first.
    pub fn people(&self) -> Vec<String> {
        let mut people: Vec<(String, u64)> = Vec::new();
        for entry in &self.entries {
            match people.iter_mut().find(|(p, _)| same_person(p, &entry.who)) {
                Some((_, last)) => *last = (*last).max(entry.at),
                None => people.push((entry.who.clone(), entry.at)),
            }
        }
        people.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        people.into_iter().map(|(p, _)| p).collect()
    }

    /// `person`'s lines (of `what`, when given), newest first.
    pub fn of(&self, person: &str, what: Option<What>) -> Vec<&Entry> {
        self.entries
            .iter()
            .rev()
            .filter(|e| same_person(&e.who, person) && what.is_none_or(|w| e.what == w))
            .collect()
    }

    /// `person`'s lines grouped by day (UTC), newest day first.
    pub fn days(&self, person: &str, what: Option<What>) -> Vec<(Date, Vec<&Entry>)> {
        let mut days: Vec<(Date, Vec<&Entry>)> = Vec::new();
        for entry in self.of(person, what) {
            let day = Date::of(entry.at);
            match days.last_mut() {
                Some((last, entries)) if *last == day => entries.push(entry),
                _ => days.push((day, vec![entry])),
            }
        }
        days
    }

    /// How many changes `person` made since `since` (seconds since 1970).
    pub fn changes_since(&self, person: &str, since: u64) -> u32 {
        self.of(person, None)
            .into_iter()
            .take_while(|e| e.at >= since)
            .map(|e| e.count)
            .sum()
    }
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            files.push(path);
        }
    }
}

/// Lines waiting to be written, so a run of the same change becomes one
/// line: a line is written once nothing has folded into it for
/// [`FOLD_SECONDS`].
#[derive(Debug, Default)]
pub struct Recorder {
    /// Each waiting line and when something last folded into it.
    pending: Vec<(Entry, u64)>,
}

impl Recorder {
    /// Adds `entry`, folding it into a waiting line of the same change.
    pub fn note(&mut self, entry: Entry) {
        let at = entry.at;
        if let Some((line, last)) = self.pending.iter_mut().find(|(line, last)| {
            line.folds_with(&entry) && at.saturating_sub(*last) <= FOLD_SECONDS
        }) {
            line.count = line.count.saturating_add(entry.count);
            *last = (*last).max(at);
            for link in entry.links {
                if line.links.len() < MAX_LINKS && !line.links.contains(&link) {
                    line.links.push(link);
                }
            }
            return;
        }
        self.pending.push((entry, at));
    }

    /// The lines ready to write at `now` (every one with `all`), oldest
    /// first.
    pub fn take(&mut self, now: u64, all: bool) -> Vec<Entry> {
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|(_, last)| all || now.saturating_sub(*last) > FOLD_SECONDS);
        self.pending = waiting;
        let mut ready: Vec<Entry> = ready.into_iter().map(|(entry, _)| entry).collect();
        ready.sort_by_key(|e| e.at);
        ready
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}
