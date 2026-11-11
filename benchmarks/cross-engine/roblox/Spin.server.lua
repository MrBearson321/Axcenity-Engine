-- XB10k: turns one spinner every Heartbeat.
local RunService = game:GetService("RunService")
local k = script:GetAttribute("Index")
local part = workspace.Spinners:WaitForChild("S" .. k)
local speed = 30 + k % 7 * 20
local angle = 0
RunService.Heartbeat:Connect(function(deltaTime)
    angle = (angle + speed * deltaTime) % 360
    part.Orientation = Vector3.new(0, angle, 0)
end)
