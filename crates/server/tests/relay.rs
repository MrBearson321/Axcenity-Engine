//! Multiplayer readiness MP5: the real `axcenity-server` gets a join code
//! from a relay, and `axcenity-bots` join with its link, through the relay
//! or through a hole they punch. (Behind real routers: `scripts/nat-lab.sh`.)

mod common;

use axcenity_relay::{Relay, RelaySettings};
use common::{bots, example, finish, scratch, server_with};

#[test]
fn bots_join_by_the_servers_relay_link_through_the_relay_or_directly() {
    let relay = Relay::start(RelaySettings {
        port: 0,
        ..Default::default()
    })
    .expect("a free port");
    let folder = scratch("relay");
    let game = example("hello-spawn");
    let address = format!("127.0.0.1:{}", relay.port());
    let server = server_with(&game, 0, &["--relay", &address], &[]);
    let said = server.wait_for_all(&["Join code"], 15);
    let link = said
        .iter()
        .filter(|line| line.contains("Join code"))
        .find_map(|line| line.split("players join with ").nth(1))
        .expect("a join link")
        .trim()
        .to_owned();
    assert!(
        link.starts_with(&format!("axcenity://join/{address}/")),
        "{link}"
    );

    for (punch, route) in [(false, "relayed"), (true, "punched")] {
        let report = folder.join(format!("{route}.jsonl"));
        let mut arguments = vec!["--join", &link, "--count", "3", "--seconds", "4", "--idle"];
        if !punch {
            arguments.push("--no-punch");
        }
        let running = bots(&game, server.port, &report, &arguments);
        let (ok, summary) = finish(running, &report);
        assert!(ok, "{summary}");
        assert_eq!(summary["joined"], 3, "{summary}");
        assert_eq!(summary["routes"][route], 3, "{summary}");
    }
    let _ = std::fs::remove_dir_all(&folder);
}
