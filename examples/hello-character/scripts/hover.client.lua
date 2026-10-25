-- A LocalScript: H hovers. "Hover" is a custom movement mode, so the
-- ThirdPersonController stops walking, jumping, crouching and falling, and
-- this script moves the character instead. H again clears the mode, and the
-- character walks or runs as it did before. 6 and 7 do the same from blocks.

local RunService = game:GetService("RunService")
local UserInputService = game:GetService("UserInputService")
local controller = workspace.Character.ThirdPersonController
local body = workspace.Character.Body

local SPEED = 12

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.H then
        if controller.Movement == "Hover" then
            controller.Movement = ""
        else
            controller.Movement = "Hover"
        end
    end
end)

-- While hovering, E rises, Q sinks, and W goes where the camera looks.
RunService.Heartbeat:Connect(function(seconds)
    if controller.Movement ~= "Hover" then
        return
    end
    local move = Vector3.new(0, 0, 0)
    if UserInputService:IsKeyDown(Enum.KeyCode.E) then
        move = move + Vector3.new(0, 1, 0)
    end
    if UserInputService:IsKeyDown(Enum.KeyCode.Q) then
        move = move - Vector3.new(0, 1, 0)
    end
    if UserInputService:IsKeyDown(Enum.KeyCode.W) then
        local look = workspace.CurrentCamera.LookVector
        local ahead = Vector3.new(look.X, 0, look.Z)
        if ahead.Magnitude > 0 then
            move = move + ahead.Unit
        end
    end
    body.Position = body.Position + move * (SPEED * seconds)
end)
