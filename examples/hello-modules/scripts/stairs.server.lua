-- `require` runs a ModuleScript and returns what it returned. The file
-- `scripts/shared/Builder.module.lua` is `game.ReplicatedStorage.Builder`.
local Builder = require(game.ReplicatedStorage.Builder)

local steps = Builder.stairs(8)
print("Built " .. #steps .. " steps with the Builder module")
