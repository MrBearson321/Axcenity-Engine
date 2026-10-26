-- A LocalScript. The grey Guard's NavigationAgent sees 30 studs, 90°
-- across the way it faces (toward you, past the wall), and hears 25 studs,
-- through walls. You start hidden behind the wall. Walk out to the side
-- with A or D and it sees you; press F to whistle and it hears you, even
-- from behind the wall. listen.client.blocks.json hears the whistle from
-- blocks.

local UserInputService = game:GetService("UserInputService")
local agent = workspace.Guard.NavigationAgent
local body = workspace.Guard.Body

-- PlayerSensed fires with "Sight" or "Hearing" when one starts.
agent.PlayerSensed:Connect(function(sense)
    if sense == "Sight" then
        print("Guard: I see you!")
    else
        print("Guard: I hear you!")
    end
end)

-- SeesPlayer stays true while it sees you: the Guard is red until you hide.
agent.Changed:Connect(function(property)
    if property == "SeesPlayer" then
        if agent.SeesPlayer then
            body.Color = Color3.new(0.85, 0.25, 0.25)
        else
            body.Color = Color3.new(0.45, 0.45, 0.5)
            print("Guard: where did you go?")
        end
    end
end)

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.F then
        workspace.Character.Whistle:Play()
    end
end)
