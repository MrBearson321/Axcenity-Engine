-- L switches between English and Spanish; the dialogue box and the quest
-- tracker change at once.
local UserInputService = game:GetService("UserInputService")
local LocalizationService = game:GetService("LocalizationService")

UserInputService.InputBegan:Connect(function(input)
	if input.KeyCode == Enum.KeyCode.L then
		if LocalizationService.Language == "English" then
			LocalizationService.Language = "Español"
		else
			LocalizationService.Language = "English"
		end
	end
end)
