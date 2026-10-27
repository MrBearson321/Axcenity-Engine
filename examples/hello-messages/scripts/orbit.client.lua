-- Circles the camera around the tower, just before each frame is drawn.
-- "Turn" is bound second but runs first: lower priorities run earlier.
local RunService = game:GetService("RunService")
local camera = workspace.CurrentCamera
camera.CameraType = Enum.CameraType.Scriptable

local angle = 0

RunService:BindToRenderStep("Orbit", Enum.RenderPriority.Camera.Value, function()
    local position = Vector3.new(math.sin(angle) * 20, 10, math.cos(angle) * 20)
    camera.CFrame = CFrame.lookAt(position, Vector3.new(0, 3, 0))
end)

RunService:BindToRenderStep("Turn", Enum.RenderPriority.Input.Value, function(deltaTime)
    angle += deltaTime * 0.5
end)
