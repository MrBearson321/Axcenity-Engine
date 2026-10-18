-- Modules can require other modules.
local Palette = require(game.ReplicatedStorage.Palette)

local Builder = {}

-- Builds a staircase of `count` steps along X, and returns its parts.
function Builder.stairs(count)
    local steps = {}
    for n = 1, count do
        local step = Instance.new("Part")
        step.Name = "Step" .. n
        step.Anchored = true
        step.Size = Vector3.new(2, n, 4)
        step.Position = Vector3.new(n * 2 - count - 1, n / 2, 0)
        step.Color = Palette.color(n)
        step.Parent = workspace
        steps[n] = step
    end
    return steps
end

return Builder
