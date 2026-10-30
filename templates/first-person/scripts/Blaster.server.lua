-- START HERE: the blaster. How hard it hits and how many shots it holds are
-- the Blaster Tool's attributes (Damage, ClipSize, Ammo): select
-- Workspace > Character > Inventory > Blaster and change them in
-- Properties. Blaster.client.lua sends each shot here, along the camera's
-- view; this Script, on the server, decides what it hits.
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local remote = ReplicatedStorage.Blaster

local RANGE = 300

local character = workspace.Character
local inventory = character.Inventory
local blaster = inventory.Blaster
inventory:Equip(blaster)

-- The Health of what a shot hit: the one in its Model, if it has one.
local function healthOf(part)
	local model = part:FindFirstAncestorWhichIsA("Model")
	return model and model:FindFirstChildWhichIsA("Health")
end

-- `Handle` is the Tool's own property (empty: its child part named Handle),
-- so the part is found by name.
local muzzle = blaster:FindFirstChild("Handle").Muzzle
local function flash()
	muzzle.Brightness = 4
	task.wait(0.05)
	muzzle.Brightness = 0
end

remote.OnServerEvent:Connect(function(_player, action, origin, direction)
	if action == "Reload" then
		blaster:SetAttribute("Ammo", blaster:GetAttribute("ClipSize"))
		return
	end
	if action ~= "Fire" or inventory.Equipped ~= blaster then
		return
	end
	local ammo = blaster:GetAttribute("Ammo")
	if ammo <= 0 then
		return
	end
	blaster:SetAttribute("Ammo", ammo - 1)
	task.spawn(flash)
	local params = RaycastParams.new({ FilterDescendantsInstances = { character } })
	local hit = workspace:Raycast(origin, direction.Unit * RANGE, params)
	local health = hit and healthOf(hit.Instance)
	if health then
		health:TakeDamage(blaster:GetAttribute("Damage"), character)
	end
end)
