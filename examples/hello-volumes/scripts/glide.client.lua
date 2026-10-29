-- A LocalScript. The cave on the right holds two GraphicsVolumes: CaveLook
-- (brighter exposure, cold blue fog, a cool tint and stronger bloom) and,
-- at the back, Embers (warm fog and tint, Priority 1, so it wins where they
-- overlap). Fly into the cave and each look fades in over its
-- BlendDistance, and out again when you leave. Press V to glide in and
-- out on your own; V again gives the camera back.

local RunService = game:GetService("RunService")
local UserInputService = game:GetService("UserInputService")

local camera = workspace.CurrentCamera
local outside = Vector3.new(0, 5, 0)
local inside = Vector3.new(36, 5, 0)
local gliding = false
local time = 0

RunService:BindToRenderStep("Glide", Enum.RenderPriority.Camera.Value, function(deltaTime)
    if not gliding then
        return
    end
    time += deltaTime
    -- In and out again every 12 seconds, easing at each end.
    local along = (1 - math.cos(time * math.pi / 6)) / 2
    local position = outside:Lerp(inside, along)
    camera.CFrame = CFrame.lookAt(position, position + Vector3.new(1, -0.25, 0))
end)

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode ~= Enum.KeyCode.V then
        return
    end
    gliding = not gliding
    if gliding then
        time = 0
        camera.CameraType = Enum.CameraType.Scriptable
    else
        camera.CameraType = Enum.CameraType.Custom
    end
end)

print("Fly into the cave, or press V to glide in and out.")
