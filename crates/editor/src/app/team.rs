//! Team Create in the editor (TC2 of `docs/research/team-create.md`).
//!
//! While in a session, the edit World keeps an edit journal, and each frame
//! [`EditorApp::team_frame`] sends what changed since the last frame as one
//! transaction named after the newest undo step (`collab::Client::capture`),
//! then applies what the others did. So every tool, plugin and AI client
//! syncs without code of its own. Nothing moves while a gesture is open (a
//! drag, typing a name): the frame after it ends sends it and catches up.
//!
//! Undo in a session only changes what's still as the step left it
//! (`History::set_shared`), and its result syncs like any edit. Steps that
//! swap the whole World (Console commands, prefab Apply) can't be undone in
//! a session yet, and terrain, the string table and project files aren't
//! shared yet (TC5): the Output says so.
//!
//! What SplineMeshes and SplineScatters make isn't shared: each copy makes
//! its own ([`World::unrecorded`] in `generate_frame`).
//!
//! The members and the host meet in a [`Hub`] in this process. TC3 puts a
//! network between them; the editor side stays the same.

// Only tests start a session until TC3 (network) and TC4 (menus) do.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use axcenity_collab::{Client, Host, MemberId, Notice, Role, ToClient, ToHost};
use axcenity_core::spline::is_generator;

use super::*;

/// Most rounds of messages one frame passes back and forth.
const ROUNDS_PER_FRAME: usize = 8;

/// A session's host and each member's waiting messages, in this process.
pub(crate) struct Hub {
    host: Host,
    inboxes: HashMap<MemberId, VecDeque<ToClient>>,
    next_member: u32,
}

pub(crate) type SharedHub = Arc<Mutex<Hub>>;

impl Hub {
    /// A session for `world`, as the hosting editor has it, without what
    /// SplineMeshes and SplineScatters made (every copy makes its own).
    pub(crate) fn new(mut world: World) -> SharedHub {
        world.stop_recording_edits();
        let generated: Vec<InstanceId> = world
            .descendants(world.root())
            .into_iter()
            .filter(|&id| world.get(id).is_some_and(|i| is_generator(i.class())))
            .flat_map(|id| {
                world
                    .get(id)
                    .map(|i| i.children().to_vec())
                    .unwrap_or_default()
            })
            .collect();
        for id in generated {
            let _ = world.destroy(id);
        }
        Arc::new(Mutex::new(Hub {
            host: Host::new(world),
            inboxes: HashMap::new(),
            next_member: 1,
        }))
    }

    /// Lets a new member in with `role`.
    pub(crate) fn add_member(&mut self, role: Role) -> MemberId {
        let member = MemberId(self.next_member);
        self.next_member += 1;
        self.host.add_member(member, role);
        self.inboxes.insert(member, VecDeque::new());
        member
    }

    pub(crate) fn host(&self) -> &Host {
        &self.host
    }

    fn send(&mut self, from: MemberId, messages: Vec<ToHost>) {
        for message in messages {
            for out in self.host.receive(from, message) {
                self.inboxes
                    .entry(out.to)
                    .or_default()
                    .push_back(out.message);
            }
        }
    }

    fn take(&mut self, member: MemberId) -> Vec<ToClient> {
        self.inboxes
            .get_mut(&member)
            .map(|inbox| inbox.drain(..).collect())
            .unwrap_or_default()
    }
}

/// This editor's place in a session.
pub(crate) struct Team {
    client: Client,
    hub: SharedHub,
}

impl Team {
    pub(crate) fn client(&self) -> &Client {
        &self.client
    }
}

impl EditorApp {
    /// Starts a session for the open world, with this editor as its owner,
    /// and returns the hub others join.
    pub(crate) fn host_team_session(&mut self) -> SharedHub {
        let hub = Hub::new(self.world.clone());
        let member = hub.lock().expect("hub").add_member(Role::Owner);
        self.join_team_session(hub.clone(), member);
        hub
    }

    /// Joins a session as `member` (one the hub let in). The edit World is
    /// replaced with the session's.
    pub(crate) fn join_team_session(&mut self, hub: SharedHub, member: MemberId) {
        let mut client = Client::new(member);
        client.connect();
        self.team = Some(Team { client, hub });
        self.team_frame();
    }

    pub(crate) fn in_team_session(&self) -> bool {
        self.team.is_some()
    }

    /// Sends this frame's edits and applies the others', unless a gesture
    /// is open. Runs each frame.
    pub(crate) fn team_frame(&mut self) {
        if self.team.is_none()
            || self.history.is_open()
            || self.viewport.is_dragging()
            || self.ui_designer.is_dragging()
        {
            return;
        }
        let mut changed = false;
        for _ in 0..ROUNDS_PER_FRAME {
            let label = self.history.undo_label().unwrap_or("Edit").to_owned();
            let team = self.team.as_mut().expect("checked above");
            let sent = team.client.capture(&mut self.world, label);
            let outbox = team.client.take_outbox();
            let busy = !outbox.is_empty();
            let inbox = {
                let mut hub = team.hub.lock().expect("hub");
                hub.send(team.client.member(), outbox);
                hub.take(team.client.member())
            };
            let received = !inbox.is_empty();
            for message in inbox {
                team.client.receive(&mut self.world, message);
            }
            if sent.is_some() {
                self.warn_unshared_step();
            }
            self.team_notices();
            changed |= received;
            if !busy && !received {
                break;
            }
        }
        if changed {
            self.forget_selection_outside_tree();
            self.generate_frame();
        }
    }

    fn team_notices(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        for notice in team.client.take_notices() {
            match notice {
                Notice::Joined => {
                    // A fresh copy of the session's World: older steps and
                    // what generators made name what's gone.
                    self.history = History::new();
                    self.history.set_shared(true);
                    self.generators = Default::default();
                    self.selection.clear();
                    self.selected = None;
                }
                Notice::Changed { label, notes, .. } => self.output.warning(format!(
                    "Team Create: \"{label}\" didn't apply in full: someone deleted what it \
                     changed ({} change(s) dropped or moved)",
                    notes.len()
                )),
                Notice::Refused { label, reason, .. } => self
                    .output
                    .warning(format!("Team Create: \"{label}\" was refused: {reason}")),
                Notice::Resynced => self.output.warning(
                    "Team Create: this copy disagreed with the session and was reloaded \
                     (your unsent edits were kept)",
                ),
            }
        }
    }

    /// The newest step changed something Team Create doesn't share yet.
    fn warn_unshared_step(&mut self) {
        if let Some(what) = self.history.top_unshared() {
            self.output.warning(format!(
                "Team Create doesn't share {what} yet: others won't see this part of \"{}\"",
                self.history.undo_label().unwrap_or("Edit")
            ));
        }
    }

    /// Undo or redo left some changes alone because someone else changed
    /// them since.
    pub(super) fn report_skipped(&mut self, skipped: usize) {
        if skipped > 0 {
            self.output.warning(format!(
                "Some changes weren't undone or redone: someone else changed {skipped} of them since"
            ));
        }
    }

    /// In a session, refuses an undo or redo that would swap the whole
    /// World, which can't be sent yet. True if refused.
    pub(super) fn team_refuses_world_swap(&mut self, redo: bool) -> bool {
        if self.team.is_none() || !self.history.swaps_world(redo) {
            return false;
        }
        self.output.warning(
            "Team Create: this step changed the whole world (a Console command or a prefab \
             Apply) and can't be undone or redone during a session yet",
        );
        true
    }
}
