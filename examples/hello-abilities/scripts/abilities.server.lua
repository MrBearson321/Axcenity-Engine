-- Phase 76: abilities and effects. Q casts a Fireball at the dummy (20
-- Mana, 3 seconds of cooldown) that hits it and poisons it, stacking up to
-- 3 times. R heals over 5 seconds (30 Mana). Standing on the yellow pad
-- stuns the character for 2 seconds: neither ability works then. The
-- abilities' cost, cooldown and tags are all properties; this script only
-- says what each one does.
local effects = game.ReplicatedStorage.Effects
local character = workspace:WaitForChild("Character")
local dummy = workspace:WaitForChild("Dummy")

local fireball = character:WaitForChild("Fireball")
fireball.Activated:Connect(function()
	dummy.Health:TakeDamage(fireball:GetAttribute("Damage"), fireball)
	dummy:ApplyEffect(effects.Poison)
	print(string.format("Fireball! The dummy has %d health", dummy.Health.Health))
end)

character:WaitForChild("Heal").Activated:Connect(function()
	character:ApplyEffect(effects.Regeneration)
end)

-- Mana comes back 5 a second.
task.spawn(function()
	while true do
		task.wait(1)
		local mana = character:GetAttribute("Mana")
		character:SetAttribute("Mana", math.min(mana + 5, character:GetAttribute("MaxMana")))
	end
end)

workspace.StunPad.Touched:Connect(function(part)
	if part:IsDescendantOf(character) and not character:HasTag("Stunned") then
		character:ApplyEffect(effects.Stun)
		print("Stunned for 2 seconds")
	end
end)

-- The dummy is back to full a moment after it falls.
dummy.Health.Died:Connect(function()
	task.wait(2)
	dummy.Health.Health = dummy.Health.MaxHealth
end)
