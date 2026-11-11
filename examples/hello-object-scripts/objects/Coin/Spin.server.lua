-- The Coin in ServerStorage is a template: this script doesn't run there.
-- Each copy `scripts/coins.server.lua` puts in the Workspace runs it.
local coin = script.Parent
print(coin.Name .. " spins")

while coin.Parent do
	coin.Orientation += Vector3.new(0, 180 * task.wait(), 0)
end
