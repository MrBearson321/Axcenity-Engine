-- The HUD on this device: the lap time, your best and its medal, the next
-- gate, and the medal times from the data table.
local DataService = game:GetService("DataService")
local ReplicatedStorage = game:GetService("ReplicatedStorage")

local LocalGui = require(ReplicatedStorage:WaitForChild("LocalGui"))
local hud = LocalGui.claim("Hud")
local car = workspace:WaitForChild("Car")
local medals = DataService:GetTable("medals")
local gateCount = #workspace:WaitForChild("Gates"):GetChildren()

local function clock(seconds)
	return string.format("%d:%04.1f", seconds // 60, seconds % 60)
end

local function show()
	hud.Lap.Text = "Lap " .. clock(car:GetAttribute("LapTime") or 0)
	local best = car:GetAttribute("BestLap") or 0
	local medal = car:GetAttribute("Medal") or ""
	hud.Best.Text = if best > 0 then "Best " .. clock(best) .. "  " .. medal else "Best -"
	hud.Gates.Text = "Next gate " .. (car:GetAttribute("NextGate") or 1) .. " of " .. gateCount
end
car.AttributeChanged:Connect(show)
show()

local times = {}
for _, key in medals:Keys() do
	local row = medals:Get(key)
	table.insert(times, row.Title .. " " .. clock(row.Seconds))
end
hud.Medals.Text = table.concat(times, "   ")
