-- The yellow Courier has no behavior tree: this script walks it. Each time
-- it gets there, it's sent to the other end.
local agent = workspace.Courier.NavigationAgent
local stops = { Vector3.new(30, 0, 30), Vector3.new(-30, 0, 30) }
local stop = 1

agent.Destination = stops[stop]
agent.Changed:Connect(function(property)
    if property == "Status" and agent.Status == "Reached" then
        stop = stop % #stops + 1
        agent.Destination = stops[stop]
    end
end)
