-- XB10k for Roblox: builds the scene from ../README.md's formulas when the
-- server starts, then clones one script per spinner and blinker, as a
-- Roblox developer would place them. The timing is XB10kTimer (a
-- LocalScript in StarterPlayerScripts).
local Players = game:GetService("Players")
Players.CharacterAutoLoads = false -- no character: the other engines have none
workspace.Gravity = 196.2

local folder = script.Parent
local SIDE, SPACING, GRID, SPINNERS, DROPS, BLINKERS = 95, 4, 8899, 100, 1000, 97

local count, total = 0, 0
local function part(parent, name, shape, pos, size, color, anchored)
    local p = Instance.new("Part")
    p.Name = name
    p.Shape = Enum.PartType[shape]
    p.Size = size
    p.Position = pos
    p.Color = color
    p.Anchored = anchored
    p.Material = Enum.Material.SmoothPlastic
    p.TopSurface = Enum.SurfaceType.Smooth
    p.BottomSurface = Enum.SurfaceType.Smooth
    p.Parent = parent
    count += 1
    total += pos.X + pos.Y + pos.Z + size.X + size.Y + size.Z
    return p
end

local function makeFolder(name)
    local f = Instance.new("Folder")
    f.Name = name
    f.Parent = workspace
    return f
end

local start = os.clock()
local floor = part(workspace, "Floor", "Block", Vector3.new(0, -0.5, 0), Vector3.new(440, 1, 440),
    Color3.new(0.35, 0.37, 0.40), true)

local grid, spinners = makeFolder("Grid"), makeFolder("Spinners")
local shapes = { "Block", "Cylinder", "Wedge" }
for i = 0, GRID + SPINNERS - 1 do
    local c, r = i % SIDE, i // SIDE
    local x = c * SPACING - (SIDE - 1) * SPACING / 2
    local z = r * SPACING - (SIDE - 1) * SPACING / 2
    local h = 1 + (i * 7 % 5)
    local hue = (i % 8) / 8
    local color = Color3.new(hue, 1 - hue, 0.5 + hue / 2)
    local pos, size = Vector3.new(x, h / 2, z), Vector3.new(2, h, 2)
    if i >= GRID then
        local k = i - GRID
        part(spinners, "S" .. k, "Block", pos, size, color, true)
    else
        part(grid, "G" .. i, shapes[i % 3 + 1], pos, size, color, true)
    end
end

local drops = makeFolder("Drops")
for j = 0, DROPS - 1 do
    local x, y, z = j % 10, j // 100, (j // 10) % 10
    part(drops, "D" .. j, if j % 2 == 0 then "Ball" else "Block",
        Vector3.new(x * 3 - 13.5, 12 + y * 3, z * 3 - 13.5), Vector3.new(1.5, 1.5, 1.5),
        Color3.new(0.95, 0.6, 0.2), false)
end

-- Sun shining from (0.5, 1, 0.25): Roblox's sun follows ClockTime and
-- GeographicLatitude, which Axcenity copies; its defaults point the sun there.
local Lighting = game:GetService("Lighting")
Lighting.GlobalShadows = true

-- One script per spinner and blinker, plus the lifter.
for k = 0, SPINNERS - 1 do
    local s = folder.XB10kSpin:Clone()
    s.Name = "Spin" .. k
    s:SetAttribute("Index", k)
    s.Disabled = false
    s.Parent = game:GetService("ServerScriptService")
end
for k = 0, BLINKERS - 1 do
    local s = folder.XB10kBlink:Clone()
    s.Name = "Blink" .. k
    s:SetAttribute("Index", k)
    s.Disabled = false
    s.Parent = game:GetService("ServerScriptService")
end
local lifter = folder.XB10kDrops:Clone()
lifter.Disabled = false
lifter.Parent = game:GetService("ServerScriptService")

local buildMs = (os.clock() - start) * 1000
print(string.format("XB10K_CHECK count=%d sum=%.2f", count, total))
workspace:SetAttribute("XB10kBuildMs", buildMs)
workspace:SetAttribute("XB10kReady", true)
