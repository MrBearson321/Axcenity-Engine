-- Touch the Pad to open the door (property Animation clip).
local pad = workspace.Pad
local door = workspace.Door
local animation = door.Open

pad.Touched:Connect(function(other)
	if other == door then
		return
	end
	animation:Play()
end)

animation.Completed:Connect(function()
	print("Door opened")
end)

animation.AnimationEvent:Connect(function(name)
	print("Door event:", name)
end)
