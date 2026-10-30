-- Game tests for the car and the race. Run them from the editor's Tests
-- panel (View > Panels > Tests), or with
-- `cargo run -p axcenity-testing -- <this project's folder>`.
describe("Race", function()
	local car, seat, player

	beforeEach(function()
		Test.step(0.2)
		car = Test.find("Workspace.Car")
		seat = car.DriverSeat
		player = game.Players:GetPlayers()[1]
	end)

	local function getIn()
		player.Character:PivotTo(CFrame.new(seat.Position + Vector3.new(-4, 3, 0)))
		Test.step(0.3)
		Test.press("Interact")
		Test.step(0.2)
	end

	-- Puts the car's chassis at (x, z), as if it had driven there. It's
	-- anchored first so it stays put; "drives forward on W" drives it.
	local function driveTo(x, z)
		car.Chassis.Anchored = true
		car.Chassis.Position = Vector3.new(x, 2, z)
		Test.step(0.25)
	end

	it("seats you with the Drive prompt", function()
		expect(seat.Occupant).toBe(nil)
		getIn()
		expect(seat.Occupant).toBeTruthy()
		expect(seat.DrivePrompt.Enabled).toBe(false)
	end)

	it("drives forward on W", function()
		getIn()
		local start = car.Chassis.Position
		Test.press("W", 1.5)
		expect((car.Chassis.Position - start).Magnitude).toBeGreaterThan(3)
	end)

	it("lets you out on Jump and shows the prompt again", function()
		getIn()
		Test.press("Jump")
		Test.step(0.2)
		expect(seat.Occupant).toBe(nil)
		expect(seat.DrivePrompt.Enabled).toBe(true)
	end)

	it("times a lap through every gate and gives the medal from the table", function()
		driveTo(0, 20) -- the start line
		driveTo(50, -60)
		driveTo(100, 0)
		driveTo(50, 60)
		driveTo(0, 20)
		local lap = car:GetAttribute("LastLap")
		expect(lap).toBeGreaterThan(0.5)
		local gold = game:GetService("DataService"):GetTable("medals"):Get("gold")
		expect(lap).toBeLessThan(gold.Seconds)
		expect(car:GetAttribute("Medal")).toBe("Gold")
		expect(car:GetAttribute("BestLap")).toBe(lap)
		expect(Test.find("Workspace.Hud.Best").Text).toBe(string.format("Best 0:%04.1f  Gold", lap))
	end)

	it("doesn't count a lap that missed a gate", function()
		driveTo(0, 20)
		driveTo(50, -60)
		driveTo(50, 60)
		driveTo(0, 20)
		expect(car:GetAttribute("LastLap")).toBe(nil)
		expect(car:GetAttribute("NextGate")).toBe(3)
	end)

	it("keeps the best lap in the save slot", function()
		driveTo(0, 20)
		driveTo(50, -60)
		driveTo(100, 0)
		driveTo(50, 60)
		driveTo(0, 20)
		local best = car:GetAttribute("BestLap")
		local menu = Test.find("Workspace.GameMenu")
		Test.press("Menu")
		Test.click(menu.Pause.Save)
		Test.wait(function()
			return menu.Pause.Status.Text == "Saved to slot 1"
		end)
		game:GetService("SaveService"):SetPlayerData("BestLap", 0)
		car:SetAttribute("BestLap", 0)
		Test.click(menu.Pause.Load)
		Test.wait(function()
			return menu.Pause.Status.Text == "Loaded slot 1"
		end)
		expect(car:GetAttribute("BestLap")).toBeNear(best)
	end)
end)
