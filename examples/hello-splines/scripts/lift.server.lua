-- The lift goes up its spline and back down, carrying whoever stands on
-- it. The guard patrols its route and the camera rides its rail with no
-- script: this one only says when the lift turns round.
local follower = workspace.Lift.SplineFollower

follower.Reached:Connect(function(which)
	print("The lift reached the " .. string.lower(which))
end)

local path = workspace.LiftPath
print(string.format("The lift's path is %.1f studs long", path.Length))
