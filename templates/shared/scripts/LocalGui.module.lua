-- Each device's own copy of a ScreenGui. ScreenGuis live in the Workspace,
-- and in a hosted game the host's Workspace is everyone's, so a menu the
-- host opens would open for every player. Instead, the first device to
-- claim a ScreenGui keeps it, and every other device plays with a copy of
-- its own, which no one else sees. In single-player you always get the
-- ScreenGui itself.
--
--     local LocalGui = require(game.ReplicatedStorage:WaitForChild("LocalGui"))
--     local hud = LocalGui.claim("Hud")
--
-- Change what's inside a claimed ScreenGui freely; leave its Enabled alone
-- on the original, since other devices switch theirs off.
local Players = game:GetService("Players")

local LocalGui = {}

function LocalGui.claim(name: string)
	local me = Players.LocalPlayer.Name
	local original = workspace:WaitForChild(name)
	local owner = original:GetAttribute("Owner")
	if owner == nil or owner == me then
		original:SetAttribute("Owner", me)
		return original
	end
	local copy = original:Clone()
	original.Enabled = false
	copy:SetAttribute("Owner", me)
	copy.Parent = workspace
	return copy
end

return LocalGui
