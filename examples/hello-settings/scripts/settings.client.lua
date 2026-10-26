-- Each widget fires ValueChanged with its new Value, whether the player
-- changed it or a script did. Setting Loudness.Value from here moves its
-- bar without anyone touching it.
local panel = workspace.Settings.Panel
local status = panel.Status

panel.Volume.ValueChanged:Connect(function(volume)
	status.Text = "Volume " .. volume
	panel.Loudness.Value = volume
end)

panel.Fullscreen.ValueChanged:Connect(function(on)
	status.Text = if on then "Fullscreen on" else "Fullscreen off"
end)

panel.Quality.ValueChanged:Connect(function(quality)
	status.Text = "Quality " .. quality
end)
