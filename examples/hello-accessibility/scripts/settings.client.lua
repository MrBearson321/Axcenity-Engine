-- A settings menu for the game UI: text size, higher contrast and reduced
-- motion (GuiService). Save keeps them in slot 1 with the game's other
-- settings, and Load puts them back. Only the menu changes: the tower in
-- the world stays as it is.
local GuiService = game:GetService("GuiService")
local SaveService = game:GetService("SaveService")
local TweenService = game:GetService("TweenService")
local panel = workspace.Settings.Panel

local SIZES = { 1, 1.25, 1.5, 2 }

local function onOff(on)
    return if on then "on" else "off"
end

-- Shows the settings on the buttons, and fades the title in again: at once
-- with reduced motion on.
local function refresh()
    panel.TextSizeButton.Text = "Text size: " .. math.round(GuiService.TextScale * 100) .. "%"
    panel.ContrastButton.Text = "Higher contrast: " .. onOff(GuiService.HighContrast)
    panel.MotionButton.Text = "Reduced motion: " .. onOff(GuiService.ReducedMotion)
    panel.Title.TextTransparency = 1
    TweenService:Create(panel.Title, TweenInfo.new(0.6), { TextTransparency = 0 }):Play()
end

-- Says what a button changed, by its accessible name, as a screen reader would.
local function say(button, value)
    print(button.AccessibleName .. ": " .. value)
end

panel.TextSizeButton.Activated:Connect(function()
    local next = 1
    for index, size in SIZES do
        if size == GuiService.TextScale then
            next = index % #SIZES + 1
        end
    end
    GuiService.TextScale = SIZES[next]
    refresh()
    say(panel.TextSizeButton, math.round(GuiService.TextScale * 100) .. "%")
end)

panel.ContrastButton.Activated:Connect(function()
    GuiService.HighContrast = not GuiService.HighContrast
    refresh()
    say(panel.ContrastButton, onOff(GuiService.HighContrast))
end)

panel.MotionButton.Activated:Connect(function()
    GuiService.ReducedMotion = not GuiService.ReducedMotion
    refresh()
    say(panel.MotionButton, onOff(GuiService.ReducedMotion))
end)

panel.SaveButton.Activated:Connect(function()
    SaveService:SaveSlot(1)
    say(panel.SaveButton, "slot 1")
end)

panel.LoadButton.Activated:Connect(function()
    if SaveService:HasSlot(1) then
        SaveService:LoadSlot(1)
        refresh()
        say(panel.LoadButton, "slot 1")
    end
end)

refresh()
