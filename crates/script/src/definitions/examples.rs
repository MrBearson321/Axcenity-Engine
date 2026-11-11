//! Short usage examples for the API's most used entries, shown on their page
//! of the editor's API Reference and in the generated reference pages
//! (`documentation/api/`). Each is checked: its entry must exist and its
//! code must compile (`every_example_names_a_real_entry_and_compiles`).
//!
//! Keep them short and runnable as they stand in a Script (or a LocalScript,
//! for input and UI), naming objects a reader would recognise.

/// `(qualified name, Luau)`: the entry as `api_example` looks it up, such as
/// `Instance.FindFirstChild` or `task.wait`.
pub(super) const EXAMPLES: &[(&str, &str)] = &[
    (
        "Instance.new",
        r#"local part = Instance.new("Part")
part.Size = Vector3.new(4, 1, 4)
part.Position = Vector3.new(0, 10, 0)
part.Parent = workspace"#,
    ),
    (
        "Instance.FindFirstChild",
        r#"local door = workspace:FindFirstChild("Door")
if door then
	door.Transparency = 0.5
end"#,
    ),
    (
        "Instance.WaitForChild",
        r#"local gui = workspace:WaitForChild("MainMenu", 10)
if gui == nil then
	print("The menu never arrived")
end"#,
    ),
    (
        "Instance.GetChildren",
        r#"for _, child in workspace.Coins:GetChildren() do
	child.Transparency = 0
end"#,
    ),
    (
        "Instance.IsA",
        r#"for _, thing in workspace:GetDescendants() do
	if thing:IsA("Part") then
		thing.Anchored = true
	end
end"#,
    ),
    (
        "Instance.Clone",
        r#"local copy = workspace.Crate:Clone()
copy.Position = workspace.Crate.Position + Vector3.new(0, 5, 0)
copy.Parent = workspace"#,
    ),
    (
        "Instance.Destroy",
        r#"local coin = workspace.Coin
coin.Touched:Connect(function()
	coin:Destroy()
end)"#,
    ),
    (
        "Instance.SetAttribute",
        r#"local chest = workspace.Chest
chest:SetAttribute("Gold", 50)
print(chest:GetAttribute("Gold"))"#,
    ),
    (
        "Instance.GetPropertyChangedSignal",
        r#"local lamp = workspace.Lamp
lamp:GetPropertyChangedSignal("Color"):Connect(function()
	print("The lamp is now", lamp.Color)
end)"#,
    ),
    (
        "Instance.Changed",
        r#"workspace.Door.Changed:Connect(function(property)
	print("Door changed:", property)
end)"#,
    ),
    (
        "Instance.ChildAdded",
        r#"workspace.ChildAdded:Connect(function(child)
	print(child.Name, "was added to the Workspace")
end)"#,
    ),
    (
        "DataModel.GetService",
        r#"local TweenService = game:GetService("TweenService")
local Players = game:GetService("Players")"#,
    ),
    (
        "Part.Touched",
        r#"local pad = workspace.Pad
pad.Touched:Connect(function(other)
	print(other.Name, "touched the pad")
end)"#,
    ),
    (
        "Part.Anchored",
        r#"local bridge = workspace.Bridge
task.wait(3)
bridge.Anchored = false -- it falls"#,
    ),
    (
        "Part.ApplyImpulse",
        r#"local ball = workspace.Ball
ball:ApplyImpulse(Vector3.new(0, 500, 0))"#,
    ),
    (
        "Part.Color",
        r#"workspace.Wall.Color = Color3.fromRGB(200, 60, 40)"#,
    ),
    (
        "Model.PivotTo",
        r#"local house = workspace.House
house:PivotTo(CFrame.new(0, 0, 50) * CFrame.Angles(0, math.rad(90), 0))"#,
    ),
    (
        "Workspace.Raycast",
        r#"local hit = workspace:Raycast(Vector3.new(0, 50, 0), Vector3.new(0, -100, 0))
if hit then
	print("Ground at", hit.Position, "on", hit.Instance.Name)
end"#,
    ),
    (
        "Workspace.GetPartBoundsInRadius",
        r#"for _, part in workspace:GetPartBoundsInRadius(Vector3.new(0, 0, 0), 10) do
	print(part.Name, "is near the middle")
end"#,
    ),
    (
        "task.wait",
        r#"while true do
	task.wait(1)
	print("Another second")
end"#,
    ),
    (
        "task.spawn",
        r#"task.spawn(function()
	task.wait(2)
	print("This runs alongside")
end)
print("This runs first")"#,
    ),
    (
        "task.delay",
        r#"task.delay(5, function()
	workspace.Door.CanCollide = false
end)"#,
    ),
    (
        "Signal.Connect",
        r#"local connection = workspace.Button.Touched:Connect(function()
	print("Pressed")
end)
task.wait(10)
connection:Disconnect()"#,
    ),
    (
        "Vector3.new",
        r#"local a = Vector3.new(1, 2, 3)
local b = Vector3.new(4, 0, 0)
print(a + b, a.Magnitude, (b - a).Unit)"#,
    ),
    (
        "CFrame.lookAt",
        r#"local turret = workspace.Turret
turret.CFrame = CFrame.lookAt(turret.Position, workspace.Target.Position)"#,
    ),
    (
        "Color3.fromRGB",
        r#"workspace.Sign.Color = Color3.fromRGB(255, 200, 0)"#,
    ),
    (
        "UDim2.fromScale",
        r#"local panel = workspace.HUD.Panel
panel.Size = UDim2.fromScale(0.5, 0.25)
panel.Position = UDim2.new(0.5, -100, 0, 20)"#,
    ),
    (
        "TweenService.Create",
        r#"local TweenService = game:GetService("TweenService")
local door = workspace.Door
local info = TweenInfo.new(1, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
local tween = TweenService:Create(door, info, { Position = door.Position + Vector3.new(0, 8, 0) })
tween:Play()"#,
    ),
    (
        "RunService.Heartbeat",
        r#"local RunService = game:GetService("RunService")
local spinner = workspace.Spinner
RunService.Heartbeat:Connect(function(dt)
	spinner.CFrame = spinner.CFrame * CFrame.Angles(0, dt, 0)
end)"#,
    ),
    (
        "UserInputService.InputBegan",
        r#"-- In a LocalScript
local UserInputService = game:GetService("UserInputService")
UserInputService.InputBegan:Connect(function(input)
	if input.KeyCode == Enum.KeyCode.E then
		print("E pressed")
	end
end)"#,
    ),
    (
        "CollectionService.GetTagged",
        r#"local CollectionService = game:GetService("CollectionService")
for _, lamp in CollectionService:GetTagged("Lamp") do
	lamp.Material = Enum.Material.Neon
end"#,
    ),
    (
        "Debris.AddItem",
        r#"local Debris = game:GetService("Debris")
local spark = Instance.new("Part")
spark.Parent = workspace
Debris:AddItem(spark, 2)"#,
    ),
    (
        "Players.PlayerAdded",
        r#"local Players = game:GetService("Players")
Players.PlayerAdded:Connect(function(player)
	print(player.DisplayName, "joined")
end)"#,
    ),
    (
        "RemoteEvent.FireServer",
        r#"-- In a LocalScript
local shout = game.ReplicatedStorage.Shout
shout:FireServer("Hello")"#,
    ),
    (
        "RemoteEvent.OnServerEvent",
        r#"local shout = game.ReplicatedStorage.Shout
shout.OnServerEvent:Connect(function(player, message)
	print(player.Name, "says", message)
end)"#,
    ),
    (
        "Sound.Play",
        r#"local chime = workspace.Bell.Chime
chime:Play()
chime.Ended:Connect(function()
	print("Done")
end)"#,
    ),
    (
        "Animator.SetTrigger",
        r#"local animator = workspace.Hero.Animator
animator:SetNumber("Speed", 12)
animator:SetTrigger("Jump")"#,
    ),
    (
        "Animator.LoadAnimation",
        r#"local animator = workspace.Hero.Animator
local wave = animator:LoadAnimation(workspace.Hero.Wave)
wave:Play(0.2)"#,
    ),
    (
        "TextButton.Activated",
        r#"-- In a LocalScript
local menu = workspace.MainMenu
menu.Buttons.Play.Activated:Connect(function()
	menu.Enabled = false
end)"#,
    ),
    (
        "ProximityPrompt.Triggered",
        r#"local prompt = workspace.Door.ProximityPrompt
prompt.Triggered:Connect(function(player)
	workspace.Door.CanCollide = false
end)"#,
    ),
    (
        "Humanoid.TakeDamage",
        r#"local lava = workspace.Lava
lava.Touched:Connect(function(other)
	local humanoid = other.Parent:FindFirstChildOfClass("Humanoid")
	if humanoid then
		humanoid:TakeDamage(10)
	end
end)"#,
    ),
    (
        "SaveService.SaveSlot",
        r#"local SaveService = game:GetService("SaveService")
SaveService:SaveSlot(1)
if SaveService:HasSlot(1) then
	SaveService:LoadSlot(1)
end"#,
    ),
    (
        "LevelService.LoadLevel",
        r#"local LevelService = game:GetService("LevelService")
LevelService:LoadLevel("Castle", "Gate")"#,
    ),
    (
        "ParticleEmitter.Emit",
        r#"workspace.Fireworks.ParticleEmitter:Emit(50)"#,
    ),
    (
        "ConsoleService.RegisterCommand",
        r#"local ConsoleService = game:GetService("ConsoleService")
ConsoleService:RegisterCommand("heal", "Fills everyone's health", function(player)
	print(player.Name, "asked to heal")
end)"#,
    ),
];

/// The example for the entry called `qualified` (`Instance.FindFirstChild`),
/// if it has one.
pub fn api_example(qualified: &str) -> Option<&'static str> {
    EXAMPLES
        .iter()
        .find(|(name, _)| *name == qualified)
        .map(|(_, code)| *code)
}
