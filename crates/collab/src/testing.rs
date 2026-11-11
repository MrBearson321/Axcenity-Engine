//! A simulated network for the tests: one host, several clients, a queue
//! each way per client. Each queue keeps its order (as TCP does), but the
//! test decides which queue moves next, so messages on different
//! connections interleave in any order. Disconnecting loses whatever is in
//! flight on that connection.

use std::collections::VecDeque;
use std::fmt::Write;

use axcenity_core::{ClassName, InstanceId, Value, World};

use crate::apply::{Doc, Refusal};
use crate::client::Client;
use crate::host::{Host, Outgoing};
use crate::message::{MemberId, ToClient, ToHost};
use crate::op::{NodeSpec, Op};
use crate::role::Role;

pub(crate) struct Net {
    pub host: Host,
    pub clients: Vec<Client>,
    up: Vec<VecDeque<ToHost>>,
    down: Vec<VecDeque<ToClient>>,
    connected: Vec<bool>,
    /// Joins from a snapshot and catch-ups from the log, counted so the
    /// convergence test can check it exercised both.
    pub snapshots: usize,
    pub catch_ups: usize,
}

pub(crate) fn member(i: usize) -> MemberId {
    MemberId(i as u32 + 1)
}

impl Net {
    /// A host for `world` and one client per role, all connected and
    /// joined.
    pub fn new(world: World, roles: &[Role]) -> Net {
        Net::with_host(Host::new(world), roles)
    }

    pub fn with_host(mut host: Host, roles: &[Role]) -> Net {
        for (i, &role) in roles.iter().enumerate() {
            host.add_member(member(i), role);
        }
        let mut net = Net {
            host,
            clients: (0..roles.len()).map(|i| Client::new(member(i))).collect(),
            up: vec![VecDeque::new(); roles.len()],
            down: vec![VecDeque::new(); roles.len()],
            connected: vec![false; roles.len()],
            snapshots: 0,
            catch_ups: 0,
        };
        for i in 0..roles.len() {
            net.connect(i);
        }
        net.settle();
        net
    }

    pub fn is_connected(&self, i: usize) -> bool {
        self.connected[i]
    }

    pub fn connect(&mut self, i: usize) {
        self.connected[i] = true;
        self.clients[i].connect();
        self.flush(i);
    }

    pub fn disconnect(&mut self, i: usize) {
        self.connected[i] = false;
        self.up[i].clear();
        self.down[i].clear();
        self.host.disconnected(member(i));
        self.clients[i].disconnected();
    }

    /// Client `i` makes an edit.
    pub fn submit(&mut self, i: usize, label: &str, ops: Vec<Op>) -> Result<u64, Refusal> {
        let result = self.clients[i].submit(label, ops);
        self.flush(i);
        result
    }

    pub fn new_id(&mut self, i: usize) -> InstanceId {
        let id = self.clients[i].new_id().expect("the client has IDs");
        self.flush(i);
        id
    }

    pub fn set_role(&mut self, i: usize, role: Role) {
        let out = self.host.set_role(member(i), role);
        self.route(out);
    }

    /// Moves one message from client `i` to the host. False if none waits.
    pub fn deliver_up(&mut self, i: usize) -> bool {
        let Some(message) = self.up[i].pop_front() else {
            return false;
        };
        let out = self.host.receive(member(i), message);
        self.route(out);
        true
    }

    /// Moves one message from the host to client `i`. False if none waits.
    pub fn deliver_down(&mut self, i: usize) -> bool {
        let Some(message) = self.down[i].pop_front() else {
            return false;
        };
        self.clients[i].receive(message);
        self.flush(i);
        true
    }

    pub fn waiting_up(&self, i: usize) -> usize {
        self.up[i].len()
    }

    pub fn waiting_down(&self, i: usize) -> usize {
        self.down[i].len()
    }

    /// Delivers everything until nothing moves.
    pub fn settle(&mut self) {
        loop {
            let mut moved = false;
            for i in 0..self.clients.len() {
                while self.deliver_up(i) {
                    moved = true;
                }
                while self.deliver_down(i) {
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
    }

    fn flush(&mut self, i: usize) {
        let out = self.clients[i].take_outbox();
        if self.connected[i] {
            self.up[i].extend(out);
        }
    }

    fn route(&mut self, out: Vec<Outgoing>) {
        for Outgoing { to, message } in out {
            let i = (to.0 - 1) as usize;
            match message {
                ToClient::Welcome { .. } => self.snapshots += 1,
                ToClient::Resumed { .. } => self.catch_ups += 1,
                _ => {}
            }
            if self.connected[i] {
                self.down[i].push_back(message);
            }
        }
    }

    /// Asserts every connected, joined client's copy equals the host's.
    pub fn assert_converged(&self) {
        let host = dump(self.host.doc());
        for (i, client) in self.clients.iter().enumerate() {
            let doc = client.doc().expect("joined");
            let copy = dump(doc);
            if copy != host {
                panic!(
                    "client {i}'s copy differs from the host's:\n{}",
                    line_diff(&host, &copy)
                );
            }
            assert_eq!(client.pending(), 0, "client {i} still has pending edits");
        }
    }
}

/// The lines that differ, each cut short (host first).
fn line_diff(host: &str, copy: &str) -> String {
    let short = |line: &str| line.chars().take(110).collect::<String>();
    let host: Vec<_> = host.lines().collect();
    let copy: Vec<_> = copy.lines().collect();
    let mut out = String::new();
    for n in 0..host.len().max(copy.len()) {
        let (h, c) = (host.get(n).copied(), copy.get(n).copied());
        if h != c {
            let (h, c) = (h.unwrap_or("-"), c.unwrap_or("-"));
            let only = |a: &str, b: &str| {
                a.split(' ')
                    .filter(|word| !b.split(' ').any(|other| other == *word))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let _ = writeln!(
                out,
                "line {n}: {}\n  host has: {}\n  copy has: {}",
                short(h.trim_start())
                    .split(" Anchored")
                    .next()
                    .unwrap_or(""),
                only(h, c),
                only(c, h)
            );
        }
    }
    out
}

/// Everything about a document that must match on every copy: the tree
/// from `game` down (IDs, classes, names, properties, attributes, tags,
/// child order), and each deleted Instance with where it was.
pub(crate) fn dump(doc: &Doc) -> String {
    let mut out = String::new();
    dump_tree(&doc.world, doc.world.root(), 0, &mut out);
    let mut graves: Vec<_> = doc.graveyard.iter().collect();
    graves.sort_by_key(|(id, _)| id.to_bits());
    for (id, place) in graves {
        let _ = writeln!(out, "deleted from {:?}:", place);
        dump_tree(&doc.world, *id, 1, &mut out);
    }
    out
}

fn dump_tree(world: &World, id: InstanceId, depth: usize, out: &mut String) {
    let Some(instance) = world.get(id) else {
        let _ = writeln!(
            out,
            "{:indent$}{} missing",
            "",
            id.to_bits(),
            indent = depth * 2
        );
        return;
    };
    let _ = write!(
        out,
        "{:indent$}{} {:?} {:?}",
        "",
        id.to_bits(),
        instance.class(),
        instance.name(),
        indent = depth * 2
    );
    for (key, value) in instance.properties() {
        let _ = write!(out, " {key}={value:?}");
    }
    for (key, value) in instance.attributes() {
        let _ = write!(out, " @{key}={value:?}");
    }
    for tag in instance.tags() {
        let _ = write!(out, " #{tag}");
    }
    out.push('\n');
    for &child in instance.children() {
        dump_tree(world, child, depth + 1, out);
    }
}

/// A Part named `name` under `parent`, at `id`.
pub(crate) fn part(id: InstanceId, parent: InstanceId, name: &str) -> NodeSpec {
    NodeSpec {
        id,
        class: ClassName::Part,
        parent,
        index: None,
        name: name.to_owned(),
        properties: Vec::new(),
        attributes: Vec::new(),
        tags: Vec::new(),
    }
}

pub(crate) fn create(nodes: Vec<NodeSpec>) -> Op {
    Op::CreateTree { nodes }
}

pub(crate) fn set(id: InstanceId, name: &str, value: Value) -> Op {
    Op::SetProps {
        id,
        properties: vec![(name.to_owned(), value)],
    }
}

pub(crate) fn rename(id: InstanceId, name: &str) -> Op {
    set(id, "Name", Value::String(name.to_owned()))
}

pub(crate) fn move_to(id: InstanceId, parent: InstanceId) -> Op {
    Op::Reparent {
        id,
        parent: Some(parent),
        index: None,
        restore: false,
    }
}

pub(crate) fn delete(id: InstanceId) -> Op {
    Op::Reparent {
        id,
        parent: None,
        index: None,
        restore: false,
    }
}

pub(crate) fn restore(id: InstanceId, parent: InstanceId) -> Op {
    Op::Reparent {
        id,
        parent: Some(parent),
        index: None,
        restore: true,
    }
}

/// The child of the Workspace named `name` in client `i`'s copy.
pub(crate) fn find(net: &Net, i: usize, name: &str) -> Option<InstanceId> {
    let world = net.clients[i].world()?;
    world.find_first_descendant(world.root(), name)
}
