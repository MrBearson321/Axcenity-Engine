-- A LocalScript: the camera flies over the scene when the game starts,
-- then gives the view back to the character. The "Beacon" tag stays over
-- the beacon wherever the camera goes, a click names the part under the
-- mouse, and E shakes the camera.

local TweenService = game:GetService("TweenService")
local RunService = game:GetService("RunService")
local UserInputService = game:GetService("UserInputService")

local camera = workspace.CurrentCamera
local beacon = workspace.Beacon
local tag = workspace.Hud.Tag

-- The flyover: from a corner, to the Overlook viewpoint (a Camera placed
-- in the world), to the other side, a second each.
local path = {
    CFrame.lookAt(Vector3.new(30, 20, 30), beacon.Position),
    workspace.Overlook.CFrame,
    CFrame.lookAt(Vector3.new(-30, 12, 10), beacon.Position),
}

local function flyTo(index)
    if index > #path then
        camera.CameraType = Enum.CameraType.Custom
        print("Flyover done: the character has the camera again")
        return
    end
    local info = TweenInfo.new(1, Enum.EasingStyle.Linear)
    local tween = TweenService:Create(camera, info, { CFrame = path[index] })
    tween.Completed:Connect(function()
        flyTo(index + 1)
    end)
    tween:Play()
end

camera.CameraType = Enum.CameraType.Scriptable
camera.CFrame = path[1]
flyTo(2)

-- Keep the tag centred just above the beacon, and hide it when the
-- beacon is behind the camera or off the screen.
RunService.Heartbeat:Connect(function()
    local top = beacon.Position + Vector3.new(0, beacon.Size.Y / 2 + 1, 0)
    local at, onScreen = camera:WorldToScreenPoint(top)
    tag.Position = UDim2.fromOffset(at.X - 60, at.Y - 14)
    tag.Visible = onScreen
end)

UserInputService.InputBegan:Connect(function(input, processed)
    if processed then
        return
    end
    if input.UserInputType == Enum.UserInputType.MouseButton1 then
        local part = game.Players.LocalPlayer:GetMouse().Target
        print("Under the mouse: " .. (part and part.Name or "nothing"))
    elseif input.KeyCode == Enum.KeyCode.E then
        camera:Shake(1, 0.6)
    end
end)
