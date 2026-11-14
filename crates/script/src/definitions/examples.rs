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
    // Scripting priority 2 (PR 5): more of the most used entries.
    (
        "Vector3.Magnitude",
        r#"local offset = workspace.Player.Position - workspace.Goal.Position
if offset.Magnitude < 10 then
	print("Almost there!")
end"#,
    ),
    (
        "Vector3.Unit",
        r#"local direction = (workspace.Target.Position - workspace.Cannon.Position).Unit
workspace.Ball.AssemblyLinearVelocity = direction * 80"#,
    ),
    (
        "Vector3.Lerp",
        r#"local from = Vector3.new(0, 5, 0)
local to = Vector3.new(20, 5, 0)
workspace.Platform.Position = from:Lerp(to, 0.5) -- halfway"#,
    ),
    (
        "Vector3.Dot",
        r#"local door = workspace.Door
local toPlayer = (workspace.Player.Position - door.Position).Unit
if door.CFrame.LookVector:Dot(toPlayer) > 0 then
	print("The player is in front of the door")
end"#,
    ),
    (
        "Vector2.new",
        r#"local center = Vector2.new(400, 300)
print(center.X, center.Y)"#,
    ),
    (
        "CFrame.new",
        r#"-- A position, then a position looking at a point.
workspace.Crate.CFrame = CFrame.new(0, 3, 0)
workspace.Camera.CFrame = CFrame.new(Vector3.new(0, 10, 20), Vector3.new(0, 0, 0))"#,
    ),
    (
        "CFrame.Angles",
        r#"local part = workspace.Spinner
part.CFrame = part.CFrame * CFrame.Angles(0, math.rad(45), 0)"#,
    ),
    (
        "CFrame.LookVector",
        r#"local cannon = workspace.Cannon
local ball = workspace.Ball
ball.Position = cannon.Position + cannon.CFrame.LookVector * 4
ball.AssemblyLinearVelocity = cannon.CFrame.LookVector * 100"#,
    ),
    (
        "CFrame.ToWorldSpace",
        r#"local car = workspace.Car
-- A point 2 studs above and 5 behind the car, wherever it faces.
local behind = car.CFrame:ToWorldSpace(CFrame.new(0, 2, 5))
workspace.Camera.CFrame = behind"#,
    ),
    (
        "Color3.fromHSV",
        r#"-- Cycle a part through the rainbow.
local hue = 0
while true do
	hue = (hue + 0.01) % 1
	workspace.Disco.Color = Color3.fromHSV(hue, 1, 1)
	task.wait()
end"#,
    ),
    (
        "UDim2.new",
        r#"local button = script.Parent
-- Scale and offset: full width less 20 pixels, 40 pixels tall.
button.Size = UDim2.new(1, -20, 0, 40)"#,
    ),
    (
        "TweenInfo.new",
        r#"local TweenService = game:GetService("TweenService")
local info = TweenInfo.new(2, Enum.EasingStyle.Quad, Enum.EasingDirection.Out, -1, true)
TweenService:Create(workspace.Platform, info, { Position = Vector3.new(0, 20, 0) }):Play()"#,
    ),
    (
        "Tween.Completed",
        r#"local TweenService = game:GetService("TweenService")
local tween = TweenService:Create(workspace.Door, TweenInfo.new(1), { Transparency = 1 })
tween.Completed:Connect(function()
	workspace.Door.CanCollide = false
end)
tween:Play()"#,
    ),
    (
        "Random.new",
        r#"local random = Random.new()
local x = random:NextNumber(-50, 50)
local coins = random:NextInteger(1, 6)
print(x, coins)"#,
    ),
    (
        "Signal.Once",
        r#"workspace.Button.Touched:Once(function(hit)
	print(hit.Name, "pressed the button first")
end)"#,
    ),
    (
        "Signal.Wait",
        r#"local Players = game:GetService("Players")
local player = Players.PlayerAdded:Wait()
print(player.Name, "joined first")"#,
    ),
    (
        "Connection.Disconnect",
        r#"local connection
connection = workspace.Lava.Touched:Connect(function(hit)
	print(hit.Name, "touched the lava")
	connection:Disconnect() -- only the first touch
end)"#,
    ),
    (
        "RunService.RenderStepped",
        r#"-- In a LocalScript: runs before each frame is drawn.
local RunService = game:GetService("RunService")
RunService.RenderStepped:Connect(function(deltaTime)
	workspace.Compass.Orientation += Vector3.new(0, 90 * deltaTime, 0)
end)"#,
    ),
    (
        "RunService.IsServer",
        r#"local RunService = game:GetService("RunService")
if RunService:IsServer() then
	print("Running on the server")
else
	print("Running on a player's device")
end"#,
    ),
    (
        "UserInputService.IsKeyDown",
        r#"local UserInputService = game:GetService("UserInputService")
game:GetService("RunService").Heartbeat:Connect(function()
	if UserInputService:IsKeyDown(Enum.KeyCode.LeftShift) then
		print("Sprinting")
	end
end)"#,
    ),
    (
        "ContextActionService.BindAction",
        r#"local ContextActionService = game:GetService("ContextActionService")
local function reload(actionName, inputState)
	if inputState == Enum.UserInputState.Begin then
		print("Reloading")
	end
end
ContextActionService:BindAction("Reload", reload, true, Enum.KeyCode.R)"#,
    ),
    (
        "CollectionService.AddTag",
        r#"local CollectionService = game:GetService("CollectionService")
CollectionService:AddTag(workspace.Spikes, "Hazard")
for _, hazard in CollectionService:GetTagged("Hazard") do
	hazard.Color = Color3.fromRGB(255, 0, 0)
end"#,
    ),
    (
        "CollectionService.GetInstanceAddedSignal",
        r#"local CollectionService = game:GetService("CollectionService")
CollectionService:GetInstanceAddedSignal("Coin"):Connect(function(coin)
	coin.Transparency = 0
end)"#,
    ),
    (
        "HttpService.JSONEncode",
        r#"local HttpService = game:GetService("HttpService")
local text = HttpService:JSONEncode({ level = 3, coins = { 1, 2, 3 } })
local back = HttpService:JSONDecode(text)
print(text, back.level)"#,
    ),
    (
        "Instance.GetDescendants",
        r#"for _, item in workspace.House:GetDescendants() do
	if item:IsA("BasePart") then
		item.Anchored = true
	end
end"#,
    ),
    (
        "Instance.FindFirstChildOfClass",
        r#"local character = workspace:FindFirstChild("Hero")
local humanoid = character and character:FindFirstChildOfClass("Humanoid")
if humanoid then
	humanoid.WalkSpeed = 24
end"#,
    ),
    (
        "Instance.FindFirstAncestorOfClass",
        r#"workspace.Lava.Touched:Connect(function(hit)
	local model = hit:FindFirstAncestorOfClass("Model")
	if model then
		print(model.Name, "touched the lava")
	end
end)"#,
    ),
    (
        "Instance.GetAttribute",
        r#"local door = workspace.Door
local key = door:GetAttribute("KeyColor") or "Red"
print("This door needs the", key, "key")"#,
    ),
    (
        "Instance.GetAttributeChangedSignal",
        r#"local door = workspace.Door
door:GetAttributeChangedSignal("Open"):Connect(function()
	door.CanCollide = not door:GetAttribute("Open")
end)"#,
    ),
    (
        "Instance.Destroying",
        r#"workspace.Bomb.Destroying:Connect(function()
	print("The bomb is gone")
end)"#,
    ),
    (
        "Instance.ClearAllChildren",
        r#"-- Empty the folder of last round's coins.
workspace.Coins:ClearAllChildren()"#,
    ),
    (
        "Instance.Parent",
        r#"local coin = workspace.Coin:Clone()
coin.Position = Vector3.new(10, 3, 0)
coin.Parent = workspace -- it appears once it has a parent"#,
    ),
    (
        "Part.Transparency",
        r#"local wall = workspace.SecretWall
for i = 0, 10 do
	wall.Transparency = i / 10
	task.wait(0.05)
end
wall.CanCollide = false"#,
    ),
    (
        "Part.CanCollide",
        r#"workspace.GhostDoor.CanCollide = false -- players walk through it"#,
    ),
    (
        "Part.Material",
        r#"workspace.Floor.Material = Enum.Material.Wood"#,
    ),
    (
        "Part.Size",
        r#"local balloon = workspace.Balloon
for _ = 1, 20 do
	balloon.Size += Vector3.new(0.2, 0.2, 0.2)
	task.wait(0.1)
end"#,
    ),
    (
        "Part.Position",
        r#"local platform = workspace.Platform
local start = platform.Position
while true do
	platform.Position = start + Vector3.new(0, math.sin(os.clock()) * 5, 0)
	task.wait()
end"#,
    ),
    (
        "Part.AssemblyLinearVelocity",
        r#"-- A jump pad: throw whatever touches it upwards.
workspace.JumpPad.Touched:Connect(function(hit)
	if not hit.Anchored then
		hit.AssemblyLinearVelocity = Vector3.new(0, 80, 0)
	end
end)"#,
    ),
    (
        "Part.TouchEnded",
        r#"local plate = workspace.PressurePlate
plate.Touched:Connect(function()
	plate.Color = Color3.fromRGB(0, 255, 0)
end)
plate.TouchEnded:Connect(function()
	plate.Color = Color3.fromRGB(255, 0, 0)
end)"#,
    ),
    (
        "Model.GetPivot",
        r#"local house = workspace.House
local where = house:GetPivot()
house:PivotTo(where * CFrame.new(0, 0, -10)) -- move it 10 studs forward"#,
    ),
    (
        "Humanoid.Died",
        r#"local humanoid = workspace.Hero:WaitForChild("Humanoid")
humanoid.Died:Connect(function()
	print("Game over")
end)"#,
    ),
    (
        "Humanoid.MoveTo",
        r#"local guard = workspace.Guard.Humanoid
guard:MoveTo(workspace.Gate.Position)
guard.MoveToFinished:Wait()
print("The guard reached the gate")"#,
    ),
    (
        "Humanoid.WalkSpeed",
        r#"workspace.SpeedPad.Touched:Connect(function(hit)
	local humanoid = hit.Parent:FindFirstChildOfClass("Humanoid")
	if humanoid then
		humanoid.WalkSpeed = 32
	end
end)"#,
    ),
    (
        "Humanoid.Health",
        r#"local humanoid = workspace.Hero.Humanoid
humanoid:GetPropertyChangedSignal("Health"):Connect(function()
	print("Health is now", humanoid.Health, "of", humanoid.MaxHealth)
end)"#,
    ),
    (
        "Players.PlayerRemoving",
        r#"local Players = game:GetService("Players")
Players.PlayerRemoving:Connect(function(player)
	print(player.Name, "left the game")
end)"#,
    ),
    (
        "Players.GetPlayers",
        r#"local Players = game:GetService("Players")
for _, player in Players:GetPlayers() do
	print(player.Name)
end"#,
    ),
    (
        "Players.LocalPlayer",
        r#"-- In a LocalScript: the player on this device.
local player = game:GetService("Players").LocalPlayer
print("Hello,", player.DisplayName)"#,
    ),
    (
        "Player.CharacterAdded",
        r#"local Players = game:GetService("Players")
Players.PlayerAdded:Connect(function(player)
	player.CharacterAdded:Connect(function(character)
		print(player.Name, "spawned as", character.Name)
	end)
end)"#,
    ),
    (
        "RemoteEvent.FireClient",
        r#"-- On the server: tell one player's device to show a message.
local message = game.ReplicatedStorage.ShowMessage
game:GetService("Players").PlayerAdded:Connect(function(player)
	message:FireClient(player, "Welcome!")
end)"#,
    ),
    (
        "RemoteEvent.OnClientEvent",
        r#"-- In a LocalScript.
game.ReplicatedStorage.ShowMessage.OnClientEvent:Connect(function(text)
	print("The server says:", text)
end)"#,
    ),
    (
        "RemoteFunction.InvokeServer",
        r#"-- In a LocalScript: ask the server and wait for its answer.
local coins = game.ReplicatedStorage.GetCoins:InvokeServer()
print("I have", coins, "coins")"#,
    ),
    (
        "RemoteFunction.OnServerInvoke",
        r#"game.ReplicatedStorage.GetCoins.OnServerInvoke = function(player)
	return player:GetAttribute("Coins") or 0
end"#,
    ),
    (
        "BindableEvent.Fire",
        r#"local roundEnded = game.ServerStorage.RoundEnded
roundEnded.Event:Connect(function(winner)
	print(winner, "won the round")
end)
roundEnded:Fire("Red team")"#,
    ),
    (
        "Sound.Ended",
        r#"local music = workspace.Music
music.Ended:Connect(function()
	print("The song finished")
end)
music:Play()"#,
    ),
    (
        "ClickDetector.MouseClick",
        r#"workspace.Lever.ClickDetector.MouseClick:Connect(function(player)
	print(player.Name, "pulled the lever")
	workspace.Gate.Transparency = 1
	workspace.Gate.CanCollide = false
end)"#,
    ),
    (
        "Workspace.Gravity",
        r#"workspace.Gravity = 30 -- low gravity, like the moon"#,
    ),
    (
        "Workspace.CurrentCamera",
        r#"-- In a LocalScript.
local camera = workspace.CurrentCamera
camera.FieldOfView = 90"#,
    ),
    (
        "Workspace.GetPartsInPart",
        r#"local zone = workspace.SafeZone
for _, part in workspace:GetPartsInPart(zone) do
	print(part.Name, "is in the safe zone")
end"#,
    ),
    (
        "RaycastParams.new",
        r#"local params = RaycastParams.new()
params.FilterDescendantsInstances = { workspace.Hero }
params.FilterType = Enum.RaycastFilterType.Exclude
local result = workspace:Raycast(workspace.Hero.Head.Position, Vector3.new(0, -50, 0), params)
if result then
	print("Standing on", result.Instance.Name)
end"#,
    ),
    (
        "Camera.WorldToScreenPoint",
        r#"-- In a LocalScript: where the treasure is on screen.
local camera = workspace.CurrentCamera
local point, visible = camera:WorldToScreenPoint(workspace.Treasure.Position)
if visible then
	print("The treasure is at", point.X, point.Y)
end"#,
    ),
    (
        "TextLabel.Text",
        r#"local label = script.Parent
for seconds = 10, 0, -1 do
	label.Text = "Starting in " .. seconds
	task.wait(1)
end"#,
    ),
    (
        "TextBox.FocusLost",
        r#"local box = script.Parent
box.FocusLost:Connect(function(enterPressed)
	if enterPressed then
		print("You typed:", box.Text)
	end
end)"#,
    ),
    (
        "AnimationTrack.Stopped",
        r#"local animator = workspace.Hero.Humanoid.Animator
local wave = animator:LoadAnimation(workspace.Hero.Wave)
wave.Stopped:Connect(function()
	print("Done waving")
end)
wave:Play()"#,
    ),
    (
        "ProximityPrompt.HoldDuration",
        r#"local prompt = workspace.Chest.ProximityPrompt
prompt.ActionText = "Open"
prompt.HoldDuration = 1.5 -- hold the key for a second and a half
prompt.Triggered:Connect(function(player)
	print(player.Name, "opened the chest")
end)"#,
    ),
    (
        "Tool.Activated",
        r#"local tool = script.Parent
tool.Activated:Connect(function()
	print("Swing!")
end)"#,
    ),
    (
        "SaveService.SetPlayerData",
        r#"local SaveService = game:GetService("SaveService")
game:GetService("Players").PlayerRemoving:Connect(function(player)
	SaveService:SetPlayerData(player, "Coins", player:GetAttribute("Coins") or 0)
end)"#,
    ),
    (
        "DataService.GetTable",
        r#"local DataService = game:GetService("DataService")
local enemies = DataService:GetTable("Enemies")
local goblin = enemies:Get("Goblin")
print(goblin)"#,
    ),
    (
        "PhysicsService.CollisionGroupSetCollidable",
        r#"local PhysicsService = game:GetService("PhysicsService")
PhysicsService:RegisterCollisionGroup("Ghosts")
PhysicsService:CollisionGroupSetCollidable("Ghosts", "Ghosts", false)
workspace.GhostA.CollisionGroup = "Ghosts"
workspace.GhostB.CollisionGroup = "Ghosts""#,
    ),
    (
        "task.cancel",
        r#"local countdown = task.delay(10, function()
	print("Boom")
end)
workspace.DefuseButton.Touched:Once(function()
	task.cancel(countdown)
	print("Defused!")
end)"#,
    ),
    (
        "task.defer",
        r#"task.defer(function()
	print("Runs after this script finishes its first run")
end)
print("Runs first")"#,
    ),
];

/// Every example, as `(qualified name, Luau)`, for tests that check them
/// further (the editor lints each against the API).
pub fn api_examples() -> &'static [(&'static str, &'static str)] {
    EXAMPLES
}

/// The example for the entry called `qualified` (`Instance.FindFirstChild`),
/// if it has one.
pub fn api_example(qualified: &str) -> Option<&'static str> {
    EXAMPLES
        .iter()
        .find(|(name, _)| *name == qualified)
        .map(|(_, code)| *code)
}
