-- Part Tools: a dock tab with a Slider and a button. The slider picks a
-- transparency; Apply gives it to the selected parts as one undo step.
-- `widget.css`, beside this file, styles the tab. A handle in the Scene
-- View changes the selected part's height.

local widget = plugin:CreateDockWidget("PartTools", { Title = "Part Tools" })

local panel = Instance.new("Frame")
panel.Class = "panel"
panel.Parent = widget

local label = Instance.new("TextLabel")
label.Class = "caption"
label.Parent = panel

local slider = Instance.new("Slider")
slider.Id = "transparency"
slider.MinValue = 0
slider.MaxValue = 100
slider.Step = 5
slider.Value = plugin:GetSetting("transparency") or 50
slider.Parent = panel

local apply = Instance.new("TextButton")
apply.Class = "apply"
apply.Text = "Apply"
apply.Parent = panel

local function show()
	label.Text = `Transparency: {slider.Value}%`
end
show()

slider.ValueChanged:Connect(function()
	show()
	plugin:SetSetting("transparency", slider.Value)
end)

apply.Activated:Connect(function()
	for _, instance in Selection:Get() do
		if instance:IsA("BasePart") then
			instance.Transparency = slider.Value / 100
		end
	end
	ChangeHistoryService:SetWaypoint("Set Transparency")
end)

-- A handle on top of the selected part: drag it up or down to change the
-- part's height, keeping its bottom where it is. Each drag is one undo step.
local heightHandle = nil

local function placeHandle()
	if heightHandle then
		heightHandle:Remove()
		heightHandle = nil
	end
	local selected = Selection:Get()
	local part = selected[#selected]
	if not (part and part:IsA("BasePart")) then
		return
	end
	local bottom = part.Position.Y - part.Size.Y / 2
	local top = part.Position + Vector3.new(0, part.Size.Y / 2, 0)
	heightHandle = plugin:CreateHandle(top, { Axis = Vector3.new(0, 1, 0), Color = Color3.fromRGB(120, 220, 120) })
	heightHandle.Dragged:Connect(function(at)
		local height = math.max(0.2, at.Y - bottom)
		part.Size = Vector3.new(part.Size.X, height, part.Size.Z)
		part.Position = Vector3.new(part.Position.X, bottom + height / 2, part.Position.Z)
	end)
	heightHandle.DragEnded:Connect(function()
		ChangeHistoryService:SetWaypoint("Resize Height")
		placeHandle()
	end)
end

Selection.SelectionChanged:Connect(placeHandle)
placeHandle()
