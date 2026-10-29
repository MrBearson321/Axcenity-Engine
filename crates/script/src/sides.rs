//! The two sides of a game: Scripts run on the server, LocalScripts on the
//! player's side (the client). Each side has its own Lua interpreter, so
//! they share no globals, and they talk only through RemoteEvents.
//!
//! Some APIs only make sense on one side, such as reading the keyboard.
//! [`SIDE_ONLY_APIS`] lists them with a message for using one on the wrong
//! side. Scripts get that message as an error, and the editor shows it as a
//! warning before the game even runs.

use axcenity_core::ClassName;
use mlua::Lua;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// Where Scripts run. One per game.
    Server,
    /// Where LocalScripts run: the player's device.
    Client,
}

impl Side {
    /// The side a script of `class` runs on: Script on the server,
    /// LocalScript on the client. `None` for anything else.
    pub fn of_class(class: ClassName) -> Option<Side> {
        match class {
            ClassName::Script => Some(Side::Server),
            ClassName::LocalScript => Some(Side::Client),
            _ => None,
        }
    }

    /// The class of the scripts that run on this side.
    pub fn script_class(self) -> ClassName {
        match self {
            Side::Server => ClassName::Script,
            Side::Client => ClassName::LocalScript,
        }
    }

    fn other(self) -> Side {
        match self {
            Side::Server => Side::Client,
            Side::Client => Side::Server,
        }
    }

    fn place(self) -> &'static str {
        match self {
            Side::Server => "the server",
            Side::Client => "the player's side",
        }
    }
}

/// An API that only works on one side.
pub struct SideOnlyApi {
    /// What scripts write to use it, such as `UserInputService`.
    pub name: &'static str,
    /// The side it works on.
    pub side: Side,
    /// Finishes "... only works in a LocalScript, because".
    because: &'static str,
    /// What to do instead on the other side.
    instead: &'static str,
}

impl SideOnlyApi {
    /// What's wrong and what to do instead, for using it on the other side.
    pub fn message(&self) -> String {
        let wrong = self.side.other();
        format!(
            "{} only works in a {}, because {}. This is a {}, which runs on {}: {}.",
            self.name,
            self.side.script_class(),
            self.because,
            wrong.script_class(),
            wrong.place(),
            self.instead
        )
    }
}

/// Every API that only works on one side.
pub const SIDE_ONLY_APIS: &[SideOnlyApi] = &[
    SideOnlyApi {
        name: "UserInputService",
        side: Side::Client,
        because: "input comes from the player's keyboard",
        instead: "read the input in a LocalScript, and send it to the server with a RemoteEvent's FireServer",
    },
    SideOnlyApi {
        name: "ContextActionService",
        side: Side::Client,
        because: "its actions come from the player's keyboard, mouse and controllers",
        instead: "bind the action in a LocalScript, and send it to the server with a RemoteEvent's FireServer",
    },
    SideOnlyApi {
        name: "CurrentCamera",
        side: Side::Client,
        because: "each player has their own camera",
        instead: "move the camera from a LocalScript",
    },
    SideOnlyApi {
        name: "LocalPlayer",
        side: Side::Client,
        because: "it's the player whose device runs the script",
        instead: "OnServerEvent handlers get the player who sent the message as their first argument",
    },
    SideOnlyApi {
        name: "GetMouse",
        side: Side::Client,
        because: "the mouse belongs to the player's device",
        instead: "read the mouse in a LocalScript, and send what it points at to the server with a RemoteEvent's FireServer",
    },
    SideOnlyApi {
        name: "FireServer",
        side: Side::Client,
        because: "it sends a message from the player's side to the server",
        instead: "to reach LocalScripts, use FireClient or FireAllClients",
    },
    SideOnlyApi {
        name: "OnClientEvent",
        side: Side::Client,
        because: "it receives what Scripts send to the player",
        instead: "use OnServerEvent to receive what LocalScripts send",
    },
    SideOnlyApi {
        name: "InvokeServer",
        side: Side::Client,
        because: "it asks the server from the player's side",
        instead: "a Script can call a function on its own side directly, or use a BindableFunction's Invoke",
    },
    SideOnlyApi {
        name: "OnClientInvoke",
        side: Side::Client,
        because: "it answers what Scripts ask the player with InvokeClient",
        instead: "set OnServerInvoke to answer what LocalScripts ask",
    },
    SideOnlyApi {
        name: "RenderStepped",
        side: Side::Client,
        because: "frames are drawn on the player's screen",
        instead: "use RunService.Heartbeat or Stepped, which fire every frame on both sides",
    },
    SideOnlyApi {
        name: "BindToRenderStep",
        side: Side::Client,
        because: "frames are drawn on the player's screen",
        instead: "connect to RunService.Heartbeat or Stepped, which fire every frame on both sides",
    },
    SideOnlyApi {
        name: "UnbindFromRenderStep",
        side: Side::Client,
        because: "functions are only bound to the render step on the player's side",
        instead: "disconnect the Heartbeat or Stepped connection instead",
    },
    SideOnlyApi {
        name: "UserGameSettings",
        side: Side::Client,
        because: "graphics settings belong to the player's computer",
        instead: "change them from a LocalScript, such as one behind a settings menu",
    },
    SideOnlyApi {
        name: "RemoveDefaultLoadingScreen",
        side: Side::Client,
        because: "the loading screen is on the player's screen",
        instead: "send a RemoteEvent to the player with FireClient, and call it from a LocalScript that listens",
    },
    SideOnlyApi {
        name: "FireClient",
        side: Side::Server,
        because: "it sends a message from the server to a player",
        instead: "to reach Scripts, use FireServer",
    },
    SideOnlyApi {
        name: "FireAllClients",
        side: Side::Server,
        because: "it sends a message from the server to every player",
        instead: "to reach Scripts, use FireServer",
    },
    SideOnlyApi {
        name: "InvokeClient",
        side: Side::Server,
        because: "it asks a player's device from the server",
        instead: "a LocalScript can call a function on its own side directly, or use a BindableFunction's Invoke",
    },
    SideOnlyApi {
        name: "OnServerInvoke",
        side: Side::Server,
        because: "it answers what LocalScripts ask with InvokeServer",
        instead: "call InvokeServer from a LocalScript to ask the server",
    },
    SideOnlyApi {
        name: "LoadCharacter",
        side: Side::Server,
        because: "the server gives each player their character",
        instead: "ask the server with a RemoteEvent's FireServer, and call it there",
    },
    SideOnlyApi {
        name: "Kick",
        side: Side::Server,
        because: "only the server can take a player out of the game",
        instead: "ask the server with a RemoteEvent's FireServer, and call it there",
    },
    SideOnlyApi {
        name: "SetNetworkOwner",
        side: Side::Server,
        because: "the server decides which device simulates each part",
        instead: "ask the server with a RemoteEvent's FireServer, and call it there",
    },
    SideOnlyApi {
        name: "GetNetworkOwner",
        side: Side::Server,
        because: "the server decides which device simulates each part",
        instead: "ask the server with a RemoteFunction's InvokeServer",
    },
    SideOnlyApi {
        name: "SetNetworkOwnershipAuto",
        side: Side::Server,
        because: "the server decides which device simulates each part",
        instead: "ask the server with a RemoteEvent's FireServer, and call it there",
    },
    SideOnlyApi {
        name: "GetNetworkOwnershipAuto",
        side: Side::Server,
        because: "the server decides which device simulates each part",
        instead: "ask the server with a RemoteFunction's InvokeServer",
    },
    SideOnlyApi {
        name: "OnServerEvent",
        side: Side::Server,
        because: "it receives what LocalScripts send to the server",
        instead: "use OnClientEvent to receive what Scripts send",
    },
];

/// The side-only API called `name`, if it is one.
pub fn side_only(name: &str) -> Option<&'static SideOnlyApi> {
    SIDE_ONLY_APIS.iter().find(|api| api.name == name)
}

/// Fails with the API's message if `name` is a side-only API for the other
/// side than the one `lua` runs. Every other name passes.
pub(crate) fn check(lua: &Lua, name: &str) -> mlua::Result<()> {
    let Some(api) = side_only(name) else {
        return Ok(());
    };
    // Each interpreter keeps its side in its app data (see `context.rs`).
    let side = lua.app_data_ref::<Side>().map(|side| *side);
    if side == Some(api.side) {
        Ok(())
    } else {
        Err(mlua::Error::runtime(api.message()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_both_kinds_of_script_and_what_to_do() {
        let message = side_only("UserInputService").unwrap().message();
        assert_eq!(
            message,
            "UserInputService only works in a LocalScript, because input comes from the \
             player's keyboard. This is a Script, which runs on the server: read the input in \
             a LocalScript, and send it to the server with a RemoteEvent's FireServer."
        );
        let message = side_only("OnServerEvent").unwrap().message();
        assert!(
            message.starts_with("OnServerEvent only works in a Script")
                && message.contains("This is a LocalScript, which runs on the player's side"),
            "{message}"
        );
    }
}
