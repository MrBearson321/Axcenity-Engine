-- Save and Load in the pause menu ask this Script, on the server, through
-- the RemoteFunction ReplicatedStorage.SaveGame that it makes. In the arena
-- they keep only the player's own Kills and Deaths (ArenaStats.module.lua),
-- in their data on the server: loading the whole world would move every
-- other player too.
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local stats = require(ReplicatedStorage:WaitForChild("ArenaStats"))

local loaded = Instance.new("BindableEvent")
loaded.Name = "GameLoaded"
loaded.Parent = ReplicatedStorage

local saveGame = Instance.new("RemoteFunction")
saveGame.Name = "SaveGame"
saveGame.OnServerInvoke = function(player, action)
	if action == "Save" then
		stats.save(player)
		return "Saved your stats"
	elseif action == "Load" then
		if not stats.load(player) then
			return "Nothing saved yet"
		end
		loaded:Fire()
		return "Loaded your stats"
	end
	return "Unknown action " .. tostring(action)
end
saveGame.Parent = ReplicatedStorage
