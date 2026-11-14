//! Hosting and joining between two games in this process, over real UDP
//! on this computer and over the loopback.

use std::time::{Duration, Instant};

use axcenity_core::{ClassName, Value, World};
use axcenity_script::{OutputKind, ScriptRuntime};

use super::*;
use crate::handshake::unique_name;

/// One running copy of a game: its world, scripts and network.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    network: Network,
    /// Everything printed and logged, as text.
    output: Vec<String>,
}

const SECONDS: f64 = 1.0 / 60.0;

impl Game {
    /// A game of "Castle" at `version`, whose player on this device has
    /// joined. `discovery` is the LAN discovery port, a different one per
    /// test, so tests running at once don't find each other.
    fn new(version: &str, discovery: u16) -> Game {
        let mut world = World::new();
        let mut scripts = ScriptRuntime::new();
        // Starting a LocalScript is when this device's player joins.
        let script = world.create(ClassName::LocalScript).unwrap();
        world
            .set_parent(script, Some(world.starter_player_scripts()))
            .unwrap();
        scripts.start_script(&mut world, script, "scripts/start.client.lua", "");
        let network = Network::new(GameInfo {
            name: "Castle".to_owned(),
            version: version.to_owned(),
        })
        .with_discovery_port(discovery);
        Game {
            world,
            scripts,
            network,
            output: Vec::new(),
        }
    }

    fn start(&mut self, path: &str, source: &str) {
        let (class, service) = if path.ends_with(".client.lua") {
            (ClassName::LocalScript, self.world.starter_player_scripts())
        } else {
            (ClassName::Script, self.world.server_script_service())
        };
        let script = self.world.create(class).unwrap();
        self.world.set_parent(script, Some(service)).unwrap();
        self.scripts
            .start_script(&mut self.world, script, path, source);
        self.collect();
    }

    /// One frame: network receive, scripts, network send.
    fn frame(&mut self) {
        self.network
            .receive(&mut self.world, &mut self.scripts, SECONDS);
        self.scripts.step(&mut self.world, SECONDS);
        self.network.send(&self.world, &mut self.scripts);
        self.collect();
    }

    fn collect(&mut self) {
        for line in self.scripts.take_output() {
            self.output.push(match line.kind {
                OutputKind::Print => line.text,
                OutputKind::Error => format!("error: {}", line.text),
                OutputKind::Warning => format!("warning: {}", line.text),
            });
        }
        self.output.extend(self.network.take_log());
    }

    /// The names in `Players`, sorted.
    fn players(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .world
            .get(self.world.players())
            .unwrap()
            .children()
            .iter()
            .filter_map(|&id| self.world.get(id))
            .filter(|player| player.class() == ClassName::Player)
            .map(|player| player.name().to_owned())
            .collect();
        names.sort();
        names
    }

    fn said(&self, text: &str) -> bool {
        self.output.iter().any(|line| line.contains(text))
    }

    fn host_udp(&mut self) -> u16 {
        self.network
            .host(&mut self.world, &mut self.scripts, Some(0), None, true)
            .expect("a free port")
    }
}

/// Runs frames of both games (about 60 a second, as real UDP needs real
/// time) until `done`, or fails after five seconds.
fn run_until(a: &mut Game, b: &mut Game, what: &str, done: impl Fn(&Game, &Game) -> bool) {
    let start = Instant::now();
    while !done(a, b) {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "timed out waiting for {what}\nhost: {:?}\njoiner: {:?}",
            a.output,
            b.output
        );
        a.frame();
        b.frame();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn two_players_on_one_computer_join_over_udp_and_see_each_other_in_players() {
    let mut host = Game::new("1.0", 47911);
    host.start(
        "scripts/welcome.server.lua",
        r#"game.Players.PlayerAdded:Connect(function(player)
            print("added " .. player.Name)
        end)"#,
    );
    let port = host.host_udp();
    let mut joiner = Game::new("1.0", 47912);
    joiner
        .network
        .join(
            &mut joiner.world,
            &mut joiner.scripts,
            &format!("127.0.0.1:{port}"),
            None,
        )
        .unwrap();

    run_until(&mut host, &mut joiner, "both rosters", |a, b| {
        a.players().len() == 2 && b.players().len() == 2
    });
    assert_eq!(host.players(), ["Player", "Player2"]);
    assert_eq!(joiner.players(), ["Player", "Player2"]);
    // The joiner's own player is the one the host numbered.
    let local = joiner.scripts.local_player().unwrap();
    assert_eq!(joiner.world.get(local).unwrap().name(), "Player2");
    assert!(joiner.network.is_joined());
    assert!(host.said("added Player2"), "{:?}", host.output);
    assert!(host.said("Player2 joined."), "{:?}", host.output);
}

#[test]
fn a_version_mismatch_is_refused_with_a_clear_reason_on_both_sides() {
    let mut host = Game::new("1.0", 47913);
    let port = host.host_udp();
    let mut joiner = Game::new("2.0", 47914);
    joiner.start(
        "scripts/menu.client.lua",
        r#"game:GetService("NetworkService").ConnectionFailed:Connect(function(reason)
            print("failed: " .. reason)
        end)"#,
    );
    joiner
        .network
        .join(
            &mut joiner.world,
            &mut joiner.scripts,
            &format!("127.0.0.1:{port}"),
            None,
        )
        .unwrap();

    run_until(&mut host, &mut joiner, "the refusal", |a, b| {
        b.said("failed:") && a.said("refused")
    });
    assert!(
        joiner.said(
            "failed: Version mismatch: the server runs version 1.0 of \"Castle\" and you have \
             version 2.0."
        ),
        "{:?}",
        joiner.output
    );
    assert!(
        host.said("refused a player with version 2.0 of \"Castle\": this server runs version 1.0"),
        "{:?}",
        host.output
    );
    assert_eq!(host.players(), ["Player"]);
    assert_eq!(joiner.players(), ["Player"]);
    assert!(!joiner.network.is_joined());
}

#[test]
fn a_script_joins_through_network_service_and_leaving_fires_player_removing() {
    let mut host = Game::new("1.0", 47915);
    host.start(
        "scripts/goodbye.server.lua",
        r#"game.Players.PlayerRemoving:Connect(function(player)
            print("removing " .. player.Name)
        end)"#,
    );
    let port = host.host_udp();
    let mut joiner = Game::new("1.0", 47916);
    joiner.start(
        "scripts/menu.client.lua",
        &format!(r#"game:GetService("NetworkService"):Join("127.0.0.1:{port}")"#),
    );
    run_until(&mut host, &mut joiner, "the join", |a, b| {
        a.players().len() == 2 && b.players().len() == 2
    });

    joiner.start(
        "scripts/leave.client.lua",
        r#"game:GetService("NetworkService"):Disconnect()"#,
    );
    run_until(&mut host, &mut joiner, "the leave", |a, b| {
        a.players().len() == 1 && b.players().len() == 1
    });
    assert!(host.said("removing Player2"), "{:?}", host.output);
    assert!(host.said("Player2 left."), "{:?}", host.output);
    assert_eq!(joiner.players(), ["Player2"]);
}

#[test]
fn the_loopback_joins_like_udp() {
    let mut host = Game::new("1.0", 47917);
    let mut joiner = Game::new("1.0", 47918);
    let client = host
        .network
        .host_in_memory(&mut host.world, &mut host.scripts, true);
    joiner
        .network
        .join_through(&mut joiner.world, &mut joiner.scripts, client, None);
    for _ in 0..10 {
        host.frame();
        joiner.frame();
    }
    assert_eq!(host.players(), ["Player", "Player2"]);
    assert_eq!(joiner.players(), ["Player", "Player2"]);

    // The host stops: the joiner is told, and the host leaves its Players.
    host.network.disconnect(&mut host.world, &mut host.scripts);
    for _ in 0..5 {
        host.frame();
        joiner.frame();
    }
    assert_eq!(host.players(), ["Player"]);
    assert_eq!(joiner.players(), ["Player2"]);
    assert!(
        joiner.said("The server closed the connection."),
        "{:?}",
        joiner.output
    );
}

#[test]
fn a_wrong_password_and_a_full_game_are_refused() {
    let mut host = Game::new("1.0", 47919);
    let players = host.world.players();
    host.world
        .set_property(players, "MaxPlayers", Value::Number(2.0))
        .unwrap();
    let port = host
        .network
        .host(
            &mut host.world,
            &mut host.scripts,
            Some(0),
            Some("moat".to_owned()),
            true,
        )
        .unwrap();
    let address = format!("127.0.0.1:{port}");

    let mut wrong = Game::new("1.0", 47920);
    let password = Some("drawbridge".to_owned());
    wrong
        .network
        .join(&mut wrong.world, &mut wrong.scripts, &address, password)
        .unwrap();
    run_until(&mut host, &mut wrong, "the refusal", |_, b| {
        b.said("Wrong password.")
    });

    let mut first = Game::new("1.0", 47921);
    let password = Some("moat".to_owned());
    first
        .network
        .join(&mut first.world, &mut first.scripts, &address, password)
        .unwrap();
    run_until(&mut host, &mut first, "the join", |a, _| {
        a.players().len() == 2
    });

    let mut second = Game::new("1.0", 47922);
    let password = Some("moat".to_owned());
    second
        .network
        .join(&mut second.world, &mut second.scripts, &address, password)
        .unwrap();
    run_until(&mut host, &mut second, "the refusal", |_, b| {
        b.said("The game is full (2 of 2 players).")
    });
}

#[test]
fn find_lan_games_lists_a_game_hosted_on_this_computer() {
    let discovery = 47923;
    let mut host = Game::new("1.0", discovery);
    let port = host.host_udp();
    // The searcher uses the same discovery port, as every game does.
    let mut searcher = Game::new("1.0", discovery);
    searcher.start(
        "scripts/browser.client.lua",
        r#"local games = game:GetService("NetworkService"):FindLanGames()
        for _, found in games do
            print(table.concat({ found.Name, found.Version, found.Address,
                tostring(found.Players), tostring(found.MaxPlayers), tostring(found.HasPassword) }, "|"))
        end
        print("searched " .. #games)"#,
    );
    run_until(&mut host, &mut searcher, "the search", |_, b| {
        b.said("searched")
    });
    let listed = searcher
        .output
        .iter()
        .find(|line| line.starts_with("Castle"))
        .unwrap_or_else(|| panic!("nothing found: {:?}", searcher.output));
    assert!(listed.starts_with("Castle|1.0|"), "{listed}");
    assert!(listed.ends_with(&format!(":{port}|1|8|false")), "{listed}");
    assert!(searcher.said("searched 1"), "{:?}", searcher.output);
}

/// This computer's address on its network, if it has one: the one its
/// packets to the internet would leave from (none are sent).
fn network_address() -> Option<std::net::Ipv4Addr> {
    let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect(("192.0.2.1", 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// Another computer on the LAN: its search reaches the host at this
/// computer's network address, as its broadcast does, not the loopback;
/// the game is listed at that address and can be joined there.
#[test]
fn a_search_from_the_network_lists_the_game_at_its_network_address_and_joins_it() {
    let Some(lan) = network_address() else {
        eprintln!("skipped: this computer has no network address");
        return;
    };
    let discovery = 47926;
    let mut host = Game::new("1.0", discovery);
    let port = host.host_udp();
    let mut search = LanSearch::start(discovery, "Castle")
        .unwrap()
        .asking_only(lan);
    let found = loop {
        host.frame();
        if let Some(found) = search.update(Duration::from_secs_f64(SECONDS)) {
            break found;
        }
        std::thread::sleep(Duration::from_millis(16));
    };
    let [game] = found.as_slice() else {
        panic!("one game: {found:?}\nhost: {:?}", host.output);
    };
    assert_eq!(game.address, format!("{lan}:{port}"));
    assert_eq!((game.players, game.max_players), (1, 8));

    let mut joiner = Game::new("1.0", 47927);
    joiner
        .network
        .join(&mut joiner.world, &mut joiner.scripts, &game.address, None)
        .unwrap();
    run_until(&mut host, &mut joiner, "the join", |a, b| {
        a.players().len() == 2 && b.players().len() == 2
    });
}

/// The check by hand for a game on the LAN showing on another computer.
/// Host `examples/hello-part` on one computer (`cargo run -p
/// axcenity-player -- --host examples/hello-part`), then on another on the
/// same network run `cargo test -p axcenity-net -- --ignored --nocapture
/// a_game_hosted_on_another_computer`.
#[test]
#[ignore = "needs examples/hello-part hosted on another computer on the LAN"]
fn a_game_hosted_on_another_computer_shows_in_a_lan_search() {
    let mut search = LanSearch::start(DISCOVERY_PORT, "Hello Part").unwrap();
    let found = loop {
        if let Some(found) = search.update(Duration::from_millis(50)) {
            break found;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    println!("found: {found:?}");
    let here = network_address().map(std::net::IpAddr::V4);
    assert!(
        found.iter().any(|game| {
            let ip = game.address.parse::<std::net::SocketAddr>().unwrap().ip();
            !ip.is_loopback() && Some(ip) != here
        }),
        "no game from another computer: {found:?}"
    );
}

#[test]
fn hosting_on_a_taken_port_fires_connection_failed() {
    let mut first = Game::new("1.0", 47924);
    let port = first.host_udp();
    let mut second = Game::new("1.0", 47925);
    second.start(
        "scripts/menu.client.lua",
        &format!(
            r#"local NetworkService = game:GetService("NetworkService")
            NetworkService.ConnectionFailed:Connect(function(reason) print(reason) end)
            NetworkService:Host({port})"#
        ),
    );
    second.frame();
    assert!(
        second.said(&format!("Can't host on port {port}")),
        "{:?}",
        second.output
    );
    assert!(!second.network.is_hosting());
}

#[test]
fn a_taken_name_gets_a_number() {
    let taken = ["Player", "Player2", "Knight"];
    assert_eq!(unique_name("Player", taken.iter().copied()), "Player3");
    assert_eq!(unique_name("Knight", taken.iter().copied()), "Knight2");
    assert_eq!(unique_name("Squire", taken.iter().copied()), "Squire");
    assert_eq!(unique_name("  ", taken.iter().copied()), "Player3");
}

/// `name`'s number in the repository's `releases/formats.toml`, which the
/// release tools copy into each release record.
fn released_format(name: &str) -> u32 {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../releases/formats.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    text.lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == name).then(|| value.trim().parse().unwrap())
        })
        .unwrap_or_else(|| panic!("releases/formats.toml has no `{name}`"))
}

#[test]
fn the_release_files_list_this_network_protocol() {
    assert_eq!(released_format("network_protocol"), PROTOCOL_VERSION);
}

#[test]
fn a_secured_host_keeps_its_certificate_and_its_join_link_joins_it() {
    let folder = std::env::temp_dir().join(format!("axcenity-net-cert-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut host = Game::new("1.0", 47926);
    host.network.set_data_folder(Some(folder.clone()));
    let port = host.host_udp();
    host.collect();
    let fingerprint = host.network.fingerprint().expect("hosted secured");
    assert!(host.said(&format!("#{fingerprint}")), "{:?}", host.output);

    let mut joiner = Game::new("1.0", 47927);
    let link = crate::join_link(&format!("127.0.0.1:{port}"), &fingerprint);
    joiner
        .network
        .join(&mut joiner.world, &mut joiner.scripts, &link, None)
        .unwrap();
    run_until(&mut host, &mut joiner, "the secured join", |a, b| {
        a.players().len() == 2 && b.players().len() == 2
    });
    assert_eq!(host.network.tokens_issued(), 1);

    // Hosting again from the same data folder: the same certificate, so
    // players who joined before still trust it.
    host.network.disconnect(&mut host.world, &mut host.scripts);
    host.host_udp();
    assert_eq!(host.network.fingerprint(), Some(fingerprint));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_lan_host_is_not_secured_and_still_joins() {
    let mut host = Game::new("1.0", 47928);
    host.network.set_secure(false);
    let port = host.host_udp();
    host.collect();
    assert!(host.network.fingerprint().is_none());
    assert!(host.said("not secured"), "{:?}", host.output);
    let mut joiner = Game::new("1.0", 47929);
    joiner
        .network
        .join(
            &mut joiner.world,
            &mut joiner.scripts,
            &format!("127.0.0.1:{port}"),
            None,
        )
        .unwrap();
    run_until(&mut host, &mut joiner, "the LAN join", |a, b| {
        a.players().len() == 2 && b.players().len() == 2
    });
    assert_eq!(host.network.tokens_issued(), 0);
}

/// A relay on a free port, and a host of "Castle" on it with its code.
fn relayed_host(discovery: u16) -> (axcenity_relay::Relay, String, Game) {
    let relay = axcenity_relay::Relay::start(axcenity_relay::RelaySettings {
        port: 0,
        ..Default::default()
    })
    .expect("a free port");
    let address = format!("127.0.0.1:{}", relay.port());
    let mut host = Game::new("1.0", discovery);
    host.network.set_relay(Some(address.clone()));
    host.host_udp();
    let start = Instant::now();
    while host.network.join_code().is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "no join code: {:?}",
            host.output
        );
        host.frame();
        std::thread::sleep(Duration::from_millis(10));
    }
    (relay, address, host)
}

#[test]
fn a_player_joins_by_code_through_the_relay() {
    let (relay, address, mut host) = relayed_host(47930);
    let code = host.network.join_code().unwrap().to_owned();
    assert!(
        host.said(&format!("/{code}#")),
        "the join link: {:?}",
        host.output
    );
    let mut joiner = Game::new("1.0", 47931);
    joiner.network.set_relay(Some(address.clone()));
    // No direct path: everything goes through the relay.
    joiner.network.set_punching(false);
    let shown = format!("{}-{}", &code[..3], &code[3..]).to_lowercase();
    joiner
        .network
        .join(&mut joiner.world, &mut joiner.scripts, &shown, None)
        .unwrap();
    run_until(&mut host, &mut joiner, "the relayed join", |a, b| {
        a.players().len() == 2 && b.players().len() == 2
    });
    let relay_address: std::net::SocketAddr = address.parse().unwrap();
    assert_eq!(joiner.network.route(), Some(Route::Relayed(relay_address)));
    assert!(joiner.said("through the relay"), "{:?}", joiner.output);
    assert_eq!(relay.players(), 1);

    // The game goes on through the relay: a chat-free check that the
    // joiner still hears the host a second later.
    for _ in 0..60 {
        host.frame();
        joiner.frame();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(joiner.network.is_joined());
}

#[test]
fn a_join_by_code_punches_a_direct_path_when_it_can() {
    let (_relay, address, mut host) = relayed_host(47932);
    let code = host.network.join_code().unwrap().to_owned();
    let fingerprint = host.network.fingerprint().unwrap();
    let mut joiner = Game::new("1.0", 47933);
    // The host's link: the relay, the code and the fingerprint.
    let link = crate::join_link(&format!("{address}/{code}"), &fingerprint);
    joiner
        .network
        .join(&mut joiner.world, &mut joiner.scripts, &link, None)
        .unwrap();
    run_until(&mut host, &mut joiner, "the punched join", |a, b| {
        a.players().len() == 2 && b.players().len() == 2
    });
    // On one computer the host's address as the relay sees it is reachable.
    assert!(
        matches!(joiner.network.route(), Some(Route::Punched(_))),
        "{:?}",
        joiner.network.route()
    );
}

#[test]
fn a_wrong_code_or_fingerprint_is_refused_with_why() {
    let (_relay, address, mut host) = relayed_host(47934);
    let code = host.network.join_code().unwrap().to_owned();
    let mut joiner = Game::new("1.0", 47935);
    joiner.start(
        "scripts/menu.client.lua",
        r#"game:GetService("NetworkService").ConnectionFailed:Connect(function(reason)
            print("failed: " .. reason)
        end)"#,
    );
    joiner.network.set_relay(Some(address.clone()));
    let other = if code == "ZZZZZZ" { "YYYYYY" } else { "ZZZZZZ" };
    joiner
        .network
        .join(&mut joiner.world, &mut joiner.scripts, other, None)
        .unwrap();
    run_until(&mut host, &mut joiner, "the refusal", |_, b| {
        b.said("failed:")
    });
    assert!(
        joiner.said("No game is hosted with the code"),
        "{:?}",
        joiner.output
    );

    let wrong = crate::join_link(&format!("{address}/{code}"), &"ab".repeat(32));
    joiner.output.clear();
    joiner
        .network
        .join(&mut joiner.world, &mut joiner.scripts, &wrong, None)
        .unwrap();
    run_until(&mut host, &mut joiner, "the wrong certificate", |_, b| {
        b.said("failed:")
    });
    assert!(
        joiner.said("isn't the one you expected"),
        "{:?}",
        joiner.output
    );

    // A code with no relay to look it up on.
    let mut lost = Game::new("1.0", 47936);
    let err = lost
        .network
        .join(&mut lost.world, &mut lost.scripts, &code, None)
        .unwrap_err();
    assert!(err.contains("which relay"), "{err}");
}
