-- Game tests for the blaster and the targets. Run them from the editor's
-- Tests panel (View > Panels > Tests), or with
-- `cargo run -p axcenity-testing -- <this project's folder>`.
describe("Blaster", function()
	local blaster, target

	beforeEach(function()
		blaster = Test.find("Workspace.Character.Inventory.Blaster")
		target = Test.find("Workspace.Targets.Target2")
		Test.step(0.2)
	end)

	local function fire()
		Test.press("Primary")
		Test.step(0.1)
	end

	it("starts in hand with a full clip", function()
		expect(Test.find("Workspace.Character.Inventory").Equipped).toBe(blaster)
		expect(blaster:GetAttribute("Ammo")).toBe(12)
		expect(Test.find("Workspace.Hud.Ammo").Text).toBe("12 / 12")
	end)

	it("spends a shot and hits the target in front", function()
		fire()
		expect(blaster:GetAttribute("Ammo")).toBe(11)
		expect(target.Health.Health).toBe(25)
		expect(Test.find("Workspace.Hud.Ammo").Text).toBe("11 / 12")
	end)

	it("counts a target knocked down on the HUD", function()
		fire()
		fire()
		expect(target.Health.Health).toBe(0)
		Test.wait(function()
			return Test.find("Workspace.Hud.Score").Text == "Targets down: 1"
		end)
	end)

	it("stands a target up again", function()
		fire()
		fire()
		Test.step(3.5)
		expect(target.Health.Health).toBe(50)
	end)

	it("reloads on R", function()
		fire()
		fire()
		fire()
		expect(blaster:GetAttribute("Ammo")).toBe(9)
		Test.press("Reload")
		Test.step(0.1)
		expect(blaster:GetAttribute("Ammo")).toBe(12)
	end)

	it("doesn't fire with an empty clip", function()
		blaster:SetAttribute("Ammo", 0)
		fire()
		expect(blaster:GetAttribute("Ammo")).toBe(0)
		expect(target.Health.Health).toBe(50)
	end)
end)
