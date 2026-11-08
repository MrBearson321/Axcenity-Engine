-- A town of Models spread over 4 km. Raise PARTS to 50000 to measure a big world.
local PARTS = 8000
local PER_MODEL = 8
local SIZE = 2000

local rows = math.floor(math.sqrt(PARTS))
local step = 2 * SIZE / rows
local model
for x = 0, rows - 1 do
	for z = 0, rows - 1 do
		if (x + z) % PER_MODEL == 0 then
			model = Instance.new("Model")
			model.Name = "House"
			model.Parent = workspace
		end
		local part = Instance.new("Part")
		part.Anchored = true
		part.Size = Vector3.new(6, 4 + (x * z) % 9, 6)
		part.Position = Vector3.new(-SIZE + x * step, part.Size.Y / 2, -SIZE + z * step)
		part.Color = Color3.fromHSV((x + z) % 24 / 24, 0.5, 0.9)
		part.Parent = model
	end
end
print("Built", rows * rows, "parts")

-- A teleport that lands on loaded ground: ask for the region first.
game.Players.PlayerAdded:Connect(function(player)
	while not player.Character do
		task.wait()
	end
	task.wait(4)
	local there = Vector3.new(1500, 3, 1500)
	player:RequestStreamAroundAsync(there)
	player.Character:PivotTo(CFrame.new(there))
	print("Teleported", player.Name, "to the far corner")
end)
