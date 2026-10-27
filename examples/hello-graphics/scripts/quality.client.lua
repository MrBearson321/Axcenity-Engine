-- A LocalScript: graphics settings belong to the player's computer, so only
-- LocalScripts can change them. Press G to step through the presets: Low,
-- Medium, High, Ultra, then Automatic (what suits this computer). The player
-- saves the choice in its graphics.json, so it's still there next time; in
-- the editor's Play it lasts until Stop.

local UserInputService = game:GetService("UserInputService")
local UserGameSettings = game:GetService("UserGameSettings")

local order = {
    Enum.GraphicsQuality.Low,
    Enum.GraphicsQuality.Medium,
    Enum.GraphicsQuality.High,
    Enum.GraphicsQuality.Ultra,
    Enum.GraphicsQuality.Automatic,
}

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode ~= Enum.KeyCode.G then
        return
    end
    -- The one after the quality in use, going back to the first after the last.
    local nextOne = 1
    for index, quality in ipairs(order) do
        if quality == UserGameSettings.GraphicsQuality then
            nextOne = index % #order + 1
        end
    end
    UserGameSettings.GraphicsQuality = order[nextOne]
    print("Graphics quality: " .. order[nextOne].Name)
end)

print("Press G to change the graphics quality (now " .. UserGameSettings.GraphicsQuality.Name .. ").")
