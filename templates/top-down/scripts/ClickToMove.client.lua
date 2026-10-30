-- Click to move: a click (Primary) on the ground sends the point under the
-- mouse to Hero.server.lua, and a ring marks it. The camera looks down on
-- the hero from CAMERA_OFFSET; change that for a closer or steeper view.
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local RunService = game:GetService("RunService")
local UserInputService = game:GetService("UserInputService")

local CAMERA_OFFSET = Vector3.new(0, 44, 22)

local moveTo = ReplicatedStorage:WaitForChild("MoveTo")
local hero = workspace:WaitForChild("Hero")
local body = hero:WaitForChild("Body")
local camera = workspace.CurrentCamera
local mouse = Players.LocalPlayer:GetMouse()

camera.CameraType = Enum.CameraType.Scriptable
RunService.RenderStepped:Connect(function()
	camera.CFrame = CFrame.lookAt(body.Position + CAMERA_OFFSET, body.Position)
end)

local marker = Instance.new("Part")
marker.Name = "MoveMarker"
marker.Shape = Enum.PartType.Cylinder
marker.Size = Vector3.new(0.2, 3, 3)
marker.Orientation = Vector3.new(0, 0, 90)
marker.Color = Color3.fromRGB(255, 255, 255)
marker.Material = Enum.Material.Neon
marker.Anchored = true
marker.CanCollide = false
marker.CanQuery = false
marker.CastShadow = false
marker.Transparency = 1
marker.Parent = workspace

UserInputService.InputBegan:Connect(function(_input, gameProcessedEvent)
	if gameProcessedEvent or not UserInputService:IsActionPressed("Primary") then
		return
	end
	if not mouse.Target then
		return
	end
	local point = mouse.Hit.Position
	moveTo:FireServer(point)
	marker.Position = point + Vector3.new(0, 0.15, 0)
	marker.Transparency = 0.3
end)
