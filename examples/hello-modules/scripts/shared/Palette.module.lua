-- A ModuleScript: code that other scripts load with `require`. This file
-- becomes `game.ReplicatedStorage.Palette`. It runs once on each side, the
-- first time a script there requires it, and every script that requires it
-- gets the same table back.

local Palette = {}

Palette.colors = {
    Color3.fromRGB(239, 71, 111),
    Color3.fromRGB(255, 209, 102),
    Color3.fromRGB(6, 214, 160),
    Color3.fromRGB(17, 138, 178),
}

-- The color for step number `n`, going round the list.
function Palette.color(n)
    return Palette.colors[(n - 1) % #Palette.colors + 1]
end

-- A module returns exactly one value, usually a table of functions.
return Palette
