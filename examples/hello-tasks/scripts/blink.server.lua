-- Switches the lamp between red and blue once a second, forever.
-- `task.wait(1)` pauses only this script: the game keeps drawing, and the
-- other scripts keep running, while it waits.

local lamp = workspace.Lamp
local red = Color3.fromRGB(255, 64, 38)
local blue = Color3.fromRGB(38, 115, 255)

lamp.Color = red
local blinks = 0
while true do
    task.wait(1)
    if lamp.Color == red then
        lamp.Color = blue
    else
        lamp.Color = red
    end
    blinks = blinks + 1
    print("blink " .. blinks)
end
