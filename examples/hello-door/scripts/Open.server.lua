-- Touch the Pad to open the door: the Door's Animator plays its Open clip.
local pad = workspace.Pad
local door = workspace.Door
local track = door.Animator:LoadAnimation(door.Open)

pad.Touched:Connect(function(other)
	if other == door or track.IsPlaying then
		return
	end
	track:Play()
end)

track.Stopped:Connect(function()
	print("Door opened")
end)

track:GetMarkerReachedSignal("Opened"):Connect(function()
	print("Door event: Opened")
end)
