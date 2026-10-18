-- LocalScripts can require modules too. The player's side runs its own
-- copy of the module, so changing the table here doesn't change the server's.
local Palette = require(game.ReplicatedStorage.Palette)

print("The player's side has its own Palette, with " .. #Palette.colors .. " colors")
