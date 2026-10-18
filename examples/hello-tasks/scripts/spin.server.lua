-- `task.spawn` starts a function that runs alongside the rest of this script.
-- `task.wait()` with no time waits one frame and returns how long that took,
-- so the spinner turns at the same speed at any frame rate.

local spinner = workspace.Spinner
local DEGREES_PER_SECOND = 90

task.spawn(function()
    while true do
        local seconds = task.wait()
        local angle = (spinner.Orientation.Y + DEGREES_PER_SECOND * seconds) % 360
        spinner.Orientation = Vector3.new(0, angle, 0)
    end
end)

-- `task.delay` runs a function later, without pausing this script.
task.delay(3, function()
    local ball = Instance.new("Part")
    ball.Name = "Ball"
    ball.Shape = Enum.PartType.Ball
    ball.Material = Enum.Material.Metal
    ball.Color = Color3.fromRGB(230, 230, 240)
    ball.Size = Vector3.new(2, 2, 2)
    ball.Position = Vector3.new(3, 4, 0)
    ball.Parent = workspace
    print("a ball appeared after 3 seconds")
end)

print("spin.server.lua reached its last line, but the spinner keeps turning")
