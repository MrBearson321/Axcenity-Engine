-- A LocalScript: the camera looks down a street of generated houses
-- (procedural worlds PW6). Each house is a BuildingGenerator: change its
-- Seed, Floors, RoofStyle or Size in the editor and it's built again; Bake
-- keeps one as ordinary parts. The corner shop at the end stands inside a
-- Spline, whose points are its corners.

local camera = workspace.CurrentCamera
camera.CameraType = Enum.CameraType.Scriptable
camera.CFrame = CFrame.lookAt(Vector3.new(-100, 28, 6), Vector3.new(10, 4, 0))
