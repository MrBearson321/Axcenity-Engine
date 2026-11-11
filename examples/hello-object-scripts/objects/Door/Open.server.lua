-- One file for every door: each copy of the Door prefab runs this script,
-- and `script.Parent` is that copy's own door.
local door = script.Parent
local closed = door.Position
local opened = closed + Vector3.new(0, door.Size.Y, 0)

print(door.Name .. " is ready")

-- Every three seconds the door slides up, then back down.
while door.Parent do
	task.wait(3)
	door.Position = opened
	print(door.Name .. " opens")
	task.wait(1)
	door.Position = closed
end
