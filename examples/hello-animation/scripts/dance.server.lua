-- Drives the noodle's animator controller with its parameters.
-- The Animator starts in its default state, Locomotion: a blend tree that
-- mixes Idle (Speed 0), Walk (Speed 2) and Run (Speed 6). Setting the
-- Triggers Wave or Hop plays those once (from the Any State), and then it
-- goes back to Locomotion by itself.
local animator = workspace.Noodle.Animator

animator.StateChanged:Connect(function(state, previous)
	print("Noodle went from " .. previous .. " to " .. state)
end)

-- Speeds up from standing to running and back, a little each frame, so
-- the blend tree moves smoothly from Idle through Walk to Run.
local function rampSpeed(from, to, seconds)
	local passed = 0
	while passed < seconds do
		passed = passed + task.wait()
		animator:SetNumber("Speed", from + (to - from) * math.min(passed / seconds, 1))
	end
end

while true do
	task.wait(2)
	animator:SetTrigger("Wave")
	task.wait(2)
	rampSpeed(0, 6, 4)
	task.wait(1)
	rampSpeed(6, 0, 3)
	animator:SetTrigger("Hop")
end
