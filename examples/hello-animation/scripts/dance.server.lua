-- Cycles the noodle through its animator controller's states.
-- The Animator starts in its default state, Idle.
local animator = workspace.Noodle.Animator

animator.StateChanged:Connect(function(state, previous)
	print("Noodle went from " .. previous .. " to " .. state)
end)

while true do
	task.wait(3)
	animator:Play("Wave")
	task.wait(2)
	animator:Play("Hop", 0.1)
	task.wait(1.5)
	animator:Play("Walk")
	task.wait(3)
	animator:Play("Idle")
end
