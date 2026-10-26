-- The project's loading screen is the ScreenGui "Loading" (File > Build
-- Game > Loading screen). It's shown over the HUD from the first frame and,
-- because the project waits for a script, stays until this says it's ready.
local TweenService = game:GetService("TweenService")
local ReplicatedFirst = game:GetService("ReplicatedFirst")
local loading = workspace.Loading.Backdrop

local fill = TweenService:Create(
    loading.Track.Fill,
    TweenInfo.new(1.5, Enum.EasingStyle.Quad, Enum.EasingDirection.Out),
    { Size = UDim2.new(1, 0, 1, 0) }
)
fill.Completed:Connect(function()
    loading.Status.Text = "Ready!"
    task.wait(0.3)
    ReplicatedFirst:RemoveDefaultLoadingScreen()
end)
fill:Play()
