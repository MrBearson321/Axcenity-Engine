-- Particles: the campfire's Fire and Smoke emitters and the pad's Stars run
-- on their own (and show in the editor too). The pad's Sparks emitter is
-- switched off, and gives off a burst with Emit whenever a ball lands.

local pad = workspace.Pad

pad.Touched:Connect(function(otherPart)
    pad.Sparks:Emit(40)
    print(otherPart.Name .. " landed in a shower of sparks")
end)

-- Drop a ball onto the pad every two seconds.
while true do
    local ball = Instance.new("Part")
    ball.Name = "Ball"
    ball.Shape = Enum.PartType.Ball
    ball.Size = Vector3.new(1.5, 1.5, 1.5)
    ball.Color = Color3.fromRGB(255, 120, 60)
    ball.Position = Vector3.new(6, 12, 0)
    ball.Parent = workspace
    task.wait(2)
    ball.Parent = nil
end
