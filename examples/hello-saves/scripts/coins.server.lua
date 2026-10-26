-- A Script. Walk into the yellow coins to collect them, and onto the green
-- pad to make it your checkpoint. Walk off the edge of the floor and you're
-- back on the pad, with the coins you had there. keys.client.blocks.json
-- saves to slot 1 (K), loads it (L) and goes back to the checkpoint (R),
-- from blocks. Slots are kept in your data for this game, not in the
-- project, and the next Play starts from slot 1 if there is one.

local SaveService = game:GetService("SaveService")
local RunService = game:GetService("RunService")
local hero = workspace.Hero.Body

if SaveService:HasSlot(1) then
    SaveService:LoadSlot(1)
    print("Loaded slot 1: " .. SaveService:GetPlayerData("Coins") .. " coins")
else
    SaveService:SetPlayerData("Coins", 0)
    print("A new game. K saves, L loads, R goes back to the checkpoint.")
end
SaveService:SetSetting("Volume", 0.8)

local function near(part, distance)
    return (hero.Position - part.Position).Magnitude < distance
end

RunService.Heartbeat:Connect(function()
    for i = 1, 3 do
        local coin = workspace.Coins:FindFirstChild("Coin" .. i)
        -- A collected coin is see-through; the save keeps that.
        if coin and coin.Transparency < 1 and near(coin, 2.5) then
            coin.Transparency = 1
            local coins = SaveService:GetPlayerData("Coins") + 1
            SaveService:SetPlayerData("Coins", coins)
            print("Coins: " .. coins)
        end
    end
    if SaveService:GetCheckpoint() ~= workspace.Checkpoint and near(workspace.Pad, 4) then
        SaveService:SetCheckpoint(workspace.Checkpoint)
        print("Checkpoint!")
    end
    if hero.Position.Y < -30 then
        if SaveService:GetCheckpoint() then
            SaveService:RestoreCheckpoint()
            print("Back at the checkpoint, with " .. SaveService:GetPlayerData("Coins") .. " coins")
        else
            hero.Position = Vector3.new(0, 2.5, 0)
        end
    end
end)
