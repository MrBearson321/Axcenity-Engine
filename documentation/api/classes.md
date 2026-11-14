---
description: "Instance classes: everything that can be in the tree."
---

# Classes

Instance classes: everything that can be in the tree.

| Name | What it is |
|---|---|
| [Instance](classes/Instance.md) | Anything in the game tree. |
| [DataModel](classes/DataModel.md) | The root of the game, called `game` in scripts. |
| [Workspace](classes/Workspace.md) | The service that holds the 3D world. |
| [Players](classes/Players.md) | The service with a Player for everyone in the game. |
| [Lighting](classes/Lighting.md) | The service for the world's lighting. |
| [ReplicatedStorage](classes/ReplicatedStorage.md) | The service for things Scripts and LocalScripts share, such as RemoteEvents and ModuleScripts. |
| [ServerScriptService](classes/ServerScriptService.md) | The service that holds server scripts (`scripts/*.server.lua`). |
| [ServerStorage](classes/ServerStorage.md) | The service for things only the server uses, such as Models a Script clones into the Workspace. |
| [StarterPlayer](classes/StarterPlayer.md) | The service with what each player starts with: StarterPlayerScripts, and the StarterCharacter each character is a copy of. |
| [StarterPlayerScripts](classes/StarterPlayerScripts.md) | Holds the LocalScripts (`scripts/*.client.lua`), which run on the player's side. |
| [Part](classes/Part.md) | A solid shape in the world. |
| [MeshPart](classes/MeshPart.md) | A solid 3D model imported from a glTF or FBX file, drawn with the file's own materials (or its SurfaceAppearances'). |
| [UnionOperation](classes/UnionOperation.md) | Parts combined into one solid by Union or Intersect (Model ▸ Solid Modeling), or by `UnionAsync`, `SubtractAsync` or `IntersectAsync`. |
| [NegateOperation](classes/NegateOperation.md) | A Part made negative by Negate (Model ▸ Solid Modeling): shown pink and see-through, and when it's unioned with other parts it cuts itself out of them, as a doorway out of a wall. |
| [Seat](classes/Seat.md) | A Part that characters sit on. |
| [VehicleSeat](classes/VehicleSeat.md) | A Seat that drives the car it's welded into: the body made of every part welded to it, the chassis. |
| [DamageZone](classes/DamageZone.md) | A Part that hurts: every Health whose Model has a part inside it loses DamagePerSecond a second, with the zone as the source. |
| [ReverbZone](classes/ReverbZone.md) | A Part the camera walks into: while the camera is inside it, its ReverbType blends onto every spatial Sound over FadeTime seconds, and out again on leaving. |
| [SpawnLocation](classes/SpawnLocation.md) | A Part characters spawn on: a player's character goes on top of an Enabled SpawnLocation of their team's TeamColor (or a Neutral one when the team has none), chosen at random. |
| [Script](classes/Script.md) | Lua code that runs on the server, from a `.server.lua` file. |
| [LocalScript](classes/LocalScript.md) | Lua code that runs on the player's side (input, camera), from a `.client.lua` file. |
| [ModuleScript](classes/ModuleScript.md) | Lua code that scripts load with `require`, from a `.module.lua` file. |
| [RemoteEvent](classes/RemoteEvent.md) | Carries messages between Scripts and LocalScripts. |
| [RemoteFunction](classes/RemoteFunction.md) | Asks the server and waits for the answer: a LocalScript's `InvokeServer(...)` pauses until the server's `OnServerInvoke` returns, and returns what it returned. |
| [BindableEvent](classes/BindableEvent.md) | An event scripts on the same side fire for each other: `Fire(...)` fires `Event` with the same values. |
| [BindableFunction](classes/BindableFunction.md) | A function scripts on the same side call for each other: `Invoke(...)` calls `OnInvoke` and returns what it returns. |
| [Player](classes/Player.md) | Someone playing the game, in Players. |
| [Model](classes/Model.md) | Groups the parts of one thing, such as a car or a house. |
| [Folder](classes/Folder.md) | Groups Instances to keep the tree tidy. |
| [SurfaceAppearance](classes/SurfaceAppearance.md) | Texture maps for a Part, or for one material of a MeshPart (named by MaterialName, in place of the file's own). |
| [ScreenGui](classes/ScreenGui.md) | The on-screen UI root, usually inside Workspace. |
| [Frame](classes/Frame.md) | A rectangular UI element that can contain other UI elements. |
| [TextLabel](classes/TextLabel.md) | A UI element that displays text. |
| [TextButton](classes/TextButton.md) | A clickable UI element. |
| [ImageLabel](classes/ImageLabel.md) | A UI element that displays an image from the project. |
| [TextBox](classes/TextBox.md) | A box the player can type in. |
| [PageLayout](classes/PageLayout.md) | Shows one child Frame at a time, chosen by CurrentPage (the Frame's Name). |
| [Table](classes/Table.md) | Lays its children out as equal cells, left to right then top to bottom. |
| [ScrollingFrame](classes/ScrollingFrame.md) | A Frame that scrolls: its children sit on a canvas (CanvasSize) bigger than the frame, moved by CanvasPosition with the wheel, its scroll bars or a drag. |
| [Slider](classes/Slider.md) | A track with a thumb: drag it, or click the track, to pick a number from MinValue to MaxValue (in Steps). |
| [Toggle](classes/Toggle.md) | An on/off switch or checkbox: a click flips its Value. |
| [Dropdown](classes/Dropdown.md) | A box showing its chosen Value. |
| [ProgressBar](classes/ProgressBar.md) | A bar filled as far as Value is from MinValue to MaxValue, for health, loading and the like. |
| [BillboardGui](classes/BillboardGui.md) | UI that floats in the world, parented to a Part or MeshPart and drawn on the screen UI overlay. |
| [Sound](classes/Sound.md) | A sound effect or music track that scripts can play. |
| [SoundGroup](classes/SoundGroup.md) | An audio bus: the Sounds whose SoundGroup is it, and the SoundGroups inside it, are mixed together at its Volume, through the sound effects inside it. |
| [ReverbSoundEffect](classes/ReverbSoundEffect.md) | Inside a Sound or SoundGroup: makes it sound like a room, with DecayTime, Density, DryLevel and WetLevel. |
| [EqualizerSoundEffect](classes/EqualizerSoundEffect.md) | Inside a Sound or SoundGroup: raises or lowers its lows, mids and highs (LowGain, MidGain, HighGain, around MidRange). |
| [CompressorSoundEffect](classes/CompressorSoundEffect.md) | Inside a Sound or SoundGroup: turns down what's louder than Threshold by Ratio, so loud and quiet sit closer together. |
| [EchoSoundEffect](classes/EchoSoundEffect.md) | Inside a Sound or SoundGroup: repeats it every Delay seconds, each repeat Feedback decibels quieter. |
| [DistortionSoundEffect](classes/DistortionSoundEffect.md) | Inside a Sound or SoundGroup: overdrives it, by Level. |
| [PitchShiftSoundEffect](classes/PitchShiftSoundEffect.md) | Inside a Sound or SoundGroup: raises or lowers its pitch by Octave without changing its speed. |
| [LowPassSoundEffect](classes/LowPassSoundEffect.md) | Inside a Sound or SoundGroup: lets through only what's below Cutoff, so it sounds muffled, as through a wall or under a pause menu. |
| [HighPassSoundEffect](classes/HighPassSoundEffect.md) | Inside a Sound or SoundGroup: lets through only what's above Cutoff, so it sounds thin, as through a radio. |
| [PointLight](classes/PointLight.md) | A light that shines in all directions from just above its parent part. |
| [SpotLight](classes/SpotLight.md) | A light that shines in a cone from just above its parent part. |
| [RectLight](classes/RectLight.md) | A light that shines from a rectangle Width by Height studs, out of its front only, like a window or a softbox: on the side of its parent part its Direction points out of, or anywhere else from its own Position, turned by its Orientation (the editor shows it as an icon you can drag). |
| [ReflectionProbe](classes/ReflectionProbe.md) | Inside a Part: shiny surfaces in the part's box reflect the world as the editor baked it from the part's middle (Lighting > Bake Probes), lined up with the box's walls, in place of the sky. |
| [LightProbeVolume](classes/LightProbeVolume.md) | Inside a Part: a grid of light probes filling the part's box, baked in the editor (Lighting > Bake Probes) with the light from the sun, lamps and sky and what it bounces off. |
| [Atmosphere](classes/Atmosphere.md) | Inside Lighting: the sky is drawn as sunlight scattered by air in place of the sky colors: blue by day, red and orange toward sunset, dark and starry at night as ClockTime goes round. |
| [Clouds](classes/Clouds.md) | Inside Lighting: a layer of cloud high in the sky (800 to 1200 studs up), lit by the sun and sky, whose shadows fall on the world beneath. |
| [Weather](classes/Weather.md) | Inside Lighting: rain or snow falling around the camera, and how wet and snow-covered surfaces are (parts, models and terrain). |
| [OutlineEffect](classes/OutlineEffect.md) | Inside Lighting: lines drawn along the edges of everything solid, around each silhouette and along creases where a surface turns sharply, found on screen from what's drawn, so no part needs any setup. |
| [ColorCorrectionEffect](classes/ColorCorrectionEffect.md) | Inside Lighting: changes the colors of the finished picture, after tone mapping. |
| [DepthOfFieldEffect](classes/DepthOfFieldEffect.md) | Inside Lighting: blurs what's nearer or farther than the distance in focus, as a camera's lens does: sharp from FocusDistance - InFocusRadius to FocusDistance + InFocusRadius studs away, blurring toward the camera by NearIntensity and away from it by FarIntensity. |
| [VignetteEffect](classes/VignetteEffect.md) | Inside Lighting: darkens the picture toward its corners, or tints them with its Color. |
| [FilmGrainEffect](classes/FilmGrainEffect.md) | Inside Lighting: moving noise over the picture, as on film, changing every frame. |
| [ChromaticAberrationEffect](classes/ChromaticAberrationEffect.md) | Inside Lighting: red and blue drift apart toward the picture's edges, as through a cheap lens. |
| [SharpenEffect](classes/SharpenEffect.md) | Inside Lighting: makes the edges between colors crisper (useful with a render scale below 100%). |
| [SunRaysEffect](classes/SunRaysEffect.md) | Inside Lighting: shafts of light streaming from the sun past whatever is in front of it, such as bright sky between leaves or a window, worked out on screen from what's drawn (so they show only while the sun is in or near the view). |
| [MotionBlurEffect](classes/MotionBlurEffect.md) | Inside Lighting: streaks what moves across the view along its motion, as a camera's shutter does: parts that move, posed characters, and everything when the camera moves. |
| [GraphicsVolume](classes/GraphicsVolume.md) | A Part that changes the look while the camera is inside it, fading in over BlendDistance studs outside it and out again on leaving. |
| [ExposureEffect](classes/ExposureEffect.md) | Inside a GraphicsVolume: the exposure while the camera is in the volume, in place of Lighting's Exposure. |
| [FogEffect](classes/FogEffect.md) | Inside a GraphicsVolume: distance fog from Start to End studs in Color while the camera is in the volume, in place of the world's fog. |
| [BloomEffect](classes/BloomEffect.md) | Inside a GraphicsVolume: how bright light blooms while the camera is in the volume, in place of Lighting's BloomIntensity and BloomThreshold. |
| [ParticleEmitter](classes/ParticleEmitter.md) | Gives off particles (sparks, smoke, fire, stars...) from inside its parent part, shown in the editor and the game. |
| [Animator](classes/Animator.md) | Inside a Part, MeshPart, Model, ScreenGui or UI element: plays animation on its parent while the game runs: its state machine, if it has one, and the AnimationTracks loaded with LoadAnimation. |
| [Animation](classes/Animation.md) | Points at a clip (`*.anim.json`). |
| [AnimationTrack](classes/AnimationTrack.md) | One Animation loaded on an Animator, from `animator:LoadAnimation(animation)`: play, stop and blend it over the state machine. |
| [Sequence](classes/Sequence.md) | Plays a cutscene: a `*.sequence.json` file made in the editor's Sequencer tab, with camera cuts, keyed properties, clips on Animators, Sounds, events, fades, letterbox bars, subtitles from the string table and levels added for a while. |
| [Spline](classes/Spline.md) | A curve through its SplinePoint children, in order (and back to the first when Closed), for SplineFollowers to move parts along, NavigationAgents to patrol and the Sequencer's camera to ride. |
| [SplinePoint](classes/SplinePoint.md) | One point of the Spline it's in: where the curve passes (Position), and how it bends there (TangentMode, InTangent and OutTangent). |
| [SplineFollower](classes/SplineFollower.md) | Moves the Part (with the parts welded to it) or the Model it's parented to along its Spline at Speed during Play: moving platforms, trains and gondolas. |
| [SplineMesh](classes/SplineMesh.md) | Inside a Spline: bends the model MeshId names along the curve, repeated to fill it with no gaps, for roads, pipes, fences and rails. |
| [SplineScatter](classes/SplineScatter.md) | Inside a Spline: places the models Models names along the curve every Spacing studs, or inside a Closed spline at Density, each turned, scaled and moved at random from Seed and stood on the surface below (none where it's steeper than MaxSlope). |
| [Decal](classes/Decal.md) | Inside a part: a picture (Texture) on one of its faces (Face), tinted by Color3 and faded by Transparency. |
| [ProjectedDecal](classes/ProjectedDecal.md) | A box that lays its picture (Texture) onto whatever solid surfaces are inside it, following their shape: bullet holes, puddles, road markings and dirt. |
| [Terrain](classes/Terrain.md) | A landscape: a heightfield shaped in the editor's Terrain Studio, drawn with its TerrainLayers and colliding as its ground. |
| [TerrainLayer](classes/TerrainLayer.md) | Inside a Terrain: one material it's painted with (a tint, texture maps and tiling), and an optional rule that lays it down by height and slope. |
| [ThirdPersonController](classes/ThirdPersonController.md) | Walks, runs, jumps and crouches the character model it is parented to, kinematically, up slopes and steps within its limits, and owns the camera during Play: behind the character, zoomed with the mouse wheel. |
| [FirstPersonController](classes/FirstPersonController.md) | Moves the character model it is parented to just like a ThirdPersonController, with the camera at the character's eyes during Play, looking where the player looks. |
| [NavigationAgent](classes/NavigationAgent.md) | Walks the Model or Part it is parented to during Play: along the navigation mesh to its Destination, round walls and gaps, or straight at its Velocity. |
| [Checkpoint](classes/Checkpoint.md) | A place in the world to come back to. |
| [Camera](classes/Camera.md) | A view of the world. |
| [Attachment](classes/Attachment.md) | A point and a direction on the part it's parented to, for things that need a spot on a part. |
| [VehicleWheel](classes/VehicleWheel.md) | A wheel of the car whose part it's parented to: a spring and a ray to the ground, so no wheel collider touches it. |
| [Ragdoll](classes/Ragdoll.md) | A ragdoll for the skinned MeshPart it's in: a capsule Part per main bone, named after the bone, and a BallSocketConstraint per joint, which Build (in its Properties) makes from the skeleton with a humanoid preset's limits. |
| [IKControl](classes/IKControl.md) | Inverse kinematics during the game, for the skinned MeshPart it's in (or its Animator). |
| [FaceControls](classes/FaceControls.md) | The face of the MeshPart it's in: one number property per standard facial control, 0 to 1 (Blink_Left, Brow_Raise_Left, Mouth_Smile_Left, Jaw_Open …). |
| [Fracture](classes/Fracture.md) | Breaks the Part or MeshPart it's in into pieces. |
| [Health](classes/Health.md) | How much a Model can take: Health out of MaxHealth, put in any Model. |
| [ProximityPrompt](classes/ProximityPrompt.md) | A prompt to do something, under a part or an Attachment. |
| [ClickDetector](classes/ClickDetector.md) | Makes the part it's in clickable. |
| [Tool](classes/Tool.md) | Something a character carries: a Model of parts with a Handle. |
| [Inventory](classes/Inventory.md) | The Tools a character carries, put in the character's Model: up to Capacity Tools as its children, in hotbar order, and Equipped, the one in hand. |
| [Humanoid](classes/Humanoid.md) | A Roblox-familiar front for a character, in its Model. |
| [Ability](classes/Ability.md) | Something a character can do, in its Model or its Player: a Fireball, a dash, a heal. |
| [Effect](classes/Effect.md) | A change to the Model it's in: to an attribute, or a Health or Humanoid property. |
| [ForceField](classes/ForceField.md) | Keeps the Health of the Model it's in from losing any: TakeDamage, DamageZones and falls do nothing. |
| [Teams](classes/Teams.md) | The service that holds the Teams players can be on. |
| [Team](classes/Team.md) | A side players can be on, in Teams. |
| [SoundService](classes/SoundService.md) | The service that holds the SoundGroups, the game's audio buses. |
| [TextChatService](classes/TextChatService.md) | The default chat (Phase 67): a chat window at the bottom left that `/` opens, styled with `TextChatService` rules (`TextChatService::key` styles the names). |
| [WeldConstraint](classes/WeldConstraint.md) | Holds `Part0` and `Part1` together where they were when it was made (or enabled). |
| [HingeConstraint](classes/HingeConstraint.md) | Joins two parts at their attachments so Attachment1's part turns around Attachment0's X axis (its `Axis`), like a door or a wheel. |
| [BallSocketConstraint](classes/BallSocketConstraint.md) | Holds two attachments together and lets them turn any way, like a shoulder or a pendulum. |
| [PrismaticConstraint](classes/PrismaticConstraint.md) | Lets Attachment1's part slide along Attachment0's X axis only, like a drawer or a piston. |
| [CylindricalConstraint](classes/CylindricalConstraint.md) | Lets Attachment1's part slide along and turn around Attachment0's X axis, like a drill or a screw. |
| [SpringConstraint](classes/SpringConstraint.md) | Pulls or pushes its attachments toward FreeLength apart with Stiffness, slowed by Damping, never shorter than MinLength or longer than MaxLength. |
| [RopeConstraint](classes/RopeConstraint.md) | Keeps its attachments at most Length apart; closer, it's slack. |
| [RodConstraint](classes/RodConstraint.md) | Keeps its attachments exactly Length apart, letting them turn freely. |
| [LinearVelocity](classes/LinearVelocity.md) | Keeps Attachment0's part (and everything welded to it) moving at VectorVelocity, pushing with at most MaxForce through its center of mass. |
| [AngularVelocity](classes/AngularVelocity.md) | Keeps Attachment0's part spinning at AngularVelocity (radians a second), turning it with at most MaxTorque. |
| [VectorForce](classes/VectorForce.md) | Pushes Attachment0's part with a constant Force every physics step, at Attachment0 or (ApplyAtCenterOfMass) its center of mass. |
| [Torque](classes/Torque.md) | Turns Attachment0's part with a constant Torque every physics step. |
| [AlignPosition](classes/AlignPosition.md) | Moves Attachment0's part so Attachment0 reaches Attachment1 (TwoAttachment Mode) or Position (OneAttachment), smoothly, closing most of the gap in about 1/Responsiveness seconds, with at most MaxForce and MaxVelocity. |
| [AlignOrientation](classes/AlignOrientation.md) | Turns Attachment0's part so Attachment0 faces the way Attachment1 does (TwoAttachment Mode) or CFrame's rotation (OneAttachment), smoothly, in about 1/Responsiveness seconds, with at most MaxTorque and MaxAngularVelocity. |
| [IntValue](classes/IntValue.md) | Holds one whole number in Value (other numbers are rounded). |
| [NumberValue](classes/NumberValue.md) | Holds one number in Value. |
| [StringValue](classes/StringValue.md) | Holds one string in Value. |
| [BoolValue](classes/BoolValue.md) | Holds one boolean in Value. |
| [Vector3Value](classes/Vector3Value.md) | Holds one Vector3 in Value. |
| [Color3Value](classes/Color3Value.md) | Holds one Color3 in Value. |
| [CFrameValue](classes/CFrameValue.md) | Holds one CFrame in Value. |
| [ObjectValue](classes/ObjectValue.md) | Points at another Instance in Value, or nil. |
| [DataLayerService](classes/DataLayerService.md) | The service that holds the project's DataLayers (roadmap R16). |
| [DataLayer](classes/DataLayer.md) | A data layer (roadmap R16): the Instances tagged with its Name (CollectionService), wherever they are in the tree. |
| [ScatterRule](classes/ScatterRule.md) | Inside a Terrain: places plants, props or models (Models) over the ground by itself, where its height, slope, layer and water settings allow, about Spacing studs apart at Density, gathered into clumps by Clumping and keeping AvoidRadius from what the rules before it placed. |
| [Biome](classes/Biome.md) | Inside a Terrain: an area of the ground, such as a forest or an alpine meadow (procedural worlds PW2). |
| [TerrainPath](classes/TerrainPath.md) | Inside a Spline: a road or path on the terrain (procedural worlds PW4), Width studs across along the Spline's curve. |
| [BuildingGenerator](classes/BuildingGenerator.md) | A building made from a footprint (procedural worlds PW6). |
