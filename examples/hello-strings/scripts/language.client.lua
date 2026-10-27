-- The Language button steps through the string table's languages. Every
-- label and button with a TextKey shows its text in the new one.
local LocalizationService = game:GetService("LocalizationService")
local panel = workspace.Menu.Panel

panel.LanguageButton.Activated:Connect(function()
    local languages = LocalizationService.Languages
    local next = 1
    for index, language in languages do
        if language == LocalizationService.Language then
            next = index % #languages + 1
        end
    end
    LocalizationService.Language = languages[next]
    print("Language: " .. LocalizationService.Language)
end)

panel.PlayButton.Activated:Connect(function()
    print(LocalizationService:GetString("menu.play"))
end)
