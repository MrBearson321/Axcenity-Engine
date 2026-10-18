-- An imported model is a MeshPart. It collides like any part (its collision
-- shape is the model's convex hull, so things slide down the pyramid's sides)
-- and fires Touched like one.

local pyramid = workspace.Pyramid

pyramid.Touched:Connect(function(otherPart)
    print(otherPart.Name .. " touched the pyramid")
end)

print("The pyramid's model is " .. pyramid.MeshId)
