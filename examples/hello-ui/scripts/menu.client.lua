local menu = workspace.MainMenu.Menu
local status = menu.Status

menu.StartButton.Activated:Connect(function()
    status.Text = "Button activated!"
end)
