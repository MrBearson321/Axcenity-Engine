-- Asks the server for the score every two seconds. InvokeServer waits for
-- the answer.
local ReplicatedStorage = game:GetService("ReplicatedStorage")

while true do
    task.wait(2)
    local score, saved = ReplicatedStorage.GetScore:InvokeServer()
    print("Score: " .. score .. " " .. saved)
end
