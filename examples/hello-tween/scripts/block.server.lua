-- Raises the Block 4 studs and turns it blue over one second, slowing
-- down at the end (Quad, Out).
local TweenService = game:GetService("TweenService")
local block = workspace.Block

local info = TweenInfo.new(1, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
local rise = TweenService:Create(block, info, {
    Position = block.Position + Vector3.new(0, 4, 0),
    Color = Color3.fromRGB(80, 160, 255),
})
rise.Completed:Connect(function(state)
    print("The block finished rising:", state)
end)

task.wait(1)
rise:Play()
