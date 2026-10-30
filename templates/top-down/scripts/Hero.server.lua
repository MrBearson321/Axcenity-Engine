-- START HERE: the hero and the rules. ClickToMove.client.lua sends where
-- you clicked; the Hero's NavigationAgent walks there round the walls. An
-- enemy close enough hurts you; at 0 health you go back to the start, and
-- reaching the green Goal is a win (player data, so the save slot keeps
-- it). The enemies' behavior trees are in assets/ai (open them in the
-- Behavior Tree tab), and Alarm.server.blocks.json turns an enemy red
-- when it sees you.
local CollectionService = game:GetService("CollectionService")
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local RunService = game:GetService("RunService")
local SaveService = game:GetService("SaveService")

local DAMAGE_PER_SECOND = 40
local CAUGHT_DISTANCE = 3.5

local hero = workspace.Hero
local body = hero.Body
local agent = hero.NavigationAgent
local health = hero.Health
local start = body.Position
local goal = workspace.Goal

ReplicatedStorage.MoveTo.OnServerEvent:Connect(function(_player, position)
	if typeof(position) == "Vector3" then
		agent.Destination = position
	end
end)

-- The HUD shows the hero's Status attribute and the player's Wins.
local function showWins()
	for _, player in Players:GetPlayers() do
		player:SetAttribute("Wins", SaveService:GetPlayerData("Wins") or 0)
	end
end
Players.PlayerAdded:Connect(showWins)
ReplicatedStorage:WaitForChild("GameLoaded").Event:Connect(showWins)
showWins()

local function backToStart(status)
	body.Position = start
	agent.Destination = Vector3.new(start.X, 0, start.Z)
	hero:SetAttribute("Status", status)
end

health.Died:Connect(function()
	backToStart("Caught! Back to the start.")
	health.Health = health.MaxHealth
end)

RunService.Heartbeat:Connect(function(deltaTime)
	for _, enemy in CollectionService:GetTagged("Enemy") do
		local offset = enemy.Body.Position - body.Position
		if Vector3.new(offset.X, 0, offset.Z).Magnitude < CAUGHT_DISTANCE then
			health:TakeDamage(DAMAGE_PER_SECOND * deltaTime, enemy)
		end
	end
	local toGoal = goal.Position - body.Position
	if math.abs(toGoal.X) < goal.Size.X / 2 and math.abs(toGoal.Z) < goal.Size.Z / 2 then
		SaveService:SetPlayerData("Wins", (SaveService:GetPlayerData("Wins") or 0) + 1)
		showWins()
		backToStart("You made it!")
	end
end)
