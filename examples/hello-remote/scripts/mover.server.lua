-- A Script: it runs on the server, which decides what really happens. When
-- a player's LocalScript fires MoveRequest, this moves the block. Anything
-- a player sends could be wrong (or, later, a cheat), so check it first.

local MoveRequest = game.ReplicatedStorage.MoveRequest
local block = workspace.Mover

local SPEED = 12 -- studs per second
local EDGE = 18 -- how far from the middle the block may go

local function clamp(x)
    return math.max(-EDGE, math.min(EDGE, x))
end

-- The server's handler gets the player who sent the message first (unused
-- here; the `_` says so on purpose).
MoveRequest.OnServerEvent:Connect(function(_player, direction, seconds)
    if type(direction) ~= "userdata" or type(seconds) ~= "number" then
        return
    end
    -- At most a tenth of a second per message, so nobody can jump far.
    seconds = math.max(0, math.min(seconds, 0.1))
    local moved = block.Position + direction.Unit * SPEED * seconds
    block.Position = Vector3.new(clamp(moved.X), moved.Y, clamp(moved.Z))
end)
