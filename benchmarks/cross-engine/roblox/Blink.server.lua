-- XB10k: wakes every 0.1 to 0.5 s, reads a grid block and recolors it.
local k = script:GetAttribute("Index")
local part = workspace.Grid:WaitForChild("G" .. (k * 91))
local wait = 0.1 + (k % 5) * 0.1
local base = part.Color
local count = 0
while true do
    task.wait(wait)
    count += 1
    local height = part.Position.Y
    if count % 2 == 0 and height > 0 then
        part.Color = base
    else
        part.Color = Color3.new(1, 0.5, 0)
    end
end
