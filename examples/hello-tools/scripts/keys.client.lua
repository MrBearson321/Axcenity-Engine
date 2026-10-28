-- K saves to slot 1 and L loads it: the Inventory, the Tool in hand and
-- the Tools left on the ground come back as they were.
local UserInputService = game:GetService("UserInputService")
local SaveService = game:GetService("SaveService")

UserInputService.InputBegan:Connect(function(input, processed)
	if processed then
		return
	end
	if input.KeyCode == Enum.KeyCode.K then
		SaveService:SaveSlot(1)
		print("Saved to slot 1")
	elseif input.KeyCode == Enum.KeyCode.L then
		local ok, problem = pcall(SaveService.LoadSlot, SaveService, 1)
		print(if ok then "Loaded slot 1" else problem)
	end
end)
