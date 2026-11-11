-- Places three copies of the Coin template. Its script starts in each copy
-- once it's in the Workspace.
local template = game.ServerStorage.Coin

for i = 1, 3 do
	local coin = template:Clone()
	coin.Name = "Coin" .. i
	coin.Position = Vector3.new(-6 + i * 3, 2, -6)
	coin.Parent = workspace
end
