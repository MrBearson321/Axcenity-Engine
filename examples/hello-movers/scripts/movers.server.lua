-- Six movers, each pushing or turning the part its Attachment0 is on.
--   Balloon: a VectorForce of its weight, set below, makes it hover.
--   SlowFollower, QuickFollower: AlignPositions chasing the circling Target
--     at Responsiveness 10 and 40; the quick one keeps up closer.
--   Drone: a LinearVelocity flies it back and forth, an AlignOrientation
--     keeps it level although a Gust pushes up on one side.
--   Top: an AngularVelocity spins it on the floor.
--   Spinner: an AlignPosition holds it in the air while a Torque turns it.

local RunService = game:GetService("RunService")

-- Its mass times gravity, upward, through its middle.
local balloon = workspace.Balloon
balloon.Lift.Force = Vector3.new(0, balloon:GetMass() * workspace.Gravity, 0)

-- The target circles; the followers' attachments go round with it.
local target = workspace.Target
local center = target.Position
local t = 0
RunService.Heartbeat:Connect(function(dt)
	t += dt
	target.Position = center + Vector3.new(math.sin(t) * 5, math.cos(t) * 2 + 2, 0)
end)

-- The drone turns back every 3 seconds.
local flight = workspace.Drone.Flight
task.spawn(function()
	while true do
		task.wait(3)
		flight.VectorVelocity = -flight.VectorVelocity
	end
end)

-- How far a follower is behind the attachment it follows.
local function behind(follower)
	local goal = follower.Follow.Attachment1.WorldPosition
	return (follower.Position - goal).Magnitude
end

-- How far a part leans from level, in degrees.
local function tilt(part)
	return math.deg(math.acos(math.clamp(part.CFrame.UpVector.Y, -1, 1)))
end

task.wait(4)
print(string.format(
	"balloon %.1f, quick closer %s, drone tilt %d, top %.1f",
	balloon.Position.Y,
	tostring(behind(workspace.QuickFollower) < behind(workspace.SlowFollower)),
	math.round(tilt(workspace.Drone)),
	workspace.Top.AssemblyAngularVelocity.Y
))
