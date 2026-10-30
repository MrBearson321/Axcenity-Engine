-- The pause menu and the settings menu. Menu (Escape, or Start on a
-- controller) pauses: the character stops and the menu shows. Settings
-- lists the actions in MenuSettings.module.lua; click one and press a key,
-- mouse button or controller button to rebind it. Rebinds are SaveService
-- settings, so they're kept in the save slot too.
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local UserInputService = game:GetService("UserInputService")

local LocalGui = require(ReplicatedStorage:WaitForChild("LocalGui"))
local settings = require(ReplicatedStorage:WaitForChild("MenuSettings"))
local saveGame = ReplicatedStorage:WaitForChild("SaveGame")

local gui = LocalGui.claim("GameMenu")
local pause = gui:WaitForChild("Pause")
local settingsPanel = gui:WaitForChild("Settings")
local rows = settingsPanel:WaitForChild("Rows")

-- The character's controller, whichever kind it has.
local function controller()
	local character = Players.LocalPlayer.Character
	if not character then
		return nil
	end
	return character:FindFirstChildWhichIsA("ThirdPersonController", true)
		or character:FindFirstChildWhichIsA("FirstPersonController", true)
end

local function setPaused(paused: boolean)
	pause.Visible = paused
	settingsPanel.Visible = false
	local moving = controller()
	if moving then
		-- PlatformStand holds the character still while the menu is open.
		moving.PlatformStand = paused
	end
	pause.Status.Text = "Escape closes this menu"
end

local function paused()
	return pause.Visible or settingsPanel.Visible
end

pause.Resume.Activated:Connect(function()
	setPaused(false)
end)

pause.Settings.Activated:Connect(function()
	pause.Visible = false
	settingsPanel.Visible = true
end)

settingsPanel.Back.Activated:Connect(function()
	settingsPanel.Visible = false
	pause.Visible = true
end)

for _, action in { "Save", "Load" } do
	pause[action].Activated:Connect(function()
		pause.Status.Text = saveGame:InvokeServer(action)
	end)
end

-- One row per action, copied from the hidden Row.
local waiting = nil
local template = rows:WaitForChild("Row")
local function show(row)
	row.Binding.Text = table.concat(UserInputService:GetActionInputs(row.Name), ", ")
end
for index, action in settings.Actions do
	local row = template:Clone()
	row.Name = action
	row.Label.Text = action
	row.Position = UDim2.fromOffset(0, (index - 1) * 48)
	row.Visible = true
	row.Parent = rows
	show(row)
	row.Binding.Activated:Connect(function()
		waiting = row
		row.Binding.Text = "Press a key..."
	end)
end

local MOUSE_BUTTONS = {
	[Enum.UserInputType.MouseButton1] = true,
	[Enum.UserInputType.MouseButton2] = true,
	[Enum.UserInputType.MouseButton3] = true,
}

UserInputService.InputBegan:Connect(function(input, gameProcessedEvent)
	if waiting then
		local new = nil
		if input.KeyCode ~= Enum.KeyCode.Unknown then
			new = input.KeyCode
		elseif MOUSE_BUTTONS[input.UserInputType] and not gameProcessedEvent then
			new = input.UserInputType
		end
		if new then
			UserInputService:RebindAction(waiting.Name, { new })
			show(waiting)
			waiting = nil
		end
	elseif not gameProcessedEvent and UserInputService:IsActionPressed("Menu") then
		setPaused(not paused())
	end
end)
