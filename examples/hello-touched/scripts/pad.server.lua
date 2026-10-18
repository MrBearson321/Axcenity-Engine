-- The Milestone 1 demo: a ball drops onto a pad. When something lands on the
-- pad, the pad changes color and glows for a moment, and the script prints
-- what landed. A bounce counts as landing again.

local pad = workspace.Pad
local colors = {
    Color3.fromRGB(80, 220, 100),
    Color3.fromRGB(80, 160, 255),
    Color3.fromRGB(255, 200, 60),
}
local landings = 0

-- `Connect` runs this function every time another part starts touching the
-- pad, and hands it that part.
local connection = pad.Touched:Connect(function(otherPart)
    landings = landings + 1
    print(otherPart.Name .. " touched the pad")
    pad.Color = colors[(landings - 1) % #colors + 1]
    pad.Material = Enum.Material.Neon
    -- Waiting here pauses only this handler, not the game or other scripts.
    task.wait(0.5)
    pad.Material = Enum.Material.Plastic
end)

-- Three seconds in, drop a second ball.
task.wait(3)
local ball = Instance.new("Part")
ball.Name = "BlueBall"
ball.Shape = Enum.PartType.Ball
ball.Size = Vector3.new(2, 2, 2)
ball.Color = Color3.fromRGB(60, 90, 230)
ball.Position = Vector3.new(1.5, 10, 1.5)
ball.Parent = workspace

-- Six seconds in, stop listening. A third ball lands, and nothing happens.
task.wait(3)
connection:Disconnect()
print("stopped listening for touches")

local last = Instance.new("Part")
last.Name = "IgnoredBall"
last.Shape = Enum.PartType.Ball
last.Size = Vector3.new(2, 2, 2)
last.Position = Vector3.new(-1.5, 10, -1.5)
last.Parent = workspace
