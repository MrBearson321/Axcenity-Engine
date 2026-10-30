-- The HUD on this device: how many coins you have, from your leaderstats.
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")

local LocalGui = require(ReplicatedStorage:WaitForChild("LocalGui"))
local hud = LocalGui.claim("Hud")
local coins = Players.LocalPlayer:WaitForChild("leaderstats"):WaitForChild("Coins")

local function show()
	hud.Coins.Text = "Coins: " .. coins.Value
end
coins.Changed:Connect(show)
show()
