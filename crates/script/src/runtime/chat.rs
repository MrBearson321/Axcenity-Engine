//! The host's side of TextChatService (Phase 67, `chat.rs`): messages typed
//! on this device, filtered and received from others, and the chat lines.

use crate::*;

impl ScriptRuntime {
    /// The chat window's Enter (Phase 67): `text` is sent as this device's
    /// player, as `TextChatService:SendMessage` does.
    pub fn send_chat(&mut self, text: &str) {
        if let Some(text) = chat::clean(text) {
            self.chat.0.borrow_mut().outgoing.push(text);
        }
    }

    /// Messages sent from this device since the last call, for the server.
    pub fn take_chat_sends(&mut self) -> Vec<String> {
        std::mem::take(&mut self.chat.0.borrow_mut().outgoing)
    }

    /// System messages Scripts showed since the last call, for every device.
    pub fn take_system_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.chat.0.borrow_mut().system)
    }

    /// The server's `OnIncomingMessage` decides what `player` sent: the text
    /// to send on (changed or not), or `None` to drop it.
    pub fn filter_chat(
        &mut self,
        world: &mut World,
        player: Option<InstanceId>,
        text: &str,
    ) -> Option<String> {
        self.fire_changes(world);
        let filtered = self.server.filter_chat(world, player, text);
        self.collect(Side::Server);
        self.fire_changes(world);
        filtered
    }

    /// A message arrived on this device (from `player`, or a system message
    /// for `None`): `MessageReceived` fires in the next step, and the chat
    /// window shows it.
    pub fn receive_chat(&mut self, player: Option<InstanceId>, text: String) {
        self.chat.0.borrow_mut().incoming.push((player, text));
    }

    /// The chat window's lines, oldest first.
    pub fn chat_lines(&self) -> Vec<ChatLine> {
        self.chat.0.borrow().lines.iter().cloned().collect()
    }

    /// Fires `MessageReceived` on both sides for each message that arrived,
    /// and adds it to the chat window's lines.
    pub(crate) fn fire_chat(&mut self, world: &mut World) {
        let incoming = std::mem::take(&mut self.chat.0.borrow_mut().incoming);
        for (player, text) in incoming {
            let from = player
                .and_then(|player| world.get(player))
                .map(|p| p.name().to_owned());
            {
                let mut chat = self.chat.0.borrow_mut();
                chat.lines.push_back(ChatLine {
                    from,
                    text: text.clone(),
                });
                while chat.lines.len() > chat::MAX_LINES {
                    chat.lines.pop_front();
                }
            }
            for side in [Side::Server, Side::Client] {
                self.context(side).fire_chat_message(world, player, &text);
                self.collect(side);
                self.fire_changes(world);
            }
        }
    }
}
