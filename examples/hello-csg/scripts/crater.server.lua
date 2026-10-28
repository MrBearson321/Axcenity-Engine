-- The Doorway is a UnionOperation: the editor's Union joined the Wall and
-- the negated Door (pink while it was negative), cutting the doorway out.
-- Its CollisionFidelity is PreciseConvexDecomposition, so characters walk
-- through the doorway. Separate (Model > Solid Modeling) gives the parts back.

-- At runtime, SubtractAsync scoops the ball out of the ground in the
-- background; the script waits for the result.
local ground = workspace.Ground
local scoop = workspace.Scoop
local crater = ground:SubtractAsync({ scoop }, {
	CollisionFidelity = Enum.CollisionFidelity.PreciseConvexDecomposition,
})
crater.Name = "Crater"
crater.Parent = workspace
ground:Destroy()
scoop:Destroy()
print("made the crater")

-- The marble drops in and rolls to the bottom.
workspace.Marble.Anchored = false
