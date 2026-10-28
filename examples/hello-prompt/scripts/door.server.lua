-- Walk up to the door and hold E (X on a controller) for half a second to
-- open it; again to close it.
local door = workspace.Door
local prompt = door.ProximityPrompt
local open = false

prompt.Triggered:Connect(function(player)
	open = not open
	door.CanCollide = not open
	door.Transparency = if open then 0.8 else 0
	prompt.ActionText = if open then "Close" else "Open"
	print(player.Name, if open then "opened the door" else "closed the door")
end)

-- The chest is behind the wall: its prompt shows only from where the
-- chest can be seen, and only once.
local chest = workspace.Chest.Lid.ProximityPrompt
chest.Triggered:Connect(function()
	print("found 10 coins")
	chest.Enabled = false
end)
