-- While the Farmer's quest is active, walking up to an apple picks it.
-- The dialogue and the quest itself need no script: the Farmer's
-- ProximityPrompt names his dialogue, and the quest is in assets/data.
local QuestService = game:GetService("QuestService")
local RunService = game:GetService("RunService")
local body = workspace.Character.Body

RunService.Heartbeat:Connect(function()
	if QuestService:GetState("apples") ~= "Active" then
		return
	end
	for _, apple in workspace.Apples:GetChildren() do
		if (apple.Position - body.Position).Magnitude < 3 then
			apple:Destroy()
			QuestService:Progress("Apple")
		end
	end
end)

QuestService.QuestCompleted:Connect(function(quest)
	print("Quest complete:", QuestService:GetTitle(quest))
end)
