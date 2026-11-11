---
description: "The default chat (Phase 67): a chat window at the bottom left that / opens, styled with TextChatService rules (TextChatService::key styles the names)."
---

# TextChatService

```luau
game.TextChatService
```

The default chat (Phase 67): a chat window at the bottom left that `/` opens, styled with `TextChatService` rules (`TextChatService::key` styles the names). SendMessage sends from a LocalScript; on the server, OnIncomingMessage sees every message and returns its text, changed or not, or nil to drop it; DisplaySystemMessage shows a message from the game. MessageReceived fires on each device as a message arrives. ChatEnabled off hides the window.

It also has every member of [Instance](Instance.md).

### Properties

#### ChatEnabled

```luau
TextChatService.ChatEnabled: boolean
```

If false, the default chat window isn't drawn and `/` doesn't open it, for a game with its own chat or none. Scripts can still send and hear messages.

#### OnIncomingMessage

```luau
TextChatService.OnIncomingMessage: (message: { Text: string, Player: Player? }) -> string?
```

Scripts only (Phase 67): sees each message a player sends before anyone else does, and returns the text to send (changed or not), or nil to drop it: `TextChatService.OnIncomingMessage = function(message) return message.Text end`. It must return at once; an error keeps the message as it was. Scripts only.

### Events

#### MessageReceived

```luau
TextChatService.MessageReceived: Signal
```

Fires on each device as a chat message arrives there, with the message: `message.Text`, and `message.Player` (nil for a system message). On both sides, after the server's OnIncomingMessage.

### Functions

#### SendMessage

```luau
TextChatService:SendMessage(text: string): ()
```

LocalScripts only (Phase 67): sends `text` as this device's player, as the chat window's Enter does. The server's OnIncomingMessage may change or drop it; then every device's MessageReceived fires. LocalScripts only.

#### DisplaySystemMessage

```luau
TextChatService:DisplaySystemMessage(text: string): ()
```

Shows `text` in the chat as the game's, with no player's name: from a Script on every device, from a LocalScript on this one.

