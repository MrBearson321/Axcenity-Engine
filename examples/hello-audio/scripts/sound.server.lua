local pad = workspace.Pad
local chime = workspace.Chime
local music = workspace.Music

music:Play()
pad.Touched:Connect(function()
    chime:Play()
end)
