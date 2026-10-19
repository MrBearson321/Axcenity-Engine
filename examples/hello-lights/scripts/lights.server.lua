-- Point and spot lights. The Lamp and the Spotlight in the world each have a
-- light inside them; this script adds a grid of 120 small glowing balls,
-- each with its own PointLight, then animates them all every frame.

local RunService = game:GetService("RunService")

-- A color from a hue (0 to 1, round the rainbow).
local function rainbow(hue)
    local function channel(offset)
        local value = math.abs((hue * 6 + offset) % 6 - 3) - 1
        return math.min(math.max(value, 0), 1)
    end
    return Color3.new(channel(0), channel(4), channel(2))
end

local lights = {}
for row = 1, 10 do
    for column = 1, 12 do
        local ball = Instance.new("Part")
        ball.Name = "Glow" .. row .. "_" .. column
        ball.Shape = Enum.PartType.Ball
        ball.Anchored = true
        ball.Size = Vector3.new(0.6, 0.6, 0.6)
        ball.Material = Enum.Material.Neon
        ball.Position = Vector3.new(column * 5 - 32.5, 0.5, row * 5 - 40)

        local light = Instance.new("PointLight")
        light.Range = 6
        light.Brightness = 3
        light.Parent = ball
        ball.Parent = workspace
        table.insert(lights, { ball = ball, light = light, hue = (row + column) / 22 })
    end
end
print("Added " .. #lights .. " PointLights.")

local spot = workspace.Spotlight.SpotLight
local time = 0
RunService.Heartbeat:Connect(function(deltaTime)
    time = time + deltaTime
    -- The colors run round the rainbow.
    for _, glow in ipairs(lights) do
        local color = rainbow((glow.hue + time * 0.1) % 1)
        glow.ball.Color = color
        glow.light.Color = color
    end
    -- The spotlight sweeps from side to side.
    spot.Direction = Vector3.new(math.sin(time) * 0.6, -1, 0)
end)
