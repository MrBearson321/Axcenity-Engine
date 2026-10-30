-- Save and Load in the pause menu ask this Script, on the server, through
-- the RemoteFunction ReplicatedStorage.SaveGame that it makes. A slot keeps
-- the Workspace as it is, the player data (SaveService:SetPlayerData) and
-- the rebound keys. Slots are kept in your data for this game, never in the
-- project folder.
--
-- Other Scripts can listen to ReplicatedStorage.GameLoaded (a
-- BindableEvent) to put things back after a load, such as a HUD's numbers.
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local SaveService = game:GetService("SaveService")
local settings = require(ReplicatedStorage:WaitForChild("MenuSettings"))

local loaded = Instance.new("BindableEvent")
loaded.Name = "GameLoaded"
loaded.Parent = ReplicatedStorage

local saveGame = Instance.new("RemoteFunction")
saveGame.Name = "SaveGame"
saveGame.OnServerInvoke = function(_player, action)
	local slot = settings.Slot
	if action == "Save" then
		SaveService:SaveSlot(slot)
		return "Saved to slot " .. slot
	elseif action == "Load" then
		if not SaveService:HasSlot(slot) then
			return "Nothing saved yet"
		end
		SaveService:LoadSlot(slot)
		loaded:Fire()
		return "Loaded slot " .. slot
	end
	return "Unknown action " .. tostring(action)
end
saveGame.Parent = ReplicatedStorage
