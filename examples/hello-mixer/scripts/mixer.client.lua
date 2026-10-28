-- A LocalScript. Music and the footsteps play through the Music and SFX
-- groups under Master, in SoundService. Fly into the cave (the blue zone
-- under the roof) and its reverb fades in on the footsteps, and out again
-- when you leave. P opens the pause menu: a low-pass on the Music group
-- dulls the music, and the AudioSettings ScreenGui the engine made (for
-- the UserAdjustable groups) shows sliders for the master volume, Music
-- and SFX. 1 and 2 save and load slot 1, volumes included.

local RunService = game:GetService("RunService")
local UserInputService = game:GetService("UserInputService")
local SaveService = game:GetService("SaveService")
local SoundService = game:GetService("SoundService")

local music = SoundService.Master.Music
local steps = workspace.Steps
local pause = workspace.PauseMenu

-- Off until the menu opens.
local muffle = Instance.new("LowPassSoundEffect")
muffle.Name = "PauseMuffle"
muffle.Cutoff = 600
muffle.Enabled = false
muffle.Parent = music

-- The footsteps follow the camera, at its feet.
RunService.RenderStepped:Connect(function()
    steps.Position = workspace.CurrentCamera.CFrame.Position + Vector3.new(0, -2, 0)
end)

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.P then
        local open = not pause.Enabled
        pause.Enabled = open
        workspace.AudioSettings.Enabled = open
        muffle.Enabled = open
    elseif input.KeyCode == Enum.KeyCode.One then
        SaveService:SaveSlot(1)
        print("Saved. Music " .. music.Volume .. ", SFX " .. SoundService.Master.SFX.Volume)
    elseif input.KeyCode == Enum.KeyCode.Two and SaveService:HasSlot(1) then
        SaveService:LoadSlot(1)
        print("Loaded. Music " .. music.Volume .. ", SFX " .. SoundService.Master.SFX.Volume)
    end
end)

print("Fly into the cave to hear its reverb on your footsteps. P pauses: the music dulls and the audio settings open.")
