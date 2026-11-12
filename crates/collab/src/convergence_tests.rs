//! Convergence: three members make random edits while messages arrive in
//! random order, connections drop and come back, and the host sometimes
//! keeps only a short log (so a member rejoins from a snapshot). Afterwards
//! every copy must equal the host's, no transaction may have been applied
//! twice, and no copy may ever have disagreed with the host.

use axcenity_core::{ClassName, InstanceId, Material, Value, Vector3};

use crate::apply::Note;
use crate::client::Notice;
use crate::host::Host;
use crate::op::{NodeSpec, Op};
use crate::role::Role;
use crate::testing::*;

/// A small, fast random number generator (xorshift64*), so the test needs
/// no dependency and a failing seed always replays the same way.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        (!items.is_empty()).then(|| items[self.below(items.len())])
    }
}

const MEMBERS: usize = 3;

/// 150 seeds by default (a few seconds); set `AXCENITY_COLLAB_SEEDS` to run
/// more, and `AXCENITY_COLLAB_SEED` to replay one.
#[test]
fn random_edits_over_a_bad_network_end_the_same_everywhere() {
    let number = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok());
    if let Some(seed) = number("AXCENITY_COLLAB_SEED") {
        run(seed);
        return;
    }
    let mut seen = Seen::default();
    for seed in 0..number("AXCENITY_COLLAB_SEEDS").unwrap_or(150) {
        seen.add(&run(seed));
    }
    // The test only means something if the hard paths happened.
    assert!(seen.refused > 0, "no transaction was refused: {seen:?}");
    assert!(seen.dropped > 0, "delete never won over an edit: {seen:?}");
    assert!(
        seen.rehomed > 0,
        "nothing went to a surviving ancestor: {seen:?}"
    );
    assert!(
        seen.rejoined_from_snapshot > 0,
        "nobody rejoined from a snapshot: {seen:?}"
    );
    assert!(
        seen.caught_up > 0,
        "nobody caught up from the log: {seen:?}"
    );
    eprintln!("{seen:?}");
}

/// How often the hard paths happened across all seeds.
#[derive(Debug, Default)]
struct Seen {
    refused: usize,
    dropped: usize,
    rehomed: usize,
    rejoined_from_snapshot: usize,
    caught_up: usize,
}

impl Seen {
    fn add(&mut self, other: &Seen) {
        self.refused += other.refused;
        self.dropped += other.dropped;
        self.rehomed += other.rehomed;
        self.rejoined_from_snapshot += other.rejoined_from_snapshot;
        self.caught_up += other.caught_up;
    }
}

fn run(seed: u64) -> Seen {
    let mut seen = Seen::default();
    let mut rng = Rng::new(seed);
    let (world, _) = sample_world();
    let host = if seed.is_multiple_of(4) {
        Host::new(world).with_log_kept(3)
    } else {
        Host::new(world)
    };
    let mut net = Net::with_host(host, &[Role::Owner, Role::Editor, Role::Editor]);
    // Instances each member is building outside the game.
    let mut building: Vec<Vec<InstanceId>> = vec![Vec::new(); MEMBERS];
    for _ in 0..300 {
        let roll = rng.below(100);
        let i = rng.below(MEMBERS);
        match roll {
            0..=16 => {
                let ops = random_transaction(&mut net, i, &mut rng);
                if !ops.is_empty() {
                    // A move into itself is refused locally; that's fine.
                    let _ = net.submit(i, "Edit", ops);
                }
            }
            17..=34 => {
                // As a tool makes them: straight on the World, then captured.
                for _ in 0..1 + rng.below(3) {
                    random_direct_edit(&mut net, i, &mut rng, &mut building);
                }
                net.capture(i, "Edit");
            }
            35..=84 => {
                if rng.chance(50) {
                    net.deliver_up(i);
                } else {
                    net.deliver_down(i);
                }
            }
            85..=89 if net.is_connected(i) => net.disconnect(i),
            90..=96 if !net.is_connected(i) => net.connect(i),
            _ => {}
        }
        check_no_resync(&mut net, seed, &mut seen);
    }
    for i in 0..MEMBERS {
        if !net.is_connected(i) {
            net.connect(i);
        }
    }
    net.settle();
    check_no_resync(&mut net, seed, &mut seen);
    for i in 0..MEMBERS {
        assert_eq!(net.waiting_up(i) + net.waiting_down(i), 0);
    }
    let mut applied: Vec<_> = net.host.log().map(|l| (l.author, l.client_seq)).collect();
    let all = applied.len();
    applied.sort();
    applied.dedup();
    assert_eq!(
        applied.len(),
        all,
        "seed {seed}: a transaction was applied twice"
    );
    // `assert_converged` names the client; the seed says how to replay it.
    eprintln!("seed {seed}: {} transactions", net.host.seq());
    net.assert_converged();
    // Each member's first join is a snapshot too.
    seen.rejoined_from_snapshot = net.snapshots - MEMBERS;
    seen.caught_up = net.catch_ups;
    seen
}

fn check_no_resync(net: &mut Net, seed: u64, seen: &mut Seen) {
    for client in &mut net.clients {
        for notice in client.take_notices() {
            match notice {
                Notice::Joined | Notice::Arrived { .. } | Notice::Left { .. } => {}
                Notice::Resynced => panic!("seed {seed}: a copy disagreed with the host"),
                Notice::HostLeft => panic!("seed {seed}: the host never ends here"),
                Notice::DocRefused { .. } => panic!("seed {seed}: no live documents here"),
                Notice::Refused { .. } => seen.refused += 1,
                Notice::Changed { notes, .. } => {
                    for note in notes {
                        match note {
                            Note::Dropped { .. } => seen.dropped += 1,
                            Note::Rehomed { .. } => seen.rehomed += 1,
                        }
                    }
                }
            }
        }
    }
}

fn sample_world() -> (axcenity_core::World, ()) {
    let mut world = axcenity_core::World::new();
    let workspace = world.workspace();
    for name in ["A", "B", "C"] {
        let id = world.create(axcenity_core::ClassName::Part).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(workspace)).unwrap();
    }
    let folder = world.create(axcenity_core::ClassName::Folder).unwrap();
    world.set_name(folder, "Shed").unwrap();
    world.set_parent(folder, Some(workspace)).unwrap();
    (world, ())
}

/// One to three random edits, made from what client `i` sees now.
fn random_transaction(net: &mut Net, i: usize, rng: &mut Rng) -> Vec<Op> {
    let count = 1 + rng.below(3);
    let mut ops = Vec::new();
    for _ in 0..count {
        if let Some(op) = random_op(net, i, rng) {
            ops.push(op);
        }
    }
    ops
}

fn random_op(net: &mut Net, i: usize, rng: &mut Rng) -> Option<Op> {
    if !net.clients[i].is_joined() {
        return None;
    }
    let world = &net.worlds[i];
    let workspace = world.workspace();
    let inside = world.descendants(workspace);
    let parts: Vec<InstanceId> = inside
        .iter()
        .copied()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|x| x.class() == axcenity_core::ClassName::Part)
        })
        .collect();
    let mut places = inside.clone();
    places.push(workspace);
    let deleted: Vec<InstanceId> = {
        let mut ids: Vec<_> = net.clients[i].graveyard().keys().copied().collect();
        ids.sort_by_key(|id| id.to_bits());
        ids
    };
    let op = match rng.below(10) {
        0 | 1 => {
            let parent = rng.pick(&places)?;
            let class = if rng.chance(70) {
                axcenity_core::ClassName::Part
            } else {
                axcenity_core::ClassName::Model
            };
            let first = net.try_new_id(i)?;
            let mut nodes = vec![NodeSpec {
                class,
                ..part(first, parent, &format!("N{}", rng.below(50)))
            }];
            if class == axcenity_core::ClassName::Model
                && let Some(second) = net.try_new_id(i)
            {
                nodes.push(part(second, first, "Inside"));
            }
            create(nodes)
        }
        2 => set(
            rng.pick(&parts)?,
            "Transparency",
            Value::Number(rng.below(5) as f64 / 4.0),
        ),
        3 => {
            let material = rng.pick(&[
                Material::Plastic,
                Material::Wood,
                Material::Granite,
                Material::Marble,
            ])?;
            Op::SetProps {
                id: rng.pick(&parts)?,
                properties: vec![
                    ("Material".to_owned(), Value::Material(material)),
                    (
                        "Metalness".to_owned(),
                        Value::Number(rng.below(3) as f64 / 2.0),
                    ),
                ],
            }
        }
        4 => set(
            rng.pick(&parts)?,
            "Position",
            Value::Vector3(Vector3 {
                x: rng.below(10) as f32,
                y: 1.0,
                z: rng.below(10) as f32,
            }),
        ),
        5 => rename(rng.pick(&inside)?, &format!("R{}", rng.below(20))),
        6 => {
            let id = rng.pick(&inside)?;
            Op::Reparent {
                id,
                parent: Some(rng.pick(&places)?),
                index: rng.chance(50).then(|| rng.below(3)),
                restore: false,
            }
        }
        7 => {
            if rng.chance(60) {
                delete(rng.pick(&inside)?)
            } else {
                restore(rng.pick(&deleted)?, rng.pick(&places)?)
            }
        }
        8 => Op::SetAttribute {
            id: rng.pick(&inside)?,
            name: rng.pick(&["Damage", "Speed"])?.to_owned(),
            value: rng.chance(70).then(|| Value::Number(rng.below(10) as f64)),
        },
        _ => Op::Tag {
            id: rng.pick(&inside)?,
            tag: rng.pick(&["Lava", "Hot", "Big"])?.to_owned(),
            added: rng.chance(60),
        },
    };
    Some(op)
}

/// One edit straight on member `i`'s World, as a tool makes it. Errors
/// (a move into itself, a stale ID) are ignored, as a tool would refuse
/// them.
fn random_direct_edit(net: &mut Net, i: usize, rng: &mut Rng, building: &mut [Vec<InstanceId>]) {
    if !net.clients[i].is_joined() {
        return;
    }
    let deleted: Vec<InstanceId> = {
        let mut ids: Vec<_> = net.clients[i].graveyard().keys().copied().collect();
        ids.sort_by_key(|id| id.to_bits());
        ids
    };
    let world = net.world_mut(i);
    building[i].retain(|&id| world.contains(id) && world.get(id).unwrap().parent().is_none());
    let workspace = world.workspace();
    let inside = world.descendants(workspace);
    let mut places = inside.clone();
    places.push(workspace);
    let parts: Vec<InstanceId> = inside
        .iter()
        .copied()
        .filter(|&id| world.get(id).is_some_and(|x| x.class() == ClassName::Part))
        .collect();
    match rng.below(12) {
        0 => {
            let Some(parent) = rng.pick(&places) else {
                return;
            };
            let Ok(id) = world.create(ClassName::Part) else {
                return;
            };
            let _ = world.set_name(id, format!("D{}", rng.below(50)));
            let _ = world.set_parent(id, Some(parent));
        }
        1 => {
            // Built outside the game first; placed (or thrown away) later.
            let Ok(id) = world.create(ClassName::Model) else {
                return;
            };
            if let Ok(part) = world.create(ClassName::Part) {
                let _ = world.set_parent(part, Some(id));
            }
            let _ = world.add_tag(id, "Prefab");
            building[i].push(id);
        }
        2 => {
            let Some(id) = rng.pick(&building[i]) else {
                return;
            };
            if rng.chance(25) {
                let _ = world.destroy(id);
            } else if let Some(parent) = rng.pick(&places) {
                let _ = world.set_parent(id, Some(parent));
            }
        }
        3 => {
            let Some(id) = rng.pick(&inside) else { return };
            if let Ok(copy) = world.duplicate(id) {
                let parent = rng.pick(&places).unwrap_or(workspace);
                let _ = world.set_parent(copy, Some(parent));
            }
        }
        4 => {
            let Some(id) = rng.pick(&parts) else { return };
            let material = rng
                .pick(&[Material::Plastic, Material::Wood, Material::Granite])
                .unwrap();
            let _ = world.set_known_property(id, "Material", Value::Material(material));
            if rng.chance(50) {
                let _ = world.set_known_property(
                    id,
                    "Metalness",
                    Value::Number(rng.below(3) as f64 / 2.0),
                );
            }
        }
        5 => {
            let Some(id) = rng.pick(&parts) else { return };
            let _ = world.set_known_property(
                id,
                "Position",
                Value::Vector3(Vector3 {
                    x: rng.below(10) as f32,
                    y: 1.0,
                    z: rng.below(10) as f32,
                }),
            );
        }
        6 => {
            let Some(id) = rng.pick(&inside) else { return };
            let _ = world.set_name(id, format!("S{}", rng.below(20)));
        }
        7 => {
            let (Some(id), Some(parent)) = (rng.pick(&inside), rng.pick(&places)) else {
                return;
            };
            if rng.chance(50) {
                let _ = world.set_parent_at(id, parent, rng.below(3));
            } else {
                let _ = world.set_parent(id, Some(parent));
            }
        }
        8 => {
            if rng.chance(60) {
                let Some(id) = rng.pick(&inside) else { return };
                let _ = world.set_parent(id, None);
            } else {
                let (Some(id), Some(parent)) = (rng.pick(&deleted), rng.pick(&places)) else {
                    return;
                };
                let _ = world.set_parent(id, Some(parent));
            }
        }
        9 => {
            let Some(id) = rng.pick(&inside) else { return };
            let name = rng.pick(&["Damage", "Speed"]).unwrap();
            let value = rng.chance(70).then(|| Value::Number(rng.below(10) as f64));
            let _ = world.set_attribute(id, name, value);
        }
        10 => {
            let Some(id) = rng.pick(&inside) else { return };
            let tag = rng.pick(&["Lava", "Hot", "Big"]).unwrap();
            if rng.chance(60) {
                let _ = world.add_tag(id, tag);
            } else {
                let _ = world.remove_tag(id, tag);
            }
        }
        _ => {
            let Some(id) = rng.pick(&parts) else { return };
            let _ = world.set_known_property(
                id,
                "Transparency",
                Value::Number(rng.below(5) as f64 / 4.0),
            );
        }
    }
}
