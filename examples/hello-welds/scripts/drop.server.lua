-- The table's top and four legs are welded together, so they fall and land
-- as one piece. The red Crate is welded to the anchored blue Beam, and an
-- anchored part anchors everything welded to it, so the Crate hangs there.

local hold = workspace.Crate.Hold

task.wait(2)
hold.Enabled = false
print("unwelded the crate")

-- Welding it again holds it wherever it is now: in mid-air...
task.wait(0.15)
hold.Enabled = true
print("welded the crate in mid-air")

-- ...until the weld lets go for good.
task.wait(1.5)
hold.Enabled = false
print("dropped the crate")

-- The table is one assembly: its root is the top, the largest part.
local top = workspace.TableTop
print(#top:GetConnectedParts(true), "parts in the table, rooted at", top:GetRootPart().Name)
