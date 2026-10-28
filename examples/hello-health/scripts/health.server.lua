-- Phase 43: the character's Health. Walk into the lava to lose health
-- (leave it and it comes back after RegenDelay), and drop off the tower to
-- take fall damage. At 0 the character goes back to the start.
local character = workspace:WaitForChild("Character")
local health = character:WaitForChild("Health")
local controller = character:WaitForChild("ThirdPersonController")
local start = character:GetPivot()

controller.Landed:Connect(function(speed)
	if speed > health.FallDamageThreshold then
		print(string.format("Landed at %d studs a second", math.floor(speed)))
	end
end)

health.Died:Connect(function()
	print("The character died")
	task.wait(2)
	character:PivotTo(start)
	health.Health = health.MaxHealth
	print("Back at the start")
end)
