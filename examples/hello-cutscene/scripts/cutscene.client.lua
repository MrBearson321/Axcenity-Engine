-- The Intro sequence plays by itself (AutoPlay). Escape skips it.
local intro = workspace:WaitForChild("Intro")

intro.EventReached:Connect(function(name)
	print("cutscene event", name)
end)

intro.Completed:Connect(function()
	print("cutscene finished: walk around")
end)
