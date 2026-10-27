-- Buttons fire Activated whether they're clicked or confirmed with Enter
-- or a controller's A button. The name box fires FocusLost when typing
-- finishes, however it was focused. Quit's NextSelectionDown is
-- "playername", so pressing down on Quit wraps focus to the top.
local panel = workspace.Menu.Panel
local status = panel.Status

for _, name in { "Play", "Options", "Quit" } do
	panel[name].Activated:Connect(function()
		status.Text = name .. " pressed"
		print(name .. " pressed")
	end)
end

panel.PlayerName.FocusLost:Connect(function()
	status.Text = "Hello, " .. panel.PlayerName.Text
	print("Hello, " .. panel.PlayerName.Text)
end)

panel.Volume.ValueChanged:Connect(function(volume)
	status.Text = "Volume " .. volume
end)
