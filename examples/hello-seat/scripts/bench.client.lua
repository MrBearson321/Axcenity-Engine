local UserInputService = game:GetService("UserInputService")

-- A LocalScript. The grey Bench is Disabled, so walking into it doesn't sit
-- the character; B sits it there from Lua. L stands it up from any seat.
-- Space stands it up too, without a script.

local controller = workspace.Character.ThirdPersonController

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.B then
        workspace.Bench:Sit(controller)
    elseif input.KeyCode == Enum.KeyCode.L then
        controller.Sit = false
    end
end)

controller.Seated:Connect(function(active, seat)
    if active then
        print("Sat on " .. seat.Name)
    else
        print("Stood up from " .. seat.Name)
    end
end)
