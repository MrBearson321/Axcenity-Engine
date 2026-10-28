-- The lever beside the start: hold E to open the Dungeon beside the
-- Meadow (AddLevel puts it in a Model called Dungeon), and again to close it.
-- A level that's loaded again has a new lever, so it's hooked up again.
local LevelService = game:GetService("LevelService")

local function hookUp()
	local lever = workspace:FindFirstChild("Lever")
	if not lever then
		return
	end
	lever.ProximityPrompt.Triggered:Connect(function()
		if workspace:FindFirstChild("Dungeon") then
			LevelService:RemoveLevel("Dungeon")
		else
			LevelService:AddLevel("Dungeon")
		end
	end)
end

hookUp()
LevelService.LevelLoaded:Connect(hookUp)
