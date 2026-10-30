-- The HUD on this device: your health and your team.
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local RunService = game:GetService("RunService")

local LocalGui = require(ReplicatedStorage:WaitForChild("LocalGui"))
local hud = LocalGui.claim("Hud")
local player = Players.LocalPlayer

RunService.Heartbeat:Connect(function()
	local character = player.Character
	local health = character and character:FindFirstChildWhichIsA("Health")
	hud.Health.Text = if health then "Health " .. math.ceil(health.Health) else "Respawning..."
	hud.Team.Text = if player.Team then player.Team.Name .. " team" else ""
end)
