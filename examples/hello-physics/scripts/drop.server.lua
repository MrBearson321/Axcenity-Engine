-- The red Dropper starts anchored, hanging in the air. Anchored parts stay
-- where they are; unanchored parts fall and collide.

local dropper = workspace.Dropper

task.wait(2)
dropper.Anchored = false
print("released the dropper")

-- Anchoring it again freezes it in mid-air...
task.wait(0.2)
dropper.Anchored = true
print("froze the dropper in mid-air")

-- ...and releasing it lets it fall the rest of the way onto the tower.
task.wait(1.5)
dropper.Anchored = false
print("released it again")
