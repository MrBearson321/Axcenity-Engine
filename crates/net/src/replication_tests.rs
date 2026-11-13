//! Phase 64: a dedicated server and players' devices in this process,
//! over the loopback. Each device has its own world: the server's, and a
//! replica of it on each player's device.

use axcenity_core::{
    CFrame, ClassName, Color3, EnumItem, InstanceId, LayerWeight, Material, Parameter,
    PhysicalProperties, UDim2, Value, Vector2, Vector3, World,
};
use axcenity_script::{OutputKind, ScriptRuntime};

use super::*;
use crate::wire::{WireArg, WireKind};

pub(crate) const SECONDS: f64 = 1.0 / 60.0;

const SERVER: &str = r#"
local remote = game.ReplicatedStorage.Shout
game.Players.PlayerAdded:Connect(function(player)
    print("added", player.Name, player.DisplayName, player.UserId > 0)
end)
game.Players.PlayerRemoving:Connect(function(player)
    print("removing", player.Name)
end)
remote.OnServerEvent:Connect(function(player, what, thing)
    print("from", player.Name, what, thing)
    remote:FireAllClients("hello from " .. player.Name)
end)
game.ServerStorage.Secret.OnServerEvent:Connect(function(player)
    print("the secret was reached by", player.Name)
end)
game.ReplicatedStorage.Ask.OnServerInvoke = function(player, n)
    return n * 2
end
"#;

const CLIENT: &str = r#"
local me = game.Players.LocalPlayer
game.ReplicatedStorage.Shout.OnClientEvent:Connect(function(text)
    print(me.Name, "heard", text)
end)
game.Players.PlayerAdded:Connect(function(player)
    print(me.Name, "sees", player.Name, "join")
end)
game.Players.PlayerRemoving:Connect(function(player)
    print(me.Name, "sees", player.Name, "leave")
end)
game.ReplicatedStorage.Ask.OnClientInvoke = function(question)
    return me.Name .. " says " .. question
end
print(me.Name, "storage", #game.ServerStorage:GetChildren(),
    #game.ServerScriptService:GetChildren(), game.Workspace:FindFirstChild("Gate") ~= nil)
-- Once all three are in, Player2 shouts and asks.
if me.Name == "Player2" then
    game.Players.PlayerAdded:Connect(function(player)
        if player.Name == "Player3" then
            game.ReplicatedStorage.Shout:FireServer("hi", game.Workspace.Gate)
            print(me.Name, "doubled", game.ReplicatedStorage.Ask:InvokeServer(21))
        end
    end)
end
"#;

/// The same game on every device: loading it makes the same Instances
/// at the same IDs everywhere.
pub(crate) struct Castle {
    pub(crate) world: World,
    pub(crate) server_script: InstanceId,
    pub(crate) client_script: InstanceId,
}

pub(crate) fn castle() -> Castle {
    let mut world = World::new();
    let add = |world: &mut World, class, name: &str, parent| {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    };
    let workspace = world.workspace();
    let gate = add(&mut world, ClassName::Part, "Gate", workspace);
    world
        .set_property(gate, "Color", Value::Color3(Color3::new(0.5, 0.2, 0.1)))
        .unwrap();
    world
        .set_attribute(gate, "Locked", Some(Value::Bool(true)))
        .unwrap();
    world.add_tag(gate, "Door").unwrap();
    let storage = world.server_storage();
    add(&mut world, ClassName::Model, "Treasure", storage);
    add(&mut world, ClassName::RemoteEvent, "Secret", storage);
    let replicated = world.replicated_storage();
    add(&mut world, ClassName::RemoteEvent, "Shout", replicated);
    add(&mut world, ClassName::RemoteFunction, "Ask", replicated);
    let service = world.server_script_service();
    let server_script = add(&mut world, ClassName::Script, "Main", service);
    let scripts = world.starter_player_scripts();
    let client_script = add(&mut world, ClassName::LocalScript, "Hud", scripts);
    Castle {
        world,
        server_script,
        client_script,
    }
}

pub(crate) fn info() -> GameInfo {
    GameInfo {
        name: "Castle".to_owned(),
        version: "1.0".to_owned(),
    }
}

/// One device: its world, scripts and network.
pub(crate) struct Device {
    pub(crate) world: World,
    pub(crate) scripts: ScriptRuntime,
    pub(crate) network: Network,
    pub(crate) output: Vec<String>,
}

impl Device {
    pub(crate) fn frame(&mut self) {
        self.network
            .receive(&mut self.world, &mut self.scripts, SECONDS);
        self.scripts.step(&mut self.world, SECONDS);
        self.network.send(&self.world, &mut self.scripts);
        for line in self.scripts.take_output() {
            self.output.push(match line.kind {
                OutputKind::Print => line.text.replace('\t', " "),
                _ => format!("error: {}", line.text),
            });
        }
        self.output.extend(self.network.take_log());
    }

    /// A frame of `axcenity-server`'s, with `Heartbeat`.
    pub(crate) fn frame_with_heartbeat(&mut self) {
        self.network
            .receive(&mut self.world, &mut self.scripts, SECONDS);
        self.scripts.step(&mut self.world, SECONDS);
        self.scripts.heartbeat(&mut self.world, SECONDS);
        self.network.send(&self.world, &mut self.scripts);
        self.output.extend(self.network.take_log());
    }

    pub(crate) fn said(&self, text: &str) -> bool {
        self.output.iter().any(|line| line == text)
    }
}

/// `axcenity-server` with the castle, hosting in memory.
pub(crate) fn server() -> (Device, Client) {
    let Castle {
        mut world,
        server_script,
        ..
    } = castle();
    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    scripts.start_script(&mut world, server_script, "scripts/main.server.lua", SERVER);
    let mut network = Network::new(info()).with_discovery_port(47931);
    let client = network.host_in_memory(&mut world, &mut scripts, false);
    let device = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };
    (device, client)
}

/// A player's device with the castle, joining through `client`.
pub(crate) fn player(client: Client, user_id: f64) -> Device {
    let Castle {
        mut world,
        client_script,
        ..
    } = castle();
    let mut scripts = ScriptRuntime::new();
    scripts.set_user_id(user_id);
    let mut network = Network::new(info());
    network.set_client_scripts(vec![ScriptCode {
        instance: client_script,
        path: "scripts/hud.client.lua".to_owned(),
        source: CLIENT.to_owned(),
    }]);
    network.join_through(&mut world, &mut scripts, client, None);
    Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    }
}

pub(crate) fn frames(server: &mut Device, players: &mut [Device], count: usize) {
    for _ in 0..count {
        server.frame();
        for player in players.iter_mut() {
            player.frame();
        }
    }
}

/// A server and three players, joined one after another.
pub(crate) fn three_players() -> (Device, Vec<Device>) {
    let (mut server, first) = server();
    let mut players = vec![player(first, 101.0)];
    frames(&mut server, &mut players, 5);
    for user_id in [102.0, 103.0] {
        let client = server.network.connect_loopback().unwrap();
        players.push(player(client, user_id));
        frames(&mut server, &mut players, 5);
    }
    (server, players)
}

#[test]
fn a_remote_event_from_player_two_reaches_the_server_with_their_player_and_fire_all_clients_reaches_all_three()
 {
    let (mut server, mut players) = three_players();
    frames(&mut server, &mut players, 5);

    assert!(
        server.said("from Player2 hi Gate"),
        "the server got Player2's Player, and the Gate as its own: {:?}",
        server.output
    );
    for name in ["Player", "Player2", "Player3"] {
        let player = players
            .iter()
            .find(|p| p.said(&format!("{name} storage 0 0 true")));
        let player = player.unwrap_or_else(|| panic!("{name} started: {:?}", players[0].output));
        assert!(
            player.said(&format!("{name} heard hello from Player2")),
            "{name}: {:?}",
            player.output
        );
    }
    // Each device's world is its own, but IDs mean the same Instances.
    let gate = server
        .world
        .find_first_child(server.world.workspace(), "Gate")
        .unwrap();
    for player in &players {
        assert!(!std::ptr::eq(&player.world, &server.world));
        assert_eq!(player.world.get(gate).unwrap().name(), "Gate");
        assert_eq!(
            player.world.get(gate).unwrap().attribute("Locked"),
            Some(&Value::Bool(true))
        );
        assert!(player.world.get(gate).unwrap().has_tag("Door"));
    }
}

#[test]
fn players_join_and_leave_with_player_added_player_removing_user_id_and_display_name() {
    let (mut server, mut players) = three_players();
    for name in ["Player", "Player2", "Player3"] {
        assert!(
            server.said(&format!("added {name} Player true")),
            "{:?}",
            server.output
        );
    }
    assert!(
        players[0].said("Player sees Player2 join"),
        "{:?}",
        players[0].output
    );
    assert!(
        players[0].said("Player sees Player3 join"),
        "{:?}",
        players[0].output
    );
    let second = players[1].scripts.local_player().unwrap();
    assert_eq!(
        server.world.get(second).unwrap().property("UserId"),
        Some(&Value::Number(102.0)),
        "the server knows player 2 by their install's UserId"
    );
    assert_eq!(
        players[0].world.get(second).unwrap().property("UserId"),
        Some(&Value::Number(102.0)),
        "and so does everyone else"
    );

    // Player 2 leaves.
    let mut gone = players.remove(1);
    gone.network.disconnect(&mut gone.world, &mut gone.scripts);
    gone.network.send(&gone.world, &mut gone.scripts);
    frames(&mut server, &mut players, 5);
    assert!(server.said("removing Player2"), "{:?}", server.output);
    assert!(
        players[0].said("Player sees Player2 leave"),
        "{:?}",
        players[0].output
    );
    assert!(
        players[1].said("Player3 sees Player2 leave"),
        "{:?}",
        players[1].output
    );
    assert!(players[0].world.get(second).is_none());
}

#[test]
fn a_player_sees_nothing_in_server_storage_or_server_script_service_and_cant_reach_it() {
    let (mut server, mut players) = three_players();
    let replica = &players[0].world;
    for service in replica.server_only_services() {
        assert!(replica.get(service).unwrap().children().is_empty());
    }
    assert!(
        players[0].said("Player storage 0 0 true"),
        "{:?}",
        players[0].output
    );
    let treasure = server
        .world
        .find_first_child(server.world.server_storage(), "Treasure")
        .unwrap();
    let secret = server
        .world
        .find_first_child(server.world.server_storage(), "Secret")
        .unwrap();
    assert!(!replica.contains(treasure) && !replica.contains(secret));

    // A modified client that knows the IDs anyway: a message to the
    // RemoteEvent in ServerStorage is dropped, and an Instance there
    // arrives as nil.
    let shout = server
        .world
        .find_first_child(server.world.replicated_storage(), "Shout")
        .unwrap();
    let forged = |remote: InstanceId| WireMessage {
        remote: remote.to_bits(),
        player: 0,
        kind: WireKind::Event,
        args: vec![
            WireArg::String(b"steal".to_vec()),
            WireArg::Instance(treasure.to_bits()),
        ],
    };
    let (world, scripts) = (&mut server.world, &mut server.scripts);
    server.network.deliver(world, scripts, 2, &forged(secret));
    server.network.deliver(world, scripts, 2, &forged(shout));
    frames(&mut server, &mut players, 2);
    assert!(
        !server.output.iter().any(|line| line.contains("the secret")),
        "{:?}",
        server.output
    );
    assert!(server.said("from Player steal nil"), "{:?}", server.output);

    // And the server never sends one: a Script's FireAllClients with an
    // Instance in ServerStorage arrives as nil.
    server
        .scripts
        .run_command(
            &mut server.world,
            "game.ReplicatedStorage.Shout:FireAllClients(game.ServerStorage.Treasure)",
        )
        .unwrap();
    frames(&mut server, &mut players, 2);
    assert!(
        players[0].said("Player heard nil"),
        "{:?}",
        players[0].output
    );
}

#[test]
fn invoke_client_asks_one_players_device_over_the_network_and_invoke_server_answers() {
    let (mut server, mut players) = three_players();
    frames(&mut server, &mut players, 3);
    assert!(
        players[1].said("Player2 doubled 42"),
        "{:?}",
        players[1].output
    );

    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
for _, player in game.Players:GetPlayers() do
    task.spawn(function()
        print(game.ReplicatedStorage.Ask:InvokeClient(player, "yes"))
    end)
end
"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 4);
    for name in ["Player", "Player2", "Player3"] {
        assert!(
            server.said(&format!("{name} says yes")),
            "{:?}",
            server.output
        );
    }
}

#[test]
fn parts_a_server_script_makes_moves_and_destroys_follow_on_every_replica() {
    let (mut server, mut players) = three_players();
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
local crate = Instance.new("Part")
crate.Name = "Crate"
crate.Position = Vector3.new(1, 2, 3)
crate.Parent = workspace
_G.crate = crate
"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 1);
    let crate_id = server
        .world
        .find_first_child(server.world.workspace(), "Crate")
        .unwrap();
    let position = |world: &World| {
        world
            .get(crate_id)
            .and_then(|c| c.property("Position"))
            .cloned()
    };
    for player in &players {
        assert_eq!(
            position(&player.world),
            Some(Value::Vector3(Vector3::new(1.0, 2.0, 3.0)))
        );
    }
    // A player's own change stays on their device.
    players[0]
        .world
        .set_property(crate_id, "Material", Value::Material(Material::Neon))
        .unwrap();
    server
        .scripts
        .run_command(
            &mut server.world,
            "_G.crate.Position = Vector3.new(4, 5, 6)",
        )
        .unwrap();
    // A move arrives as motion, shown a moment behind; once the crate has
    // stood still it's sent exactly.
    frames(&mut server, &mut players, 30);
    for player in &players {
        assert_eq!(
            position(&player.world),
            Some(Value::Vector3(Vector3::new(4.0, 5.0, 6.0)))
        );
    }
    assert_eq!(
        players[0].world.get(crate_id).unwrap().property("Material"),
        Some(&Value::Material(Material::Neon))
    );
    assert_ne!(
        server.world.get(crate_id).unwrap().property("Material"),
        Some(&Value::Material(Material::Neon))
    );
    // Into ServerStorage is out of players' sight, and then destroyed.
    server
        .scripts
        .run_command(&mut server.world, "_G.crate.Parent = game.ServerStorage")
        .unwrap();
    frames(&mut server, &mut players, 1);
    for player in &players {
        assert!(!player.world.contains(crate_id));
    }
}

#[test]
fn every_kind_of_value_crosses_the_wire_unchanged() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    let values = [
        Value::Bool(true),
        Value::Number(0.1),
        Value::String("ünïcode".to_owned()),
        Value::Vector3(Vector3::new(0.1, -2.5, 1e-7)),
        Value::Vector2(Vector2::new(3.0, 4.5)),
        Value::UDim2(UDim2::new(0.5, -10.0, 1.0, 3.0)),
        Value::Color3(Color3::new(0.25, 0.5, 1.0)),
        Value::Material(Material::Neon),
        Value::CFrame(CFrame {
            position: Vector3::new(1.0, 2.0, 3.0),
            rotation: [0.0, 0.6, 0.0, 0.8],
        }),
        Value::EnumItem(EnumItem {
            enum_name: "KeyCode".to_owned(),
            item: "E".to_owned(),
        }),
        Value::Instance(Some(part)),
        Value::Instance(None),
        Value::PhysicalProperties(Some(PhysicalProperties::new(0.7, 0.3, 0.5, 1.0, 1.0))),
        Value::PhysicalProperties(None),
        Value::NumberSequence(axcenity_core::NumberSequence::ramp(0.5, 2.0)),
        Value::NumberSequence(axcenity_core::NumberSequence::default()),
        Value::ColorSequence(axcenity_core::ColorSequence::ramp(
            Color3::new(1.0, 0.0, 0.0),
            Color3::new(0.0, 0.5, 1.0),
        )),
    ];
    for value in values {
        let wire = crate::wire::WireValue::from_value(&value);
        let text = serde_json::to_string(&wire).unwrap();
        let back: crate::wire::WireValue = serde_json::from_str(&text).unwrap();
        assert_eq!(back.to_value(), Some(value.clone()), "{text}");
        let bytes = postcard::to_allocvec(&wire).unwrap();
        let back: crate::wire::WireValue = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.to_value(), Some(value), "{text}");
    }
}

#[test]
fn what_a_spline_scatter_made_isnt_sent_but_the_scatter_is() {
    let mut world = World::new();
    let spline = world.create(ClassName::Spline).unwrap();
    world.set_parent(spline, Some(world.workspace())).unwrap();
    let scatter = world.create(ClassName::SplineScatter).unwrap();
    world.set_parent(scatter, Some(spline)).unwrap();
    let tree = world.create(ClassName::MeshPart).unwrap();
    world.set_parent(tree, Some(scatter)).unwrap();
    let mut replicator = crate::replicate::Replicator::default();
    let created = |update: &crate::replicate::Update| -> Vec<u64> {
        update
            .ops
            .iter()
            .filter_map(|op| match op {
                crate::replicate::Op::Create(instance) => Some(instance.id),
                _ => None,
            })
            .collect()
    };
    let frame = replicator.frame(&world);
    let snapshot = replicator.snapshot(&world);
    for update in [&frame, &snapshot] {
        let ids = created(update);
        assert!(ids.contains(&scatter.to_bits()) && ids.contains(&spline.to_bits()));
        assert!(!ids.contains(&tree.to_bits()), "each device makes its own");
    }
    assert!(!crate::replicate::replicated_parts(&world).contains(&tree));
}

#[test]
fn an_ikcontrols_weight_and_target_a_server_script_sets_reach_every_device() {
    let (mut server, mut players) = three_players();
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
local hero = Instance.new("MeshPart")
hero.Name = "Hero"
local ik = Instance.new("IKControl")
ik.Name = "Look"
ik.Parent = hero
local first = Instance.new("Part")
first.Name = "First"
first.Parent = workspace
local second = Instance.new("Part")
second.Name = "Second"
second.Parent = workspace
hero.Parent = workspace
ik.Weight = 0.25
ik.Target = first
_G.ik = ik
_G.second = second
"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 1);
    let world = &server.world;
    let find = |name: &str| world.find_first_child(world.workspace(), name).unwrap();
    let (hero, first, second) = (find("Hero"), find("First"), find("Second"));
    let ik = world.find_first_child(hero, "Look").unwrap();
    for player in &players {
        let control = player.world.get(ik).expect("the IKControl");
        assert_eq!(control.property("Weight"), Some(&Value::Number(0.25)));
        assert_eq!(
            control.property("Target"),
            Some(&Value::Instance(Some(first)))
        );
    }
    server
        .scripts
        .run_command(
            &mut server.world,
            "_G.ik.Weight = 1; _G.ik.Target = _G.second",
        )
        .unwrap();
    frames(&mut server, &mut players, 1);
    for player in &players {
        let control = player.world.get(ik).unwrap();
        assert_eq!(control.property("Weight"), Some(&Value::Number(1.0)));
        assert_eq!(
            control.property("Target"),
            Some(&Value::Instance(Some(second)))
        );
    }
}

#[test]
fn animator_parameters_and_layer_weights_a_server_script_sets_reach_every_device() {
    let (mut server, mut players) = three_players();
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"
local hero = Instance.new("Model")
hero.Name = "Hero"
local animator = Instance.new("Animator")
animator.Parent = hero
hero.Parent = workspace
animator:SetNumber("Speed", 6)
animator:SetBool("Aiming", true)
animator:SetTrigger("Reload")
animator:SetLayerWeight("UpperBody", 1)
_G.animator = animator
"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 1);
    let hero = server
        .world
        .find_first_child(server.world.workspace(), "Hero")
        .unwrap();
    let animator = server.world.find_first_child(hero, "Animator").unwrap();
    for player in &players {
        let live = player
            .world
            .animator(animator)
            .expect("the Animator's state");
        assert_eq!(live.parameters["Speed"], Parameter::Number(6.0));
        assert_eq!(live.parameters["Aiming"], Parameter::Bool(true));
        assert_eq!(live.parameters["Reload"], Parameter::Trigger(true));
        assert_eq!(live.layer_weights["UpperBody"], LayerWeight::new(1.0));
    }
    // A fade goes as where it's heading and how long it takes; each device
    // fades it itself.
    server
        .scripts
        .run_command(
            &mut server.world,
            r#"_G.animator:SetLayerWeight("UpperBody", 0, 0.5) _G.animator:SetNumber("Speed", 2)"#,
        )
        .unwrap();
    frames(&mut server, &mut players, 1);
    for player in &mut players {
        let live = player.world.animator_mut(animator).unwrap();
        assert_eq!(live.parameters["Speed"], Parameter::Number(2.0));
        let mut weight = live.layer_weights["UpperBody"];
        assert_eq!((weight.weight, weight.target, weight.fade), (1.0, 0.0, 0.5));
        weight.step(0.25);
        assert!((weight.weight - 0.5).abs() < 1e-12);
        // A device that uses up its Trigger keeps it used up: nothing
        // changed on the server, so nothing is sent again.
        live.parameters
            .insert("Reload".into(), Parameter::Trigger(false));
    }
    frames(&mut server, &mut players, 2);
    for player in &players {
        let live = player.world.animator(animator).unwrap();
        assert_eq!(live.parameters["Reload"], Parameter::Trigger(false));
    }
    // A player who joins later gets them all in the snapshot.
    let client = server.network.connect_loopback().unwrap();
    players.push(player(client, 104.0));
    frames(&mut server, &mut players, 5);
    let late = players[3].world.animator(animator).unwrap();
    assert_eq!(late.parameters["Speed"], Parameter::Number(2.0));
    assert_eq!(late.layer_weights["UpperBody"].target, 0.0);
}
