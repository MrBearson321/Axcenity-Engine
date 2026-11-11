-- XB10k: lifts 100 drops back up every second, so some always fall.
local drops = {}
for j = 0, 999 do
    drops[j + 1] = workspace.Drops:FindFirstChild("D" .. j)
end
local n = 1
while true do
    task.wait(1)
    for _ = 1, 100 do
        local drop = drops[n]
        local x = (n % 10) * 3 - 13.5
        local z = (math.floor(n / 10) % 10) * 3 - 13.5
        drop.Position = Vector3.new(x, 40, z)
        drop.AssemblyLinearVelocity = Vector3.zero
        drop.AssemblyAngularVelocity = Vector3.zero
        n = n % #drops + 1
    end
end
