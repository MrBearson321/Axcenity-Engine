-- The glass pane has a Fracture: Generate (in its Properties) cut it into
-- 30 pieces, saved in assets/models/GlassPane.fracture.glb. Anything that
-- hits it faster than the Fracture's Threshold shatters it.
local fracture = workspace.GlassPane.Fracture
fracture.Shattered:Connect(function()
	print("the pane shattered into " .. #workspace.GlassPanePieces:GetChildren() .. " pieces")
end)

-- The shelf's weld breaks past its BreakForce: the light crate on it is
-- fine, the heavy metal one isn't.
local bracket = workspace.Shelf.Bracket
bracket.Broken:Connect(function()
	print("the shelf broke")
end)

task.wait(1)
local crate = workspace.Crate
crate.Anchored = false
crate.AssemblyLinearVelocity = Vector3.new(0, 4, -80)
print("threw the crate")

task.wait(2)
workspace.HeavyCrate.Anchored = false
print("dropped the heavy crate")
