-- A LocalScript: it runs on the player's side, so it can read the keyboard.
-- Hold Shift to run; let go to walk again. The ThirdPersonController moves
-- the character at RunSpeed or WalkSpeed, whichever Movement names. Shift
-- leaves a custom mode, such as Hover (hover.client.lua), alone.

local UserInputService = game:GetService("UserInputService")
local controller = workspace.Character.ThirdPersonController

local function isShift(input)
    local shift = input.KeyCode == Enum.KeyCode.LeftShift or input.KeyCode == Enum.KeyCode.RightShift
    return shift and controller.Movement ~= "Hover"
end

UserInputService.InputBegan:Connect(function(input)
    if isShift(input) then
        controller.Movement = "Run"
    end
end)

UserInputService.InputEnded:Connect(function(input)
    if isShift(input) then
        controller.Movement = "Walk"
    end
end)

-- J jumps from Lua. Space jumps too, without a script.
-- K crouches or stands from Lua. Holding C crouches too, without a script.
UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.J then
        controller.Jump = true
    elseif input.KeyCode == Enum.KeyCode.K then
        controller.Crouch = not controller.Crouch
    end
end)

print("WASD to walk, hold Shift to run, Space to jump, C to crouch, H to hover, right-drag to turn the camera.")
