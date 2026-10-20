-- Drives the noodle's animator controller with its parameters.
-- The Animator starts in its default state, Idle. Its transitions:
--   Idle <-> Walk while the Bool IsWalking is true or false,
--   Any State -> Wave and Any State -> Hop when those Triggers are set,
--   and Wave and Hop go back to Idle by themselves when they finish.
local animator = workspace.Noodle.Animator

animator.StateChanged:Connect(function(state, previous)
	print("Noodle went from " .. previous .. " to " .. state)
end)

while true do
	task.wait(3)
	animator:SetTrigger("Wave")
	task.wait(2)
	animator:SetTrigger("Hop")
	task.wait(2)
	animator:SetBool("IsWalking", true)
	task.wait(3)
	animator:SetBool("IsWalking", false)
end
