-- A LocalScript: press G to step through looks made by the ColorCorrectionEffect
-- in Lighting. Each changes the finished picture after tone mapping.

local UserInputService = game:GetService("UserInputService")
local effect = game:GetService("Lighting"):WaitForChild("ColorCorrection")

local looks = {
    { name = "Warm (a .cube lookup table)", LookupTable = "assets/grading/Warm.cube", Contrast = 0.1 },
    { name = "Cold night", Brightness = -0.05, Saturation = -0.3, TintColor = Color3.new(0.75, 0.85, 1) },
    { name = "Black and white", Saturation = -1, Contrast = 0.2 },
    { name = "Posterized", Posterize = 4 },
    { name = "Retro palette", Palette = "assets/grading/Retro.bmp" },
    { name = "None", Enabled = false },
}

local current = 1

local function show(look)
    effect.Enabled = look.Enabled ~= false
    effect.Brightness = look.Brightness or 0
    effect.Contrast = look.Contrast or 0
    effect.Saturation = look.Saturation or 0
    effect.TintColor = look.TintColor or Color3.new(1, 1, 1)
    effect.LookupTable = look.LookupTable or ""
    effect.Posterize = look.Posterize or 0
    effect.Palette = look.Palette or ""
    print("Look: " .. look.name)
end

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode ~= Enum.KeyCode.G then
        return
    end
    current = current % #looks + 1
    show(looks[current])
end)

print("Press G to change the look (now " .. looks[current].name .. ").")
