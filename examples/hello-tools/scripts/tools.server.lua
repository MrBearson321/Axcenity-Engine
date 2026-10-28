-- Phase 45: Tools. Walk over the sword, the torch and the shield to pick
-- them up; the Inventory holds 2, so the third stays on the ground. 1 and 2
-- (or the controller's bumpers) put one in the right hand, the mouse wheel
-- goes through them while one is in hand, clicking uses it, and Backspace
-- drops it in front of you. keys.client.lua saves to slot 1 (K) and loads
-- it (L), Tools in hand and all.
local DataService = game:GetService("DataService")
local weapons = DataService:GetTable("weapons")
local inventory = workspace:WaitForChild("Character"):WaitForChild("Inventory")

-- Each Tool's Item names its row in assets/data/weapons.data.json.
local function row(tool)
	local key = string.split(tool.Item, ":")[2]
	return key and weapons:Get(key)
end

for _, tool in workspace:GetChildren() do
	if tool:IsA("Tool") then
		tool.Equipped:Connect(function()
			print("In hand: " .. tool.Name)
		end)
		tool.Activated:Connect(function()
			local item = row(tool)
			print(string.format("%s hits for %d", tool.Name, item and item.Damage or 0))
		end)
	end
end

inventory.ChildAdded:Connect(function(child)
	if child:IsA("Tool") then
		print(string.format("Picked up the %s (%d of %d)", child.Name, #inventory:GetItems(), inventory.Capacity))
	end
end)
