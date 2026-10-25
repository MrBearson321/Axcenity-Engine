-- A LocalScript: it runs on the player's side, so it can read the keyboard.
-- Hold Shift to run; let go to walk again. The ThirdPersonController moves
-- the character at RunSpeed or WalkSpeed, whichever Movement names.

local UserInputService = game:GetService("UserInputService")
local controller = workspace.Character.ThirdPersonController

local function isShift(input)
    return input.KeyCode == Enum.KeyCode.LeftShift or input.KeyCode == Enum.KeyCode.RightShift
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
UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.J then
        controller.Jump = true
    end
end)

print("WASD to walk, hold Shift to run, Space to jump, right-drag to turn the camera.")
