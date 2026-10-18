-- A LocalScript: it runs on the player's side, so it can read the keyboard.
-- It doesn't move the block itself. It asks the server, through the
-- MoveRequest RemoteEvent, and the server's Script does the moving. That
-- keeps the game fair once several players share one server.

local UserInputService = game:GetService("UserInputService")
local MoveRequest = game.ReplicatedStorage.MoveRequest

-- Which way each arrow key moves the block (the camera starts looking
-- toward -Z, so Up moves away from you).
local directions = {
    Up = Vector3.new(0, 0, -1),
    Down = Vector3.new(0, 0, 1),
    Left = Vector3.new(-1, 0, 0),
    Right = Vector3.new(1, 0, 0),
}

UserInputService.InputBegan:Connect(function(input)
    if input.KeyCode == Enum.KeyCode.Space then
        print("Hold the arrow keys to move the blue block.")
    end
end)

print("Hold the arrow keys to move the blue block (Space for help).")

-- Once a frame, send the direction of the arrow keys held down, if any.
while true do
    local seconds = task.wait()
    local direction = Vector3.new(0, 0, 0)
    for key, step in pairs(directions) do
        if UserInputService:IsKeyDown(Enum.KeyCode[key]) then
            direction = direction + step
        end
    end
    if direction.Magnitude > 0 then
        MoveRequest:FireServer(direction, seconds)
    end
end
