-- Phase 75: decals. Press 1 to take out the Blaster and click to fire: each
-- shot leaves a bullet hole where it hits, which fades out after 30
-- seconds. The Workspace keeps at most MaxSpawnedDecals of them; past that
-- the oldest goes first. The road marking over the hill and the puddle are
-- ProjectedDecals placed in the editor, and the sign's arrow is a Decal on
-- its front face.
local character = workspace:WaitForChild("Character")
local body = character:WaitForChild("Body")
local blaster = character:WaitForChild("Inventory"):WaitForChild("Blaster")
local random = Random.new(75)

local params = RaycastParams.new()
params.FilterType = Enum.RaycastFilterType.Exclude
params.FilterDescendantsInstances = { character }

local function shoot(from, direction)
	local hit = workspace:Raycast(from, direction * 200, params)
	if hit then
		workspace:SpawnDecal("assets/textures/hole.png",
			CFrame.lookAt(hit.Position, hit.Position + hit.Normal), 0.6, 30)
	end
end

blaster.Activated:Connect(function()
	-- Forward from the chest, scattered a little.
	local spread = Vector3.new(random:NextNumber(-0.15, 0.15), random:NextNumber(-0.1, 0.1), 0)
	local look = body.CFrame.LookVector
	shoot(body.Position + Vector3.new(0, 1, 0), (look + spread).Unit)
end)

-- A few holes already in the target.
for i = 1, 6 do
	shoot(Vector3.new(-6 + i * 1.7, 1.5 + (i % 3), 0), Vector3.new(0, 0, -1))
end
