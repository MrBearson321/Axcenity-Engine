-- The grey Guard runs assets/ai/Guard.behaviortree.json (its NavigationAgent's
-- BehaviorTree; open it in the Behavior Tree tab): it patrols between
-- (-15, 0, -15) and (15, 0, -15), and chases you while it sees you. This
-- script only shows what it's doing: red while it sees you.
local agent = workspace.Guard.NavigationAgent
local body = workspace.Guard.Body

agent.Changed:Connect(function(property)
    if property == "SeesPlayer" then
        if agent.SeesPlayer then
            body.Color = Color3.new(0.85, 0.25, 0.25)
            print("Guard: there you are!")
        else
            body.Color = Color3.new(0.45, 0.45, 0.5)
            print("Guard: lost them. Back to my rounds.")
        end
    end
end)
