-- Game tests for the abilities: run them from the editor's Tests panel,
-- or with `cargo run -p axcenity-testing -- examples/hello-abilities`.
describe("Fireball", function()
	local character, fireball, dummy

	beforeEach(function()
		character = Test.find("Workspace.Character")
		fireball = Test.find("Workspace.Character.Fireball")
		dummy = Test.find("Workspace.Dummy.Health")
	end)

	it("costs 20 Mana, hits the dummy and starts a 3 second cooldown", function()
		Test.press("Fireball")
		Test.step()
		expect(character).toHaveAttribute("Mana", 80)
		expect(dummy.Health).toBe(85)
		expect(fireball.CooldownLeft).toBeGreaterThan(2.9)
	end)

	it("does nothing while it cools down", function()
		Test.press("Fireball")
		Test.step(1)
		Test.press("Fireball")
		Test.step()
		-- Mana comes back 5 a second; a second cast would have taken 20.
		expect(character:GetAttribute("Mana")).toBeGreaterThan(79)
		-- One hit of 15 and a tick of poison, not a second hit.
		expect(dummy.Health).toBeGreaterThan(75)
	end)

	it("can't be cast while the character is stunned", function()
		character:AddTag("Stunned")
		Test.press("Fireball")
		Test.step()
		expect(character).toHaveAttribute("Mana", 100)
		expect(dummy.Health).toBe(100)
		local ok, why = fireball:CanActivate()
		expect(ok).toBe(false)
		expect(why).toBe("its holder has the Stunned tag")
	end)

	it("poisons the dummy for 2 a second, stacking to 3", function()
		for _ = 1, 4 do
			Test.press("Fireball")
			Test.step(3.1)
		end
		local poison = Test.find("Workspace.Dummy.Poison")
		expect(poison.Stacks).toBe(3)
		expect(Test.find("Workspace.Dummy")).toBeTruthy()
		poison:Destroy()
		Test.step()
		local after = dummy.Health
		Test.step(2)
		expect(dummy.Health).toBe(after)
	end)
end)
