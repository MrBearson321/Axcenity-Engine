-- Align to Grid: a ribbon button that snaps the selected parts to whole
-- studs. The whole snap is one undo step. While parts are selected, the
-- Scene View shows where each would go.

local GRID = 1

local function snap(value)
	return math.floor(value / GRID + 0.5) * GRID
end

local GUIDE = { Color = Color3.fromRGB(80, 200, 255) }
local GUIDE_LINE = { Color = Color3.fromRGB(80, 200, 255), Thickness = 1 }

-- Drawings stay until they're cleared, so redraw only when something changes.
local function showTargets()
	plugin:ClearDrawings()
	for _, instance in Selection:Get() do
		if instance:IsA("BasePart") then
			local position = instance.Position
			local target = Vector3.new(snap(position.X), snap(position.Y), snap(position.Z))
			local offset = target - position
			if offset.Magnitude > 0.001 then
				plugin:DrawBox(instance.CFrame + offset, instance.Size, GUIDE)
				plugin:DrawLine(position, target, GUIDE_LINE)
			end
		end
	end
end

Selection.SelectionChanged:Connect(showTargets)

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
	showTargets()
end)
