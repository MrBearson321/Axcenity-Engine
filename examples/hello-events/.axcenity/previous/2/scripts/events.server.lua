local RunService = game:GetService("RunService")

-- Events that react to changes. Press Play, then try changing the Lamp's
-- Color or Name in the Properties panel: this script notices.

-- Heartbeat fires once every frame, after physics, with the frame's length
-- in seconds. Turning 90 degrees a second looks the same at any frame rate.
local spinner = workspace.Spinner

RunService.Heartbeat:Connect(function(deltaTime)
    spinner.Orientation = spinner.Orientation + Vector3.new(0, 90 * deltaTime, 0)
end)

-- Changed fires with the name of the property that changed, whoever changed
-- it: a script, physics, or you in the editor during Play.
local lamp = workspace.Lamp
lamp.Changed:Connect(function(property)
    print("Lamp's " .. property .. " changed")
end)

-- GetPropertyChangedSignal watches one property. Physics moves the ball,
-- so this fires as it falls.
local ball = workspace.Ball
local landed = false
ball:GetPropertyChangedSignal("Position"):Connect(function()
    if not landed and ball.Position.Y < 1.5 then
        landed = true
        print("the ball reached the floor")
    end
end)

-- ChildAdded fires when something is put inside the Workspace.
workspace.ChildAdded:Connect(function(child)
    print(child.Name .. " was added to the Workspace")
end)

task.wait(2)
local crate = Instance.new("Part")
crate.Name = "Crate"
crate.Position = Vector3.new(-6, 8, 0)
crate.Size = Vector3.new(2, 2, 2)
crate.Color = Color3.fromRGB(190, 130, 70)
crate.Parent = workspace

task.wait(1)
lamp.Color = Color3.fromRGB(255, 220, 90)
