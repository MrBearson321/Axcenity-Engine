-- Keeps the score. Other Scripts add to it with the Scored BindableEvent,
-- and LocalScripts ask for it with the GetScore RemoteFunction.
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local HttpService = game:GetService("HttpService")

local score = 0

ReplicatedStorage.Scored.Event:Connect(function(points)
    score += points
end)

-- A BindableFunction: Scripts on this side call it with Invoke.
ReplicatedStorage.Double.OnInvoke = function(points)
    return points * 2
end

-- Answers InvokeServer: the score, and the same as JSON.
ReplicatedStorage.GetScore.OnServerInvoke = function(player)
    return score, HttpService:JSONEncode({ player = player.Name, score = score })
end
