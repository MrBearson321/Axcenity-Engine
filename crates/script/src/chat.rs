//! The default chat (Phase 67): `TextChatService`.
//!
//! - `TextChatService:SendMessage(text)` (LocalScripts, and the chat
//!   window when the player presses Enter) sends a message as the player
//!   on this device. The host takes it with `take_chat_sends` and hands it
//!   to the server.
//! - On the server, `TextChatService.OnIncomingMessage = function(message)`
//!   sees each message (`message.Text`, `message.Player`) and returns the
//!   text to send, changed or not, or nil to drop it (`filter_chat`). It
//!   must return at once; an error keeps the message as it was.
//! - `TextChatService:DisplaySystemMessage(text)` from a Script shows a
//!   system message on every device; from a LocalScript, on this one.
//! - Every message that arrives on a device fires `MessageReceived(message)`
//!   on both sides there, and goes in the chat window's lines.
//! - `ChatEnabled` (a property, so the server's setting reaches everyone)
//!   off hides the window and `/` doesn't open it; scripts still chat.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use axcenity_core::{ClassName, InstanceId};
use mlua::{Function, Lua};

use crate::Side;
use crate::instance::LuaInstance;

/// The chat window keeps this many lines.
pub const MAX_LINES: usize = 50;
/// The longest message, in characters; longer ones are cut.
pub const MAX_LENGTH: usize = 200;

/// A line of the chat window: who said it (`None` for a system message)
/// and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    pub from: Option<String>,
    pub text: String,
}

/// Messages on their way, shared by both sides of a runtime.
#[derive(Default)]
pub struct Chat {
    /// Sent from this device, for the server.
    pub(crate) outgoing: Vec<String>,
    /// System messages a Script showed, for every device.
    pub(crate) system: Vec<String>,
    /// Arrived on this device, to fire `MessageReceived` for: who sent
    /// each (a Player, or none for a system message) and its text.
    pub(crate) incoming: Vec<(Option<InstanceId>, String)>,
    /// The chat window's lines, oldest first.
    pub(crate) lines: VecDeque<ChatLine>,
}

#[derive(Clone, Default)]
pub struct SharedChat(pub Rc<RefCell<Chat>>);

/// `text` as a message: trimmed, and cut at [`MAX_LENGTH`] characters.
/// `None` when nothing is left.
pub fn clean(text: &str) -> Option<String> {
    let text: String = text.trim().chars().take(MAX_LENGTH).collect();
    (!text.is_empty()).then_some(text)
}

fn chat(lua: &Lua) -> mlua::Result<Rc<RefCell<Chat>>> {
    lua.app_data_ref::<SharedChat>()
        .map(|shared| shared.0.clone())
        .ok_or_else(|| mlua::Error::runtime("the chat isn't ready"))
}

pub fn methods(lua: &Lua) -> mlua::Result<Vec<(ClassName, &'static str, Function)>> {
    let send = lua.create_function(|lua, (_, text): (LuaInstance, String)| {
        crate::sides::check(lua, "SendMessage")?;
        if let Some(text) = clean(&text) {
            chat(lua)?.borrow_mut().outgoing.push(text);
        }
        Ok(())
    })?;
    let system = lua.create_function(|lua, (_, text): (LuaInstance, String)| {
        let Some(text) = clean(&text) else {
            return Ok(());
        };
        let chat = chat(lua)?;
        let mut chat = chat.borrow_mut();
        match lua.app_data_ref::<Side>().map(|side| *side) {
            Some(Side::Server) => chat.system.push(text),
            _ => chat.incoming.push((None, text)),
        }
        Ok(())
    })?;
    Ok(vec![
        (ClassName::TextChatService, "SendMessage", send),
        (ClassName::TextChatService, "DisplaySystemMessage", system),
    ])
}

/// The message table `MessageReceived` and `OnIncomingMessage` get.
pub fn message_table(
    lua: &Lua,
    player: Option<InstanceId>,
    text: &str,
) -> mlua::Result<mlua::Table> {
    let table = lua.create_table()?;
    table.set("Text", text)?;
    table.set("Player", player.map(LuaInstance))?;
    Ok(table)
}
