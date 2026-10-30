-- START HERE: the arena's rules, on the server. Players join the team with
-- fewer players (the Teams service: Red and Blue) and spawn on its
-- SpawnLocation. Each character gets the Sword from ServerStorage (change
-- its Damage and Reach attributes there). A knockout counts on the
-- leaderboard (Tab) and in the chat, and the player respawns after
-- Players.RespawnTime seconds. Hit the TrainingDummy to practise.
local CollectionService = game:GetService("CollectionService")
local Players = game:GetService("Players")
local ServerStorage = game:GetService("ServerStorage")
local TextChatService = game:GetService("TextChatService")

local stats = require(game:GetService("ReplicatedStorage"):WaitForChild("ArenaStats"))

-- Everything a swing at `attacker`'s reach could hit: the other team's
-- characters and the training dummies.
local function targets(attacker)
	local list = {}
	for _, player in Players:GetPlayers() do
		local character = player.Character
		if player ~= attacker and character and character.Parent then
			if attacker.Neutral or player.Neutral or player.Team ~= attacker.Team then
				table.insert(list, character)
			end
		end
	end
	for _, dummy in CollectionService:GetTagged("Dummy") do
		table.insert(list, dummy)
	end
	return list
end

local function swing(player, sword)
	local character = player.Character
	if not character then
		return
	end
	local from = character:GetPivot().Position
	for _, target in targets(player) do
		local health = target:FindFirstChildWhichIsA("Health")
		local offset = target:GetPivot().Position - from
		if health and health.Health > 0 and offset.Magnitude <= sword:GetAttribute("Reach") then
			target:SetAttribute("LastHitBy", player.Name)
			health:TakeDamage(sword:GetAttribute("Damage"), character)
		end
	end
end

local function knockedOut(player, character)
	stats.add(player, "Deaths", 1)
	local by = Players:FindFirstChild(character:GetAttribute("LastHitBy") or "")
	if by and by ~= player then
		stats.add(by, "Kills", 1)
		TextChatService:DisplaySystemMessage(by.Name .. " knocked out " .. player.Name)
	else
		TextChatService:DisplaySystemMessage(player.Name .. " was knocked out")
	end
end

local function spawned(player, character)
	local sword = ServerStorage.Sword:Clone()
	sword.Activated:Connect(function()
		swing(player, sword)
	end)
	local inventory = character:WaitForChild("Inventory")
	inventory:Add(sword)
	inventory:Equip(sword)
	character:WaitForChild("Humanoid").Died:Connect(function()
		knockedOut(player, character)
	end)
end

local function joined(player)
	stats.start(player)
	local team = if player.Team then player.Team.Name else "no"
	TextChatService:DisplaySystemMessage(player.Name .. " joined the " .. team .. " team")
	player.CharacterAdded:Connect(function(character)
		spawned(player, character)
	end)
	if player.Character then
		spawned(player, player.Character)
	end
end

Players.PlayerAdded:Connect(joined)
for _, player in Players:GetPlayers() do
	joined(player)
end
