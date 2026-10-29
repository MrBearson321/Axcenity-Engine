-- A LocalScript: the camera looks down two rows of pillars toward a striped
-- wall, through the five lens effects in Lighting. Press L to step through
-- them one at a time, all together, and none.

local UserInputService = game:GetService("UserInputService")
local Lighting = game:GetService("Lighting")

local camera = workspace.CurrentCamera
camera.CameraType = Enum.CameraType.Scriptable
camera.CFrame = CFrame.lookAt(Vector3.new(0, 4, 4), Vector3.new(0, 3, -40))

local effects = {
    DepthOfField = Lighting:WaitForChild("DepthOfField"),
    Vignette = Lighting:WaitForChild("Vignette"),
    FilmGrain = Lighting:WaitForChild("FilmGrain"),
    ChromaticAberration = Lighting:WaitForChild("ChromaticAberration"),
    Sharpen = Lighting:WaitForChild("Sharpen"),
}

local steps = {
    { name = "All five", on = { "DepthOfField", "Vignette", "FilmGrain", "ChromaticAberration", "Sharpen" } },
    { name = "Depth of field (sharp from 8 to 20 studs)", on = { "DepthOfField" } },
    { name = "Vignette", on = { "Vignette" } },
    { name = "Film grain", on = { "FilmGrain" } },
    { name = "Chromatic aberration", on = { "ChromaticAberration" } },
    { name = "Sharpen", on = { "Sharpen" } },
    { name = "None", on = {} },
}

local current = 1

local function show(step)
    for _, effect in effects do
        effect.Enabled = false
    end
    for _, name in step.on do
        effects[name].Enabled = true
    end
    print("Lens: " .. step.name)
end

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode ~= Enum.KeyCode.L then
        return
    end
    current = current % #steps + 1
    show(steps[current])
end)

print("Press L to step through the lens effects (now " .. steps[current].name .. ").")
