//! Phase 67: players' saves on the server, chat and kicking, with a
//! dedicated server and players' devices in this process.

use std::path::PathBuf;

use axcenity_script::ScriptRuntime;

use super::*;
use crate::replication_tests::{Device, castle, frames, info};

fn temp_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-net-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

/// `axcenity-server` with the castle running `server`, saving in `saves`,
/// hosting in memory.
fn server_running(server: &str, saves: Option<PathBuf>) -> (Device, Client) {
    let castle = castle();
    let mut world = castle.world;
    let mut scripts = ScriptRuntime::new();
    scripts.set_dedicated();
    scripts.set_save_folder(saves);
    scripts.start_script(
        &mut world,
        castle.server_script,
        "scripts/main.server.lua",
        server,
    );
    let mut network = Network::new(info()).with_discovery_port(47934);
    let client = network.host_in_memory(&mut world, &mut scripts, false);
    let device = Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    };
    (device, client)
}

/// A player's device running `client` as its LocalScript.
fn player_running(link: Client, user_id: f64, client: &str) -> Device {
    let castle = castle();
    let mut world = castle.world;
    let mut scripts = ScriptRuntime::new();
    scripts.set_user_id(user_id);
    let mut network = Network::new(info());
    network.set_client_scripts(vec![ScriptCode {
        instance: castle.client_script,
        path: "scripts/hud.client.lua".to_owned(),
        source: client.to_owned(),
    }]);
    network.join_through(&mut world, &mut scripts, link, None);
    Device {
        world,
        scripts,
        network,
        output: Vec::new(),
    }
}

/// A server and a player for each of `users`, joined one after another.
fn game(
    server: &str,
    client: &str,
    users: &[f64],
    saves: Option<PathBuf>,
) -> (Device, Vec<Device>) {
    let (mut server, first) = server_running(server, saves);
    let mut first = Some(first);
    let mut players = Vec::new();
    for &user in users {
        let link = match first.take() {
            Some(link) => link,
            None => server.network.connect_loopback().unwrap(),
        };
        players.push(player_running(link, user, client));
        frames(&mut server, &mut players, 10);
    }
    (server, players)
}

const COINS_SERVER: &str = r#"
local SaveService = game:GetService("SaveService")
game.Players.PlayerAdded:Connect(function(player)
    print("joined", player.UserId, SaveService:GetPlayerData(player, "Coins") or 0)
end)
game.ReplicatedStorage.Shout.OnServerEvent:Connect(function(player, coins)
    local had = SaveService:GetPlayerData(player, "Coins") or 0
    SaveService:SetPlayerData(player, "Coins", had + coins)
end)
"#;

const COINS_CLIENT: &str = r#"
local me = game.Players.LocalPlayer
task.wait(0.1)
game.ReplicatedStorage.Shout:FireServer(me.UserId == 101 and 5 or 7)
"#;

#[test]
fn two_players_coins_are_saved_separately_on_the_server_and_come_back_when_each_rejoins() {
    let saves = temp_folder("coins");
    {
        let (mut server, mut players) = game(
            COINS_SERVER,
            COINS_CLIENT,
            &[101.0, 102.0],
            Some(saves.clone()),
        );
        frames(&mut server, &mut players, 30);
        assert!(server.said("joined 101 0"), "{:?}", server.output);
        assert!(server.said("joined 102 0"), "{:?}", server.output);
        // Both leave; what's kept for each is written as they go.
        for player in &mut players {
            player
                .network
                .disconnect(&mut player.world, &mut player.scripts);
        }
        frames(&mut server, &mut players, 10);
        assert!(saves.join("players/101.json").exists());
        assert!(saves.join("players/102.json").exists());
    }
    // Another server, from the same data, and they come back the other
    // way round.
    let (server, _players) = game(COINS_SERVER, "", &[102.0, 101.0], Some(saves.clone()));
    assert!(server.said("joined 102 7"), "{:?}", server.output);
    assert!(server.said("joined 101 5"), "{:?}", server.output);
    let _ = std::fs::remove_dir_all(&saves);
}

const CHAT_SERVER: &str = r#"
local TextChatService = game:GetService("TextChatService")
TextChatService.OnIncomingMessage = function(message)
    if message.Text:find("darn") then
        return nil
    end
    return (message.Text:gsub("hi", "hello"))
end
task.wait(0.5)
TextChatService:DisplaySystemMessage("Welcome to the castle")
"#;

const CHAT_CLIENT: &str = r#"
local TextChatService = game:GetService("TextChatService")
local me = game.Players.LocalPlayer
TextChatService.MessageReceived:Connect(function(message)
    local from = message.Player and message.Player.Name or "system"
    print(me.Name, "got", from, message.Text)
end)
if me.Name == "Player2" then
    task.wait(0.2)
    TextChatService:SendMessage("hi all")
    TextChatService:SendMessage("darn it")
end
"#;

#[test]
fn a_chat_message_from_one_client_reaches_the_others_and_on_incoming_message_can_change_or_drop_it()
{
    let (mut server, mut players) = game(CHAT_SERVER, CHAT_CLIENT, &[1.0, 2.0, 3.0], None);
    frames(&mut server, &mut players, 60);
    for (player, name) in players.iter().zip(["Player", "Player2", "Player3"]) {
        assert!(
            player.said(&format!("{name} got Player2 hello all")),
            "{name}: {:?}",
            player.output
        );
        assert!(
            player.said(&format!("{name} got system Welcome to the castle")),
            "{name}: {:?}",
            player.output
        );
        assert!(
            !player.output.iter().any(|line| line.contains("darn")),
            "dropped: {:?}",
            player.output
        );
        // The chat window shows the same.
        let lines = player.scripts.chat_lines();
        assert!(
            lines
                .iter()
                .any(|line| line.from.as_deref() == Some("Player2") && line.text == "hello all"),
            "{lines:?}"
        );
    }
}

#[test]
fn a_kicked_player_is_told_why_and_leaves_the_game() {
    let client = r#"
game:GetService("NetworkService").ConnectionFailed:Connect(function(reason)
    print("failed", reason)
end)
"#;
    let (mut server, mut players) = game("", client, &[1.0, 2.0], None);
    server
        .scripts
        .run_command(&mut server.world, "game.Players.Player2:Kick('Cheating')")
        .unwrap();
    frames(&mut server, &mut players, 90);
    assert!(
        players[1].said("failed Cheating"),
        "{:?}",
        players[1].output
    );
    assert!(
        server.said("Kicked Player2: Cheating"),
        "{:?}",
        server.output
    );
    let names: Vec<String> = server
        .world
        .get(server.world.players())
        .unwrap()
        .children()
        .iter()
        .map(|&id| server.world.get(id).unwrap().name().to_owned())
        .collect();
    assert_eq!(names, ["Player"]);
    assert!(!players[0].said("failed Cheating"));
}
