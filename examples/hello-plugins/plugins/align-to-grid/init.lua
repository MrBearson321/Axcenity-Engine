-- Align to Grid: a ribbon button that snaps the selected parts to whole
-- studs. The whole snap is one undo step.

local GRID = 1

local function snap(value)
	return math.floor(value / GRID + 0.5) * GRID
end

local toolbar = plugin:CreateToolbar("Align")
local button = toolbar:CreateButton("Align to Grid", "Snap the selected parts to 1 stud", "grid-four")

button.Click:Connect(function()
	local moved = 0
	for _, instance in Selection:Get() do
		if instance:IsA("BasePart") then
			local position = instance.Position
			instance.Position = Vector3.new(snap(position.X), snap(position.Y), snap(position.Z))
			moved += 1
		end
	end
	ChangeHistoryService:SetWaypoint("Align to Grid")
	print(`Aligned {moved} part(s) to the grid`)
end)
