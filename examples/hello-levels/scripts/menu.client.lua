-- The main menu: New game starts here, and Continue loads slot 1, saved on
-- arriving in a level, which takes you back to that level.
local SaveService = game:GetService("SaveService")
local menu = workspace.Menu
local panel = menu.Panel

panel.Continue.Visible = SaveService:HasSlot(1)

panel.Play.Activated:Connect(function()
	menu.Enabled = false
end)

panel.Continue.Activated:Connect(function()
	SaveService:LoadSlot(1)
	menu.Enabled = false
end)
