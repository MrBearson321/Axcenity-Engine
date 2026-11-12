//! Lua and commands typed into the editor's Console or the player's
//! developer console during Play (`console_service.rs`).

use crate::*;

impl ScriptRuntime {
    /// Runs Lua typed into the editor's Console during Play, on the server
    /// side, as a coroutine of its own (so `task.wait` works). Commands
    /// share one set of globals. What it prints, and its errors, join the
    /// output. `Err` is the message for code that isn't valid Lua.
    pub fn run_command(&mut self, world: &mut World, source: &str) -> Result<(), String> {
        self.fire_changes(world);
        let result = self.server.run_command(world, source);
        self.collect(Side::Server);
        self.fire_changes(world);
        result
    }

    /// Lua typed into the player's Developer Console (Phase 71): on the
    /// server side, or on the client side on a device following a server
    /// elsewhere. Otherwise as [`run_command`](Self::run_command).
    pub fn run_console_lua(&mut self, world: &mut World, source: &str) -> Result<(), String> {
        if !self.client_only {
            return self.run_command(world, source);
        }
        self.fire_changes(world);
        let result = self.client.run_command(world, source);
        self.collect(Side::Client);
        self.fire_changes(world);
        result
    }

    /// Whether `name` means something to the console's Lua here: a global,
    /// or one an earlier command set.
    pub fn console_knows(&self, world: &World, name: &str) -> bool {
        let side = if self.client_only {
            &self.client
        } else {
            &self.server
        };
        side.console_knows(world, name)
    }

    /// The commands scripts registered with `ConsoleService:RegisterCommand`
    /// (Phase 71): the LocalScripts', then the Scripts' (none on a device
    /// following a server elsewhere), each by name.
    pub fn console_commands(&self) -> Vec<ConsoleCommandInfo> {
        let mut commands = self.client.console_commands();
        if !self.client_only {
            commands.extend(self.server.console_commands());
        }
        commands
    }

    /// Runs `line`, typed in this device's console, as a registered
    /// command, for the player on this device: a LocalScript's first, then
    /// a Script's. [`ConsoleRun::Unknown`] when its first word isn't one.
    pub fn run_console_command(&mut self, world: &mut World, line: &str) -> ConsoleRun {
        let Some((name, args)) = split_command(line) else {
            return ConsoleRun::Unknown;
        };
        let player = self.local_player();
        self.fire_changes(world);
        let mut run = self
            .client
            .run_console_command(world, name, &args, player, false);
        self.collect(Side::Client);
        if run == ConsoleRun::Unknown && !self.client_only {
            run = self
                .server
                .run_console_command(world, name, &args, player, false);
            self.collect(Side::Server);
        }
        self.fire_changes(world);
        run
    }

    /// On the server: runs `line`, which `player` typed in their console on
    /// another device, as one of the Scripts' commands, when their `UserId`
    /// is in `ConsoleService.AllowedUserIds`.
    pub fn run_remote_console_command(
        &mut self,
        world: &mut World,
        player: InstanceId,
        line: &str,
    ) -> ConsoleRun {
        let Some((name, args)) = split_command(line) else {
            return ConsoleRun::Unknown;
        };
        self.fire_changes(world);
        let run = self
            .server
            .run_console_command(world, name, &args, Some(player), true);
        self.collect(Side::Server);
        self.fire_changes(world);
        run
    }
}
