-- A LocalScript. The Guard's NavigationAgent has its Destination set in
-- Properties, on the yellow Goal behind the wall, so Play walks it round
-- the wall's end. P sends it somewhere else from Lua; hold V to push it
-- straight at a Velocity, with no path. go.client.blocks.json does the
-- same from blocks with 1 to 4.

local UserInputService = game:GetService("UserInputService")
local agent = workspace.Guard.NavigationAgent

-- Status says how it's getting on: Moving, Reached, Unreachable or Idle.
agent.Changed:Connect(function(property)
    if property == "Status" then
        print("Guard: " .. agent.Status)
    end
end)

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.P then
        agent.Destination = Vector3.new(16, 0, -16)
    elseif input.KeyCode == Enum.KeyCode.V then
        agent.Velocity = Vector3.new(0, 0, -8)
    end
end)

UserInputService.InputEnded:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.V then
        agent.Velocity = Vector3.new(0, 0, 0)
    end
end)
