//! Joining, players arriving and leaving, the network requests scripts make
//! (`network.rs`, Phase 63), messages between devices (Phase 64), and the
//! per-frame players step (`players.rs`, Phase 47).

use crate::*;

impl ScriptRuntime {
    /// The player on this device joins the game, once: a Player Instance
    /// named `Player` goes into `Players`, and becomes `LocalPlayer`. It
    /// goes on the AutoAssignable Team with the fewest players, and
    /// `PlayerAdded` fires (Phase 47).
    pub(crate) fn join(&mut self, world: &mut World) {
        if self.client.local_player().is_some() || self.dedicated || self.client_only {
            return;
        }
        let player = world
            .create(ClassName::Player)
            .expect("Player is not a service");
        world.set_name(player, "Player").expect("just created");
        let _ = world.set_property(player, "UserId", Value::Number(self.user_id));
        let _ = world.set_property(player, "DisplayName", Value::String("Player".to_owned()));
        world
            .set_parent(player, Some(world.players()))
            .expect("both exist");
        self.client.set_local_player(player);
        players::assign_team(world, player);
        self.player_events
            .push(players::PlayerEvent::Added { player });
        self.fire_changes(world);
    }

    /// A player on another device joins a hosted game (Phase 63): a Player
    /// named `name` goes into `Players`, with their `DisplayName` and
    /// `UserId` (Phase 64), on the AutoAssignable Team with the fewest
    /// players, and `PlayerAdded` fires. Their first character comes in
    /// the next [`step_players`](Self::step_players), as the local
    /// player's does; their own device moves it (Phase 66).
    pub fn add_remote_player(
        &mut self,
        world: &mut World,
        name: &str,
        display_name: &str,
        user_id: f64,
    ) -> InstanceId {
        self.fire_changes(world);
        let player = world
            .create(ClassName::Player)
            .expect("Player is not a service");
        world.set_name(player, name).expect("just created");
        let _ = world.set_property(player, "UserId", Value::Number(user_id));
        let _ = world.set_property(
            player,
            "DisplayName",
            Value::String(display_name.to_owned()),
        );
        world
            .set_parent(player, Some(world.players()))
            .expect("both exist");
        players::assign_team(world, player);
        self.player_events
            .push(players::PlayerEvent::Added { player });
        self.joined.push(player);
        self.fire_changes(world);
        player
    }

    /// Who owns each unanchored assembly now (Phase 66, see `ownership`):
    /// each owning Player with every part they simulate.
    pub fn owned_parts(&self, world: &World) -> BTreeMap<InstanceId, Vec<InstanceId>> {
        ownership::owned_parts(&self.ownership, world)
    }

    /// Lag compensation (Phase 81): a server hosting players notes where
    /// the parts that moved are at its `clock`, each frame, and how far
    /// back each player's shots are turned (their round trip plus the
    /// smoothing delay; 0 for its own player).
    pub fn record_lag(&mut self, world: &World, clock: f64, rewinds: &[(InstanceId, f64)]) {
        let mut lag = self.lag.0.borrow_mut();
        lag.record(world, clock);
        for &(player, seconds) in rewinds {
            lag.set_rewind(player, seconds);
        }
        lag.keep_players(world);
    }

    /// A player on another device leaves: `PlayerRemoving` fires on both
    /// sides, then the Player (and its character) go. The player on this
    /// device never leaves this way.
    pub fn remove_player(&mut self, world: &mut World, player: InstanceId) {
        if self.local_player() == Some(player) || world.get(player).is_none() {
            return;
        }
        let user = world
            .get(player)
            .and_then(|p| p.property("UserId"))
            .and_then(Value::as_number)
            .map(|id| id.max(0.0) as u64);
        self.fire_changes(world);
        self.player_events
            .push(players::PlayerEvent::Removing { player });
        self.fire_changes(world);
        // What `PlayerRemoving` handlers kept for them is written now.
        if let Some(user) = user
            && let Err(err) = self.saves.0.borrow_mut().forget_player(user)
        {
            self.output.push(OutputLine {
                kind: OutputKind::Warning,
                text: format!("SaveService: {err}"),
                trace: Vec::new(),
                side: None,
            });
        }
    }

    /// A Player the server added arrived in this replica (Phase 64):
    /// `PlayerAdded` fires in its LocalScripts.
    pub fn add_replicated_player(&mut self, world: &mut World, player: InstanceId) {
        if !self.client_only || world.get(player).is_none() {
            return;
        }
        self.fire_changes(world);
        self.player_events
            .push(players::PlayerEvent::Added { player });
        self.fire_changes(world);
    }

    /// This device's player's `UserId`: the same each time on one install
    /// (Phase 64). Set it before scripts start.
    pub fn set_user_id(&mut self, user_id: f64) {
        self.user_id = user_id;
    }

    /// This device's player's `UserId`.
    pub fn user_id(&self) -> f64 {
        self.user_id
    }

    /// No player plays on this device: a dedicated server (Phase 63). Set
    /// it before scripts start.
    pub fn set_dedicated(&mut self) {
        self.dedicated = true;
    }

    /// True once [`become_client`](Self::become_client) made this device
    /// follow a server elsewhere.
    pub fn is_client_only(&self) -> bool {
        self.client_only
    }

    /// This device joined a server elsewhere (Phase 64): every script
    /// stops, `world` becomes `replica` (the server's world as it sent it,
    /// see `World::empty_replica`), and `player` (the server's Player for
    /// this device) is `LocalPlayer`. From now on only LocalScripts run:
    /// start them (and the ModuleScripts first) again with
    /// [`start_script`](Self::start_script). The server gives out
    /// characters, so none spawns here.
    pub fn become_client(&mut self, world: &mut World, replica: World, player: InstanceId) {
        for started in std::mem::take(&mut self.started) {
            for side in [Side::Server, Side::Client] {
                self.context(side).stop_script(started.script);
            }
        }
        self.clones.clear();
        self.player_events.clear();
        self.players.started = true;
        self.client_only = true;
        *world = replica;
        world.record_changes();
        for side in [Side::Server, Side::Client] {
            self.context(side).forget_missing_instances(world);
        }
        self.client.set_local_player(player);
        self.fire_changes(world);
    }

    /// RemoteEvent and RemoteFunction messages for another device since
    /// the last call, oldest first: for Players who joined from elsewhere,
    /// or (on a replica) for the server. The host sends them in its
    /// network send step.
    pub fn take_remote_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.outgoing)
    }

    /// Messages that came from another device, which the host checked
    /// (the server sets each one's Player to whoever sent it). They arrive
    /// in the next [`step`](Self::step), as this device's own do.
    pub fn receive_remote_messages(&mut self, messages: Vec<Message>) {
        for message in messages {
            if message.kind == MessageKind::Activate {
                if message.to == Side::Server && !self.client_only {
                    self.activation_requested(&message);
                }
                continue;
            }
            self.transport.send(message);
        }
    }

    /// Renames the player on this device, as a server it joined named it
    /// (a second `Player` is `Player2`).
    pub fn rename_local_player(&mut self, world: &mut World, name: &str) {
        if let Some(player) = self.local_player() {
            let _ = world.set_name(player, name);
            self.fire_changes(world);
        }
    }

    /// The `NetworkService` calls scripts made since the last call, the
    /// Scripts' first, each in the order made (Phase 63). The host does
    /// them in its network step.
    pub fn take_network_requests(&mut self) -> Vec<NetworkRequest> {
        let mut requests = self.server.take_network_requests();
        requests.extend(self.client.take_network_requests());
        requests
    }

    /// Streaming (Phase 80), on a device following a server: `gone` is
    /// off this device now (`StreamingState` reads StreamedOut), and each
    /// of `arrived` that had gone fires `StreamedIn`.
    pub fn note_streaming(
        &mut self,
        world: &mut World,
        gone: &[InstanceId],
        arrived: &[InstanceId],
    ) {
        self.fire_changes(world);
        self.context(Side::Client)
            .note_streaming(world, gone, arrived);
        self.collect(Side::Client);
        self.fire_change_rounds(world);
    }

    /// A `RequestStreamAroundAsync`'s region is ready (or was all here):
    /// the script that asked carries on.
    pub fn finish_stream_around(&mut self, world: &mut World, request: u64) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).finish_stream_around(world, request);
            self.collect(side);
            self.fire_change_rounds(world);
        }
        self.fire_changes(world);
    }

    /// The host's answer to `FindLanGames`: every script waiting in it
    /// carries on with `games`, Scripts first.
    pub fn finish_lan_search(&mut self, world: &mut World, games: &[LanGame]) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).finish_lan_search(world, games);
            self.collect(side);
            self.fire_change_rounds(world);
        }
        self.fire_changes(world);
    }

    /// Fires `NetworkService.ConnectionFailed(reason)` on both sides:
    /// hosting or joining failed, a join was refused or a connection lost.
    pub fn fire_connection_failed(&mut self, world: &mut World, reason: &str) {
        self.fire_changes(world);
        for side in [Side::Server, Side::Client] {
            self.context(side).fire_connection_failed(world, reason);
            self.collect(side);
            self.fire_change_rounds(world);
        }
        self.fire_changes(world);
    }

    /// Makes `character`, placed in the Workspace before Play, `player`'s,
    /// and keeps a copy of it (with no parent, so out of the game) to
    /// respawn as. Its scripts' copies start when a copy goes in.
    pub(crate) fn adopt(&mut self, world: &mut World, player: InstanceId, character: InstanceId) {
        if let Ok(template) = world.duplicate(character) {
            let originals = std::iter::once(character).chain(world.descendants_iter(character));
            let copies = std::iter::once(template).chain(world.descendants_iter(template));
            for (original, copy) in originals.zip(copies) {
                let code = self
                    .started
                    .iter()
                    .find(|started| started.script == original)
                    .map(|started| (started.path.clone(), started.source.clone()));
                if let Some((path, source)) = code {
                    self.clones.push(Started {
                        script: copy,
                        path,
                        source,
                    });
                }
            }
            self.server.set_respawn_template(template);
        }
        let _ = world.set_property(player, "Character", Value::Instance(Some(character)));
        self.player_events
            .push(players::PlayerEvent::CharacterAdded { player, character });
    }

    /// Fires the player events `LoadCharacter`, joining and respawns
    /// queued, on both sides: an old character is destroyed once its
    /// `CharacterRemoving` has fired.
    /// Fires `LevelLoaded` on both sides for each level loaded, after the
    /// changes the load made have fired.
    pub(crate) fn fire_level_events(&mut self, world: &mut World) {
        for _ in 0..MAX_CHANGE_ROUNDS {
            let loaded = std::mem::take(&mut self.levels.0.borrow_mut().loaded);
            if loaded.is_empty() {
                return;
            }
            for name in loaded {
                self.fire_change_rounds(world);
                for side in [Side::Server, Side::Client] {
                    self.context(side).fire_level_loaded(world, &name);
                    self.collect(side);
                    self.fire_change_rounds(world);
                }
                self.level_loads.push(name);
            }
        }
    }

    pub(crate) fn fire_player_events(&mut self, world: &mut World) {
        for _ in 0..MAX_CHANGE_ROUNDS {
            if self.player_events.is_empty() {
                return;
            }
            for event in std::mem::take(&mut self.player_events) {
                let (owner, name, arg) = match event {
                    players::PlayerEvent::Added { player } => {
                        (world.players(), "PlayerAdded", player)
                    }
                    players::PlayerEvent::Removing { player } => {
                        (world.players(), "PlayerRemoving", player)
                    }
                    players::PlayerEvent::CharacterRemoving { player, character } => {
                        (player, "CharacterRemoving", character)
                    }
                    players::PlayerEvent::CharacterAdded { player, character } => {
                        (player, "CharacterAdded", character)
                    }
                };
                for side in [Side::Server, Side::Client] {
                    self.context(side)
                        .fire_with_instance(world, owner, name, arg);
                    self.collect(side);
                    self.fire_change_rounds(world);
                }
                if let players::PlayerEvent::CharacterRemoving { character, .. } = event
                    && world.get(character).is_some_and(|c| c.parent().is_none())
                {
                    let _ = world.destroy(character);
                }
                if let players::PlayerEvent::Removing { player } = event {
                    if let Some(Value::Instance(Some(character))) = world
                        .get(player)
                        .and_then(|instance| instance.property("Character"))
                        .cloned()
                    {
                        let _ = world.destroy(character);
                    }
                    let _ = world.destroy(player);
                }
            }
        }
    }

    /// Frame step 1, after the gameplay crate's `step_humanoids`
    /// (Phase 47). The first time, the player joins and gets a character:
    /// one already in the Workspace becomes theirs (and a copy of it, out
    /// of the game, is what they respawn as), or else, with
    /// `Players.CharacterAutoLoads` on, a copy of the StarterCharacter
    /// spawns. Then spawn ForceFields whose time is up go, and players
    /// whose `RespawnTime` has passed since their Humanoid died get a new
    /// character. `CharacterAdded` and the rest fire in Scripts and then in
    /// LocalScripts.
    pub fn step_players(&mut self, world: &mut World, seconds: f64) {
        self.fire_changes(world);
        let placed = if self.players.started {
            None
        } else {
            axcenity_character::player_character(world)
        };
        self.join(world);
        let auto_loads = |world: &World| {
            world
                .get(world.players())
                .and_then(|service| service.property("CharacterAutoLoads"))
                .and_then(Value::as_bool)
                != Some(false)
        };
        let mut due = Vec::new();
        if !self.players.started
            && let Some(player) = self.local_player()
        {
            self.players.started = true;
            match placed {
                Some(character) => self.adopt(world, player, character),
                None if auto_loads(world) => due.push(player),
                None => {}
            }
        }
        due.extend(self.players.step(world, seconds));
        due.extend(
            std::mem::take(&mut self.joined)
                .into_iter()
                .filter(|&player| world.contains(player)),
        );
        for player in due {
            if auto_loads(world) {
                self.server.load_character(world, player);
                self.collect(Side::Server);
            }
        }
        self.fire_changes(world);
    }

    /// Whether to draw the default leaderboard (`ui::leaderboard_items`):
    /// Tab shows and hides it. Its `Players.LeaderboardEnabled` is the
    /// leaderboard's own to check.
    pub fn leaderboard_shown(&self) -> bool {
        !self.players.leaderboard_hidden
    }

    /// The Player of whoever plays on this device, once they've joined.
    pub fn local_player(&self) -> Option<InstanceId> {
        self.client.local_player()
    }
}
