-- The blaster on this device: clicking (Primary) sends a shot along the
-- camera's view to Blaster.server.lua, Reload (R) fills it up again, and
-- the HUD shows the ammo and how many targets are down.
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local UserInputService = game:GetService("UserInputService")

local LocalGui = require(ReplicatedStorage:WaitForChild("LocalGui"))
local remote = ReplicatedStorage:WaitForChild("Blaster")
local hud = LocalGui.claim("Hud")
local player = Players.LocalPlayer
local camera = workspace.CurrentCamera

local blaster = workspace:WaitForChild("Character"):WaitForChild("Inventory"):WaitForChild("Blaster")

local function showAmmo()
	hud.Ammo.Text = blaster:GetAttribute("Ammo") .. " / " .. blaster:GetAttribute("ClipSize")
end
blaster:GetAttributeChangedSignal("Ammo"):Connect(showAmmo)
showAmmo()

local function showScore()
	hud.Score.Text = "Targets down: " .. (player:GetAttribute("TargetsDown") or 0)
end
player:GetAttributeChangedSignal("TargetsDown"):Connect(showScore)
showScore()

blaster.Activated:Connect(function()
	remote:FireServer("Fire", camera.CFrame.Position, camera.CFrame.LookVector)
end)

UserInputService.InputBegan:Connect(function(_input, gameProcessedEvent)
	if not gameProcessedEvent and UserInputService:IsActionPressed("Reload") then
		remote:FireServer("Reload")
	end
end)
