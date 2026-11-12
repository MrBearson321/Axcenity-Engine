//! C11's measurements on S10k, the standard scene: what joining costs in
//! bytes and time, with and without zstd, and what an idle host costs.
//! Run by hand, in release:
//! `cargo test -p axcenity-collab --release --lib perf_tests -- --ignored --nocapture`.

use std::time::{Duration, Instant};

use crate::host::Host;
use crate::message::{Snapshot, ToClient};
use crate::role::Role;
use crate::testing::*;
use crate::wire;

fn welcome(world: &axcenity_core::World) -> ToClient {
    ToClient::Welcome {
        world: Snapshot(Box::new(world.clone())),
        graveyard: Default::default(),
        seq: 0,
        last_client_seq: 0,
        role: Role::Editor,
        ids: None,
        rejected: Vec::new(),
        files: Default::default(),
        session: String::new(),
        epoch: 0,
    }
}

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let out = f();
    (out, started.elapsed())
}

#[test]
#[ignore = "a measurement, run by hand"]
fn join_message_of_s10k_with_and_without_zstd() {
    let world = axcenity_testing::s10k::world();
    let message = welcome(&world);
    let (sent, encode) = time(|| wire::encode_to_client(&message));
    let (_, decode) = time(|| wire::decode_to_client(&sent).unwrap());
    // What it would be unpacked: the format byte, then postcard.
    let bytes = match zstd::bulk::decompress(&sent[1..], 1 << 30) {
        Ok(body) if sent[0] & 0x80 != 0 => body,
        _ => sent[1..].to_vec(),
    };
    println!(
        "S10k join message: {} KiB unpacked, {} KiB as sent; encoded in {encode:?}, \
         decoded in {decode:?} (packing included)",
        bytes.len() / 1024,
        sent.len() / 1024
    );
    for level in [1, 3, 9] {
        let (packed, pack) = time(|| zstd::bulk::compress(&bytes, level).unwrap());
        let (_, unpack) = time(|| zstd::bulk::decompress(&packed, bytes.len()).unwrap());
        println!(
            "  zstd {level}: {} KiB ({:.0}%), {pack:?} to pack, {unpack:?} to unpack",
            packed.len() / 1024,
            100.0 * packed.len() as f64 / bytes.len() as f64
        );
    }
    let script = axcenity_testing::s10k::script_files().concat_text();
    let (packed, pack) = time(|| zstd::bulk::compress(script.as_bytes(), 1).unwrap());
    println!(
        "S10k's 200 scripts: {} KiB, zstd 1: {} KiB in {pack:?}",
        script.len() / 1024,
        packed.len() / 1024
    );
}

trait Concat {
    fn concat_text(&self) -> String;
}

impl Concat for Vec<(String, String)> {
    fn concat_text(&self) -> String {
        self.iter().map(|(_, text)| text.as_str()).collect()
    }
}

#[test]
#[ignore = "a measurement, run by hand"]
fn joining_and_idling_on_s10k_with_four_members() {
    let world = axcenity_testing::s10k::world();
    let host = Host::new(world);
    let (mut net, joined) = time(|| {
        let mut net = Net::with_host(
            host,
            &[Role::Owner, Role::Editor, Role::Editor, Role::Viewer],
        );
        net.settle();
        net
    });
    println!("four members joined S10k in {joined:?} (in-process, every message through wire)");
    net.assert_converged();
    // Idle: nothing changes; each frame the host ticks and each client
    // captures (nothing) and sends what it has (nothing).
    let frames = 2_000;
    let mut host_time = Duration::ZERO;
    let mut client_time = Duration::ZERO;
    let start = Instant::now();
    for frame in 0..frames {
        let now = start + Duration::from_millis(16 * frame);
        let ((), spent) = time(|| net.tick(now));
        host_time += spent;
        for i in 0..4 {
            let (_, spent) = time(|| net.capture(i, "Edit"));
            client_time += spent;
        }
        net.settle();
    }
    println!(
        "idle, per frame: host tick {:?}, each member's capture {:?}",
        host_time / frames as u32,
        client_time / (frames as u32 * 4)
    );
}

#[test]
#[ignore = "a measurement, run by hand"]
fn presence_traffic_of_four_members() {
    use crate::message::MemberId;
    use crate::presence::{Camera, Presence, TextCursor};
    // A busy member: ten selected, the camera moving, a field being
    // dragged and a caret in a script.
    let presence = Presence {
        selection: (1..=10)
            .map(|n| axcenity_core::InstanceId::from_bits(1_000 + n))
            .collect(),
        camera: Some(Camera {
            position: [12.5, 40.0, -3.25],
            look: [0.0, -0.6, 0.8],
        }),
        editing: Some("Transparency".into()),
        script: Some(TextCursor {
            path: "ServerScriptService/Door.server.lua".into(),
            anchor: vec![7; 12],
            head: vec![7; 12],
        }),
    };
    let up = wire::encode_to_host(&crate::message::ToHost::Presence(presence.clone())).len();
    let down = wire::encode_to_client(&ToClient::Presence {
        member: MemberId(2),
        presence,
    })
    .len();
    // At the client's most, 10 a second each; each goes to the 3 others.
    let per_member = 10 * (up + 3 * down);
    println!(
        "presence: {up} bytes up, {down} down; four busy members: {} KiB a second in all \
         ({} messages a second through the host)",
        4 * per_member / 1024,
        4 * 10 * 4
    );
}

/// What a member's folder costs to look at (every 0.5 s in the editor):
/// listing the synced files and reading each one's length and time.
#[test]
#[ignore = "a measurement, run by hand"]
fn a_look_at_the_s10k_folder() {
    let folder = std::env::temp_dir().join(format!("axcenity-perf-look-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    axcenity_testing::s10k::write(&folder).unwrap();
    let mut times = Vec::new();
    let mut count = 0;
    for _ in 0..50 {
        let (listed, spent) = time(|| {
            let listed = crate::files::list(&folder).unwrap();
            for path in &listed {
                std::hint::black_box(std::fs::metadata(folder.join(path)).unwrap().len());
            }
            listed.len()
        });
        count = listed;
        times.push(spent);
    }
    times.sort();
    println!(
        "a look at S10k's {count} synced files: median {:?}, most {:?}",
        times[times.len() / 2],
        times[times.len() - 1]
    );
    let _ = std::fs::remove_dir_all(&folder);
}

/// An idle [`Mirror::sync`](crate::mirror::Mirror::sync) on S10k (every
/// 0.5 s in the editor), for the host's folder and a member's.
#[test]
#[ignore = "a measurement, run by hand"]
fn an_idle_mirror_sync_on_s10k() {
    use crate::files::FolderStore;
    use crate::mirror::Mirror;
    let base = std::env::temp_dir().join(format!("axcenity-perf-mirror-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (project, theirs) = (base.join("project"), base.join("theirs"));
    axcenity_testing::s10k::write(&project).unwrap();
    std::fs::create_dir_all(&theirs).unwrap();
    let (store, manifest) = FolderStore::open(&project).unwrap();
    let host = Host::new(axcenity_core::World::new()).with_files(manifest, Box::new(store));
    let mut net = Net::with_host(host, &[Role::Owner, Role::Editor]);
    let mut mirrors = [
        Mirror::open(&project, "perf", "Owner"),
        Mirror::open(&theirs, "perf", "Sam"),
    ];
    for _ in 0..8 {
        for (i, mirror) in mirrors.iter_mut().enumerate() {
            net.with_client(i, |client, world| mirror.sync(client, world))
                .unwrap();
        }
        net.settle();
    }
    for (i, who) in ["host's folder", "member's folder"].into_iter().enumerate() {
        let mut times = Vec::new();
        for _ in 0..50 {
            let (changes, spent) =
                time(|| net.with_client(i, |client, world| mirrors[i].sync(client, world)));
            assert!(changes.unwrap().is_empty(), "idle");
            times.push(spent);
        }
        times.sort();
        println!(
            "an idle sync of the {who}: median {:?}, most {:?}",
            times[times.len() / 2],
            times[times.len() - 1]
        );
    }
    let _ = std::fs::remove_dir_all(&base);
}
