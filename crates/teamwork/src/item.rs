//! One thing on the board: a task (later also a bug or a goal), with who
//! it's assigned to, its status, comments and what happened to it.
//!
//! Each item is its own file, `team/board/<id>.json`, written as pretty
//! JSON with a stable field order, so Git merges two people's changes to
//! different fields of it and a diff reads well.

use serde::{Deserialize, Serialize};

use crate::time::Date;

/// What kind of item it is.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Something to do.
    #[default]
    Task,
    /// Something that's wrong (T2), with [`BugInfo`].
    Bug,
    /// A milestone (T3): tasks and bugs belong to it through
    /// [`Item::goal`], its progress is theirs, its due date is the target.
    Goal,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Task, Kind::Bug, Kind::Goal];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Task => "Task",
            Kind::Bug => "Bug",
            Kind::Goal => "Goal",
        }
    }

    /// "tasks", "bugs", "goals".
    pub fn plural(self) -> &'static str {
        match self {
            Kind::Task => "tasks",
            Kind::Bug => "bugs",
            Kind::Goal => "goals",
        }
    }
}

/// Where an item stands. Each kind names them its own way
/// ([`Status::name`]).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Todo,
    InProgress,
    InReview,
    Done,
    /// Won't be done.
    Closed,
}

impl Status {
    /// In board order.
    pub const ALL: [Status; 5] = [
        Status::Todo,
        Status::InProgress,
        Status::InReview,
        Status::Done,
        Status::Closed,
    ];

    pub fn name(self, kind: Kind) -> &'static str {
        match (kind, self) {
            (Kind::Task, Status::Todo) => "To do",
            (Kind::Task, Status::InProgress) => "In progress",
            (Kind::Task, Status::InReview) => "In review",
            (Kind::Task, Status::Done) => "Done",
            (Kind::Task, Status::Closed) => "Won't do",
            (Kind::Bug, Status::Todo) => "Open",
            (Kind::Bug, Status::InProgress) => "In progress",
            (Kind::Bug, Status::InReview) => "Fix in review",
            (Kind::Bug, Status::Done) => "Fixed",
            (Kind::Bug, Status::Closed) => "Won't fix",
            (Kind::Goal, Status::Todo) => "Planned",
            (Kind::Goal, Status::InProgress) => "Under way",
            (Kind::Goal, Status::InReview) => "Wrapping up",
            (Kind::Goal, Status::Done) => "Reached",
            (Kind::Goal, Status::Closed) => "Dropped",
        }
    }

    /// Done or closed: nothing more to do.
    pub fn is_finished(self) -> bool {
        matches!(self, Status::Done | Status::Closed)
    }
}

/// How soon it matters.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Urgent,
    High,
    #[default]
    Normal,
    Low,
}

impl Priority {
    /// Most urgent first.
    pub const ALL: [Priority; 4] = [
        Priority::Urgent,
        Priority::High,
        Priority::Normal,
        Priority::Low,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Priority::Urgent => "Urgent",
            Priority::High => "High",
            Priority::Normal => "Normal",
            Priority::Low => "Low",
        }
    }
}

/// How bad a bug is.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Nothing else can go on (a crash, lost work).
    Blocker,
    /// Something important doesn't work.
    Major,
    #[default]
    Minor,
    /// Cosmetic.
    Trivial,
}

impl Severity {
    /// Worst first.
    pub const ALL: [Severity; 4] = [
        Severity::Blocker,
        Severity::Major,
        Severity::Minor,
        Severity::Trivial,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Severity::Blocker => "Blocker",
            Severity::Major => "Major",
            Severity::Minor => "Minor",
            Severity::Trivial => "Trivial",
        }
    }
}

/// What a bug report says besides an item's own fields (T2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BugInfo {
    #[serde(default)]
    pub severity: Severity,
    /// How to make it happen.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub steps: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expected: String,
    /// What happens instead (an error message, say).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub actual: String,
    /// Where it was seen: the engine version and system ("0.2.0-alpha.1 on
    /// Windows").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub found_in: String,
    /// What fixed it: a checkpoint, a commit, a version.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fixed_in: String,
}

/// Something in the project an item is about.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    /// An Instance, by its path from `game` (`Workspace.Door`).
    Instance(String),
    /// A file in the project, and a line in it if it's a script.
    File {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
    },
}

impl Link {
    /// How the item shows it.
    pub fn label(&self) -> String {
        match self {
            Link::Instance(path) => path.clone(),
            Link::File {
                path,
                line: Some(line),
            } => format!("{path}:{line}"),
            Link::File { path, line: None } => path.clone(),
        }
    }
}

/// One line of an item's checklist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

/// Someone's comment on an item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub author: String,
    /// Seconds since 1970.
    pub at: u64,
    pub text: String,
}

/// What changed about an item, for its activity list.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "what")]
pub enum Change {
    Created,
    Status {
        from: Status,
        to: Status,
    },
    Assigned {
        person: String,
    },
    Unassigned {
        person: String,
    },
    Priority {
        to: Priority,
    },
    Due {
        to: Option<Date>,
    },
    Renamed {
        from: String,
    },
    Severity {
        to: Severity,
    },
    /// Put under a goal (by its title then), or taken out of one.
    Goal {
        to: Option<String>,
    },
}

/// A change, who made it and when.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Event {
    /// Seconds since 1970.
    pub at: u64,
    pub who: String,
    #[serde(flatten)]
    pub change: Change,
}

impl Event {
    /// "Paul moved it to Done".
    pub fn describe(&self, kind: Kind) -> String {
        format!("{} {}", self.who, self.action(kind))
    }

    /// What was done, without who: "moved it to Done".
    pub fn action(&self, kind: Kind) -> String {
        let who = &self.who;
        match &self.change {
            Change::Created => "created it".to_owned(),
            Change::Status { to, .. } => format!("moved it to {}", to.name(kind)),
            Change::Assigned { person } if person == who => "took it".to_owned(),
            Change::Assigned { person } => format!("assigned it to {person}"),
            Change::Unassigned { person } if person == who => "let it go".to_owned(),
            Change::Unassigned { person } => format!("unassigned {person}"),
            Change::Priority { to } => format!("set the priority to {}", to.name()),
            Change::Due { to: Some(date) } => format!("set it due {date}"),
            Change::Due { to: None } => "took the due date off".to_owned(),
            Change::Renamed { from } => format!("renamed it from “{from}”"),
            Change::Severity { to } => format!("set the severity to {}", to.name()),
            Change::Goal { to: Some(goal) } => format!("put it under “{goal}”"),
            Change::Goal { to: None } => "took it out of its goal".to_owned(),
        }
    }
}

/// One thing on the board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub priority: Priority,
    /// People, by their Collaborate names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// When it's due; a goal's target date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<Date>,
    /// The goal (its ID) this task or bug counts toward (T3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checklist: Vec<Check>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    /// A bug's report (T2); `None` for other kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bug: Option<BugInfo>,
    pub created_by: String,
    pub created_at: u64,
    pub updated_by: String,
    pub updated_at: u64,
}

/// Longest title kept.
pub const MAX_TITLE: usize = 200;
/// Longest description or comment kept.
pub const MAX_TEXT: usize = 20_000;

/// `text` with control characters other than line breaks and tabs left
/// out, trimmed, and cut to `max` characters.
pub fn tidy(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

impl Item {
    /// A new item made by `who` at `now`.
    pub fn new(kind: Kind, title: &str, who: &str, now: u64) -> Item {
        Item {
            id: crate::new_id(),
            kind,
            title: tidy(title, MAX_TITLE).replace(['\n', '\t'], " "),
            description: String::new(),
            status: Status::Todo,
            priority: Priority::Normal,
            assignees: Vec::new(),
            labels: Vec::new(),
            due: None,
            goal: None,
            links: Vec::new(),
            checklist: Vec::new(),
            comments: Vec::new(),
            events: vec![Event {
                at: now,
                who: who.to_owned(),
                change: Change::Created,
            }],
            bug: (kind == Kind::Bug).then(BugInfo::default),
            created_by: who.to_owned(),
            created_at: now,
            updated_by: who.to_owned(),
            updated_at: now,
        }
    }

    /// A bug report from an error message: its first line is the title
    /// (cut short), the whole message what happened, and `found_in` where.
    pub fn bug_from_error(message: &str, found_in: &str, who: &str, now: u64) -> Item {
        let first = message.lines().next().unwrap_or("").trim();
        let title = if first.chars().count() > 90 {
            format!("{}…", first.chars().take(89).collect::<String>())
        } else {
            first.to_owned()
        };
        let mut item = Item::new(Kind::Bug, &title, who, now);
        let bug = item.bug.get_or_insert_with(BugInfo::default);
        bug.actual = tidy(message, MAX_TEXT);
        bug.found_in = tidy(found_in, MAX_TITLE);
        item
    }

    /// The bug report, made if this item had none.
    pub fn bug_mut(&mut self) -> &mut BugInfo {
        self.bug.get_or_insert_with(BugInfo::default)
    }

    pub fn set_severity(&mut self, severity: Severity, who: &str, now: u64) {
        if self.bug.as_ref().map(|b| b.severity) != Some(severity) {
            self.bug_mut().severity = severity;
            self.record(who, now, Change::Severity { to: severity });
        }
    }

    /// A bug's severity (Minor for other kinds).
    pub fn severity(&self) -> Severity {
        self.bug.as_ref().map(|b| b.severity).unwrap_or_default()
    }

    /// "#4f2k9": how people point at it in chat.
    pub fn short_id(&self) -> String {
        format!("#{}", self.id.chars().take(5).collect::<String>())
    }

    /// Says `who` changed it at `now`.
    pub fn touch(&mut self, who: &str, now: u64) {
        self.updated_by = who.to_owned();
        self.updated_at = now.max(self.updated_at);
    }

    fn record(&mut self, who: &str, now: u64, change: Change) {
        self.events.push(Event {
            at: now,
            who: who.to_owned(),
            change,
        });
        self.touch(who, now);
    }

    pub fn set_status(&mut self, status: Status, who: &str, now: u64) {
        if self.status != status {
            let from = std::mem::replace(&mut self.status, status);
            self.record(who, now, Change::Status { from, to: status });
        }
    }

    pub fn set_priority(&mut self, priority: Priority, who: &str, now: u64) {
        if self.priority != priority {
            self.priority = priority;
            self.record(who, now, Change::Priority { to: priority });
        }
    }

    /// Puts it under `goal` (`None` takes it out). A goal can't belong to
    /// a goal, nor anything to itself.
    pub fn set_goal(&mut self, goal: Option<&Item>, who: &str, now: u64) {
        if self.kind == Kind::Goal || goal.is_some_and(|g| g.kind != Kind::Goal || g.id == self.id)
        {
            return;
        }
        let id = goal.map(|g| g.id.clone());
        if self.goal != id {
            self.goal = id;
            let to = goal.map(|g| g.title.clone());
            self.record(who, now, Change::Goal { to });
        }
    }

    pub fn set_due(&mut self, due: Option<Date>, who: &str, now: u64) {
        if self.due != due {
            self.due = due;
            self.record(who, now, Change::Due { to: due });
        }
    }

    pub fn rename(&mut self, title: &str, who: &str, now: u64) {
        let title = tidy(title, MAX_TITLE).replace(['\n', '\t'], " ");
        if !title.is_empty() && title != self.title {
            let from = std::mem::replace(&mut self.title, title);
            self.record(who, now, Change::Renamed { from });
        }
    }

    pub fn is_assigned(&self, person: &str) -> bool {
        self.assignees.iter().any(|p| same_person(p, person))
    }

    pub fn assign(&mut self, person: &str, who: &str, now: u64) {
        let person = tidy(person, 80).replace(['\n', '\t'], " ");
        if !person.is_empty() && !self.is_assigned(&person) {
            self.assignees.push(person.clone());
            self.record(who, now, Change::Assigned { person });
        }
    }

    pub fn unassign(&mut self, person: &str, who: &str, now: u64) {
        let before = self.assignees.len();
        self.assignees.retain(|p| !same_person(p, person));
        if self.assignees.len() != before {
            self.record(
                who,
                now,
                Change::Unassigned {
                    person: person.to_owned(),
                },
            );
        }
    }

    pub fn comment(&mut self, text: &str, who: &str, now: u64) -> bool {
        let text = tidy(text, MAX_TEXT);
        if text.is_empty() {
            return false;
        }
        self.comments.push(Comment {
            id: crate::new_id(),
            author: who.to_owned(),
            at: now,
            text,
        });
        self.touch(who, now);
        true
    }

    pub fn add_check(&mut self, text: &str, who: &str, now: u64) {
        let text = tidy(text, MAX_TITLE).replace(['\n', '\t'], " ");
        if !text.is_empty() {
            self.checklist.push(Check {
                id: crate::new_id(),
                text,
                done: false,
            });
            self.touch(who, now);
        }
    }

    pub fn add_link(&mut self, link: Link, who: &str, now: u64) {
        if !self.links.contains(&link) {
            self.links.push(link);
            self.touch(who, now);
        }
    }

    /// The checklist's ticked and total lines.
    pub fn checked(&self) -> (usize, usize) {
        let done = self.checklist.iter().filter(|c| c.done).count();
        (done, self.checklist.len())
    }

    /// Due before `today` and not finished.
    pub fn is_overdue(&self, today: Date) -> bool {
        !self.status.is_finished() && self.due.is_some_and(|due| due < today)
    }
}

/// Names are people: the same name in another case is the same person.
pub fn same_person(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Puts `copy` (another version of the same item: a losing save in a
/// session, or the repository's version after Keep both) into `item`.
/// Comments, checklist lines, links and events from both are kept; the
/// other fields come from whichever was changed last.
pub fn merge(item: &mut Item, copy: Item) {
    let copy_newer = copy.updated_at > item.updated_at;
    let mut comments = std::mem::take(&mut item.comments);
    for comment in &copy.comments {
        if !comments.iter().any(|c| c.id == comment.id) {
            comments.push(comment.clone());
        }
    }
    comments.sort_by(|a, b| (a.at, &a.id).cmp(&(b.at, &b.id)));
    let mut events = std::mem::take(&mut item.events);
    for event in &copy.events {
        if !events.contains(event) {
            events.push(event.clone());
        }
    }
    events.sort();
    let mut links = std::mem::take(&mut item.links);
    for link in &copy.links {
        if !links.contains(link) {
            links.push(link.clone());
        }
    }
    let (newer_checks, older_checks) = if copy_newer {
        (copy.checklist.clone(), std::mem::take(&mut item.checklist))
    } else {
        (std::mem::take(&mut item.checklist), copy.checklist.clone())
    };
    let mut checklist = newer_checks;
    for check in older_checks {
        if !checklist.iter().any(|c| c.id == check.id) {
            checklist.push(check);
        }
    }
    let created = (item.created_at, item.created_by.clone());
    if copy_newer {
        *item = copy;
    }
    item.comments = comments;
    item.events = events;
    item.links = links;
    item.checklist = checklist;
    (item.created_at, item.created_by) = created;
}
