//! Phase 76: Abilities in multiplayer. The server activates them and
//! checks cost, cooldown and tags, whatever a player's device says; the
//! device shows the cooldown sweep at once.

use std::time::Duration;

use axcenity_core::input::{Action, InputEvent, InputKind, InputMap, parse_binding};
use axcenity_core::{ClassName, InstanceId, KeyCode, Value, World};
use axcenity_script::{OutputKind, ScriptRuntime};

use super::*;
use crate::replication_tests::{SECONDS, info};

/// Gives each player who joins 100 Mana and a Fireball (20 Mana, 3 s of
/// cooldown, on the Fireball action), and counts its activations.
const SERVER: &str = r#"
game.Players.PlayerAdded:Connect(function(player)
    player:SetAttribute("Mana", 100)
    local fireball = Instance.new("Ability")
    fireball.Name = "Fireball"
    fireball.Cooldown = 3
    fireball.CostAttribute = "Mana"
    fireball.CostAmount = 20
    fireball.ActivationKey = "Fireball"
    fireball.Activated:Connect(function(by)
        print("fireball for " .. by.Name)
    end)
    fireball.Parent = player
end)
"#;

struct Device {
    world: World,
    scripts: ScriptRuntime,
    network: Network,
    output: Vec<String>,
}

impl Device {
    fn frame(&mut self, events: &[InputEvent]) {
        self.network
            .receive(&mut self.world, &mut self.scripts, SECONDS);
        self.scripts.input_events(&mut self.world, events);
        self.scripts.step_abilities(&mut self.world, SECONDS);
        self.scripts.step(&mut self.world, SECONDS);
        if !self.network.is_following() {
            self.scripts.step_health(&mut self.world, SECONDS);
        }
        self.network.send(&self.world, &mut self.scripts);
        for line in self.scripts.take_output() {
            if line.kind == OutputKind::Print {
                self.output.push(line.text);
            }
        }
    }

    fn fireball(&self) -> Option<InstanceId> {
        let player = self.scripts.local_player()?;
        self.world.find_first_child(player, "Fireball")
    }
}

/// `axcenity-server` and one player's device, `latency` apart, after a
/// moment to join.
fn game(latency: Duration) -> (Device, Device, InstanceId) {
    let mut world = World::new();
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    scripts.start_script(&mut world, script, "scripts/abilities.server.lua", SERVER);
    let mut network = Network::new(info()).with_discovery_port(47935);
    let client = network.host_in_memory(&mut world, &mut scripts, false);
    network.emulate(Conditions {
        latency,
        ..Conditions::default()
    });
    let mut server = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };

    let mut world = World::new();
    let mut scripts = ScriptRuntime::new();
    scripts.set_user_id(700.0);
    let mut map = InputMap::default();
    map.actions.push(Action {
        name: "Fireball".to_owned(),
        axis: false,
        inputs: parse_binding("Q").unwrap(),
    });
    scripts.set_input_map(map);
    let mut network = Network::new(info());
    network.join_through(&mut world, &mut scripts, client, None);
    let mut player = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };
    for _ in 0..60 {
        server.frame(&[]);
        player.frame(&[]);
    }
    let fireball = player
        .fireball()
        .expect("the player's device has the Fireball");
    (server, player, fireball)
}

fn key(pressed: bool) -> InputEvent {
    InputEvent::new(InputKind::Key {
        key: KeyCode::Q,
        pressed,
    })
}

fn mana(device: &Device, player: InstanceId) -> f64 {
    device
        .world
        .get(player)
        .unwrap()
        .attribute("Mana")
        .and_then(Value::as_number)
        .unwrap()
}

#[test]
fn a_modified_client_that_ignores_its_cooldown_still_gets_one_fireball_per_cooldown() {
    let (mut server, mut player, fireball) = game(Duration::from_millis(50));
    let me = player.scripts.local_player().unwrap();

    // For 4 seconds the modified client clears its own cooldown and
    // presses Q every other frame, so it asks the server 120 times.
    let mut asked = 0;
    for frame in 0..240 {
        let _ = player
            .world
            .set_property(fireball, "CooldownLeft", Value::Number(0.0));
        let event = key(frame % 2 == 0);
        if frame % 2 == 0 {
            asked += 1;
        }
        server.frame(&[]);
        player.frame(&[event]);
    }
    assert_eq!(asked, 120);
    for _ in 0..30 {
        server.frame(&[]);
        player.frame(&[]);
    }
    let fired: Vec<&String> = server
        .output
        .iter()
        .filter(|line| line.starts_with("fireball"))
        .collect();
    assert_eq!(fired.len(), 2, "at 0 s and 3 s: {fired:?}");
    assert_eq!(fired[0], "fireball for Player");
    assert_eq!(mana(&server, me), 60.0, "two casts of 20");
    assert_eq!(mana(&player, me), 60.0, "and the device sees it");
}

#[test]
fn the_players_device_shows_the_cooldown_sweep_at_once_and_the_server_confirms_it() {
    let (mut server, mut player, fireball) = game(Duration::from_millis(150));
    let me = player.scripts.local_player().unwrap();
    assert_eq!(player.scripts.ability_bar(&player.world).len(), 1);

    server.frame(&[]);
    player.frame(&[key(true)]);
    // The same frame, before any reply: the sweep is full.
    let slot = &player.scripts.ability_bar(&player.world)[0];
    assert_eq!(slot.key, "Q");
    assert!(slot.sweep() > 0.95, "{}", slot.sweep());
    assert_eq!(mana(&player, me), 100.0, "the server spends it");

    for _ in 0..40 {
        server.frame(&[]);
        player.frame(&[key(false)]);
    }
    assert_eq!(mana(&player, me), 80.0);
    let left = player
        .world
        .get(fireball)
        .unwrap()
        .property("CooldownLeft")
        .and_then(Value::as_number)
        .unwrap();
    assert!(
        left > 2.0 && left < 3.0,
        "the server's cooldown arrived: {left}"
    );
}
