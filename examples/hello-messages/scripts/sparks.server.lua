-- Every second a spark appears, which Debris clears two seconds later, and
-- the score goes up by double one point.
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Debris = game:GetService("Debris")

while true do
    local spark = Instance.new("Part")
    spark.Name = "Spark"
    spark.Size = Vector3.new(1, 1, 1)
    spark.Color = Color3.new(1, 0.9, 0.3)
    spark.Anchored = true
    spark.Position = Vector3.new(math.random(-8, 8), 7, math.random(-8, 8))
    spark.Parent = workspace
    Debris:AddItem(spark, 2)

    ReplicatedStorage.Scored:Fire(ReplicatedStorage.Double:Invoke(1))
    task.wait(1)
end
