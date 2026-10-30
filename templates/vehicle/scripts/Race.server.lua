-- START HERE: the race. Drive through the gates in order (Gates > Gate1 is
-- the white start line) and back over the start line for a lap. The medal
-- times are the data table assets/data/medals.data.json: open it in the
-- Data Table tab to change them. The car's attributes (LapTime, LastLap,
-- BestLap, Medal, NextGate) are what the HUD shows.
local DataService = game:GetService("DataService")
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local RunService = game:GetService("RunService")
local SaveService = game:GetService("SaveService")

local car = workspace.Car
local chassis = car.Chassis
local seat = car.DriverSeat
local prompt = seat.DrivePrompt
local medals = DataService:GetTable("medals")

local gates = {}
for index = 1, #workspace.Gates:GetChildren() do
	gates[index] = workspace.Gates["Gate" .. index].Sensor
end

-- The seat is Disabled, so walking into it doesn't sit you down: the
-- prompt does. Seat.server.blocks.json turns the prompt off as you sit,
-- and this turns it back on when you get out (Space).
prompt.Triggered:Connect(function(player)
	local character = player.Character
	local controller = character and character:FindFirstChildWhichIsA("ThirdPersonController", true)
	if controller then
		seat:Sit(controller)
	end
end)
seat:GetPropertyChangedSignal("Occupant"):Connect(function()
	if seat.Occupant == nil then
		prompt.Enabled = true
	end
end)

-- The medal a lap of `seconds` earns: the first row it's within.
local function medalFor(seconds)
	for _, key in medals:Keys() do
		local row = medals:Get(key)
		if seconds <= row.Seconds then
			return row.Title
		end
	end
	return ""
end

local function showBest()
	car:SetAttribute("BestLap", SaveService:GetPlayerData("BestLap") or 0)
end
showBest()
ReplicatedStorage:WaitForChild("GameLoaded").Event:Connect(showBest)

local clock = 0 -- game time, from each frame's length
local started = nil -- when the lap began, or nil before the first start line
local nextGate = 1
local inside = {}
car:SetAttribute("NextGate", nextGate)

local function crossed(index)
	if index ~= nextGate then
		return
	end
	if index == 1 then
		local now = clock
		if started then
			local lap = now - started
			car:SetAttribute("LastLap", lap)
			car:SetAttribute("Medal", medalFor(lap))
			local best = SaveService:GetPlayerData("BestLap")
			if not best or best == 0 or lap < best then
				SaveService:SetPlayerData("BestLap", lap)
				showBest()
			end
			print(string.format("Lap: %.1f s %s", lap, medalFor(lap)))
		end
		started = now
	end
	nextGate = index % #gates + 1
	car:SetAttribute("NextGate", nextGate)
end

local function within(part, point)
	local offset = point - part.Position
	return math.abs(offset.X) <= part.Size.X / 2
		and math.abs(offset.Y) <= part.Size.Y / 2
		and math.abs(offset.Z) <= part.Size.Z / 2
end

RunService.Heartbeat:Connect(function(deltaTime)
	clock += deltaTime
	for index, gate in gates do
		local now = within(gate, chassis.Position)
		if now and not inside[index] then
			crossed(index)
		end
		inside[index] = now
	end
	if started then
		car:SetAttribute("LapTime", clock - started)
	end
end)

Players.PlayerAdded:Connect(showBest)
