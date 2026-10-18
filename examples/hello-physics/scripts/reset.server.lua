-- Setting Position on an unanchored part teleports it, and it starts again
-- from rest. Every 5 seconds this puts the bouncy ball back where it began.

local ball = workspace.BouncyBall
local start = ball.Position

while true do
    task.wait(5)
    ball.Position = start
end
