-- Part Tools: a dock tab with a Slider and a button. The slider picks a
-- transparency; Apply gives it to the selected parts as one undo step.
-- `widget.css`, beside this file, styles the tab.

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
