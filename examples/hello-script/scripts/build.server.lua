-- Runs once when the game starts. In world.json, Block sits in the middle of
-- the floor. This script moves it, then builds a row of glowing balls.

local block = workspace.Block
block.Position = block.Position + Vector3.new(-8, 2, 0)
block.Orientation = Vector3.new(0, 45, 0)
print("Moved " .. block.Name .. " to " .. tostring(block.Position))

for i = 1, 5 do
    local ball = Instance.new("Part")
    ball.Name = "Ball" .. i
    ball.Shape = Enum.PartType.Ball
    ball.Material = Enum.Material.Neon
    ball.Size = Vector3.new(2, 2, 2)
    ball.Position = Vector3.new(-4 + i * 2.5, 1, 4)
    ball.Color = Color3.fromRGB(40 * i, 255 - 40 * i, 200)
    ball.Parent = workspace
end
print("Built 5 balls")
