-- The road, the trees beside it and the grove are made from their splines
-- by the SplineMesh and SplineScatters inside them. Nobody sits in the
-- car: this drives it down the road, steering toward a point a little
-- further along the road's spline, and stops it at the end.
local road = workspace.Road
local seat = workspace.AutoCar.DriverSeat
local chassis = workspace.AutoCar.Chassis
local RunService = game:GetService("RunService")

local LOOK_AHEAD = 12
local stopped = false

RunService.Heartbeat:Connect(function()
	if stopped then
		return
	end
	local here = chassis.Position
	local along = road:GetClosestDistance(here)
	if along >= road.Length - 10 then
		stopped = true
		seat.Throttle = 0
		seat.Steer = 0
		seat.HandBrake = true
		print("The car reached the end of the road")
		return
	end
	local ahead = road:GetCFrameAt(along + LOOK_AHEAD).Position
	local side = (ahead - here).Unit:Dot(chassis.CFrame.RightVector)
	seat.Steer = math.clamp(side * 3, -1, 1)
	seat.Throttle = 0.6
end)

print(string.format("The road is %.0f studs long", road.Length))
