-- The HUD on this device: the hero's health, your wins, and what just
-- happened.
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")

local LocalGui = require(ReplicatedStorage:WaitForChild("LocalGui"))
local hud = LocalGui.claim("Hud")
local player = Players.LocalPlayer
local hero = workspace:WaitForChild("Hero")
local health = hero:WaitForChild("Health")

local function showHealth()
	hud.Health.Text = "Health " .. math.ceil(health.Health)
end
health.HealthChanged:Connect(showHealth)
showHealth()

local function showWins()
	hud.Wins.Text = "Wins: " .. (player:GetAttribute("Wins") or 0)
end
player:GetAttributeChangedSignal("Wins"):Connect(showWins)
showWins()

hero:GetAttributeChangedSignal("Status"):Connect(function()
	hud.Status.Text = hero:GetAttribute("Status") or ""
	task.wait(3)
	hud.Status.Text = ""
end)
