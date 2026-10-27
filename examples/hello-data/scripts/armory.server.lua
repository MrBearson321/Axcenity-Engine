-- Reads the weapons table (assets/data/weapons.data.json, edited in the
-- Data Table tab): each weapon's part takes its Tint, grows with its
-- Damage, and floats by its Offset. Edit a cell during Play and the
-- parts follow it within a second.
local DataService = game:GetService("DataService")
local weapons = DataService:GetTable("weapons")

for _, key in weapons:Keys() do
    local weapon = weapons:Get(key)
    local ammo = if weapon.Ammo then weapon.Ammo.Count .. " " .. weapon.Ammo.Name else "no ammo"
    print(key, weapon.Kind, weapon.Damage, "damage,", ammo)
end

while true do
    for _, key in weapons:Keys() do
        local part = workspace:FindFirstChild(key)
        local weapon = weapons:Get(key)
        if part and weapon then
            part.Color = weapon.Tint
            part.Size = Vector3.new(1, 1 + weapon.Damage / 5, 1)
            part.Position = Vector3.new(part.Position.X, part.Size.Y / 2, 0) + weapon.Offset
        end
    end
    task.wait(1)
end
