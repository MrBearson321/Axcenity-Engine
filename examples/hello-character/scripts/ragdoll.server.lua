-- The Dummy stands idle, then every few seconds is knocked over backward
-- by a push on its head, lies there, and gets up again: its Ragdoll's
-- capsules take over its skeleton while Enabled is true, and BlendTime
-- blends it back to its Animator's idle once it's false.

local dummy = workspace.Dummy
local ragdoll = dummy.Ragdoll

-- How hard the head is pushed: the head weighs about 0.06 (its capsule's
-- volume at Plastic's density), so this sends it back at about 35 studs a
-- second.
local PUSH = 2

while true do
    task.wait(2)
    ragdoll.Enabled = true
    -- The Dummy faces its LookVector, so the push is the other way.
    ragdoll:ApplyImpulse("Head", -dummy.CFrame.LookVector * PUSH)
    task.wait(3)
    ragdoll.Enabled = false
end
