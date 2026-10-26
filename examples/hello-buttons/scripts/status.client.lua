-- MouseEnter and MouseLeave follow the pointer onto and off Play, as its
-- :hover style does. Locked has Interactable off, so it never fires.
local menu = workspace.Menu
local status = menu.Status

menu.Play.MouseEnter:Connect(function()
	status.Text = "Ready?"
end)

menu.Play.MouseLeave:Connect(function()
	status.Text = "Point at Play"
end)

menu.Play.Activated:Connect(function()
	status.Text = "Playing!"
end)

menu.Locked.Activated:Connect(function()
	status.Text = "Locked was clicked"
end)
