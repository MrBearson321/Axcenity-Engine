---
description: "The face of the MeshPart it's in: one number property per standard facial control, 0 to 1 (Blink_Left, Brow_Raise_Left, Mouth_Smile_Left, Jaw_Open …)."
---

# FaceControls

```luau
Instance.new("FaceControls")
```

The face of the MeshPart it's in: one number property per standard facial control, 0 to 1 (Blink_Left, Brow_Raise_Left, Mouth_Smile_Left, Jaw_Open …). After the Animator and before drawing, its face profile turns them into the model's blend shape weights and turns its eye and jaw bones. Scripts and TweenService animate a face by setting these properties.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
FaceControls.Enabled: boolean
```

If false, the face rests: the model's own blend shape values, and the eyes and jaw as the body's pose has them.

#### Profile

```luau
FaceControls.Profile: string
```

The face profile mapping the controls to the model's blend shapes and bones: a built-in one's name (`"axcenity"`, `"arkit"` or `"vroid"`), a `*.face.json` file's path, or empty for the one that names the most of the model's blend shapes.

#### LayerOrder

```luau
FaceControls.LayerOrder: string
```

The face layers' order, lowest first, as names separated by commas (`"Base, Expression, Dialogue, Procedural, Override"`): Base is these properties, Expression is `SetExpression`'s, Dialogue is lip sync, Procedural is blinking, gaze and idle motion, and Override is `SetOverride`'s. Layers left out keep their place after the named ones.

#### BlinkEnabled

```luau
FaceControls.BlinkEnabled: boolean
```

Whether the eyes blink by themselves, now and then.

#### BlinkEyes

```luau
FaceControls.BlinkEyes: string
```

Which eyes blink by themselves: `"Both"`, `"Left"` or `"Right"`.

#### BlinkIntervalMin

```luau
FaceControls.BlinkIntervalMin: number
```

The fewest seconds between blinks; each wait is a random time from this to BlinkIntervalMax.

#### BlinkIntervalMax

```luau
FaceControls.BlinkIntervalMax: number
```

The most seconds between blinks.

#### BlinkDuration

```luau
FaceControls.BlinkDuration: number
```

Seconds a blink takes, closing and opening.

#### DoubleBlinkChance

```luau
FaceControls.DoubleBlinkChance: number
```

How often a blink is followed straight away by another, from 0 (never) to 1 (always).

#### GazeMode

```luau
FaceControls.GazeMode: string
```

What the eyes look at: `"Animation"` (only what the controls say), `"Camera"`, `"Target"` (GazeTarget) or `"Position"` (GazePosition). The eyes turn within the face profile's limits and look ahead again when the target is too far round.

#### GazeTarget

```luau
FaceControls.GazeTarget: Instance?
```

With GazeMode `"Target"`: the part, Attachment or character (a Model, looked at in its Head) to look at.

#### GazePosition

```luau
FaceControls.GazePosition: Vector3
```

With GazeMode `"Position"`: the point in the world to look at.

#### GazeSmoothing

```luau
FaceControls.GazeSmoothing: number
```

About how many seconds the eyes take to catch up with a moving target; 0 follows at once.

#### GazeWeight

```luau
FaceControls.GazeWeight: number
```

How much the gaze counts over the eye controls, from 0 to 1.

#### IdleMotion

```luau
FaceControls.IdleMotion: number
```

Small movements a living face makes by itself (the eyes darting, the brows drifting), from 0 (none) to 1.

#### VisemeMap

```luau
FaceControls.VisemeMap: string
```

Which viseme map lip sync uses: what each of the 12 visemes (Silence, AI, E, O, U, FV, L, MBP, WQ, TH, TDN, KG) does to the controls, and how much of the opening the jaw carries. A `*.visemes.json` file's name or path in assets/faces/visemes, or empty for the built-in one.

#### LipSyncSmoothing

```luau
FaceControls.LipSyncSmoothing: number
```

Seconds one viseme takes to blend into the next during lip sync.

#### LipSyncAnticipation

```luau
FaceControls.LipSyncAnticipation: number
```

Seconds before its time each viseme starts, since mouths move a little before the sound comes.

#### ProceduralEnabled

```luau
FaceControls.ProceduralEnabled: boolean
```

If false, the face doesn't blink, look about or move by itself: it shows only its controls, expressions, lip sync and overrides. For background characters, or a face a cutscene keys whole.

#### LodDistance

```luau
FaceControls.LodDistance: number
```

Studs from the camera where the face starts costing less: beyond it the face updates 15 times a second without idle motion, beyond twice it only its expressions 4 times a second, and beyond four times it (or out of view) it rests. 0 keeps it whole at any distance.

#### ReplicateExpressions

```luau
FaceControls.ReplicateExpressions: boolean
```

In multiplayer, whether expressions set on the server (`SetExpression`, `ClearExpression`) show on players' devices. Off, each device's own LocalScripts set them.

#### ReplicateOverrides

```luau
FaceControls.ReplicateOverrides: boolean
```

In multiplayer, whether overrides and layer settings set on the server (`SetOverride`, `ClearOverrides`, `SetLayer`) go to players' devices.

#### ReplicateLipSync

```luau
FaceControls.ReplicateLipSync: boolean
```

In multiplayer, whether a lip sync played on the server (`PlayLipSync`, `StopLipSync`) plays on players' devices, kept in time with the server's.

#### ReplicationRate

```luau
FaceControls.ReplicationRate: number
```

In multiplayer, the most times a second the face's changes go to players' devices (they blend between them themselves). 0 sends every change at once.

#### Blink_Left

```luau
FaceControls.Blink_Left: number
```

Closes the left eye in a blink, 0 to 1.

#### Blink_Right

```luau
FaceControls.Blink_Right: number
```

Closes the right eye in a blink, 0 to 1.

#### Eye_Closed_Left

```luau
FaceControls.Eye_Closed_Left: number
```

Holds the left eye closed, 0 to 1, as in sleep or a wink, apart from blinking.

#### Eye_Closed_Right

```luau
FaceControls.Eye_Closed_Right: number
```

Holds the right eye closed, 0 to 1, as in sleep or a wink, apart from blinking.

#### Eye_Squint_Left

```luau
FaceControls.Eye_Squint_Left: number
```

Narrows the left eye, 0 to 1.

#### Eye_Squint_Right

```luau
FaceControls.Eye_Squint_Right: number
```

Narrows the right eye, 0 to 1.

#### Eye_Wide_Left

```luau
FaceControls.Eye_Wide_Left: number
```

Opens the left eye wide, 0 to 1.

#### Eye_Wide_Right

```luau
FaceControls.Eye_Wide_Right: number
```

Opens the right eye wide, 0 to 1.

#### Eye_Look_Up

```luau
FaceControls.Eye_Look_Up: number
```

Turns both eyes up, 0 to 1 (1 is the face profile's up limit, 25° by default). Turns the eye bones when the model has them, otherwise its look shapes.

#### Eye_Look_Down

```luau
FaceControls.Eye_Look_Down: number
```

Turns both eyes down, 0 to 1 (1 is the face profile's down limit, 20° by default).

#### Eye_Look_Left

```luau
FaceControls.Eye_Look_Left: number
```

Turns both eyes to the character's left, 0 to 1 (1 is the face profile's sideways limit, 30° by default).

#### Eye_Look_Right

```luau
FaceControls.Eye_Look_Right: number
```

Turns both eyes to the character's right, 0 to 1 (1 is the face profile's sideways limit, 30° by default).

#### Brow_Raise_Left

```luau
FaceControls.Brow_Raise_Left: number
```

Raises the left eyebrow, 0 to 1.

#### Brow_Raise_Right

```luau
FaceControls.Brow_Raise_Right: number
```

Raises the right eyebrow, 0 to 1.

#### Brow_Lower_Left

```luau
FaceControls.Brow_Lower_Left: number
```

Lowers the left eyebrow, 0 to 1.

#### Brow_Lower_Right

```luau
FaceControls.Brow_Lower_Right: number
```

Lowers the right eyebrow, 0 to 1.

#### Brow_Inward_Left

```luau
FaceControls.Brow_Inward_Left: number
```

Draws the left eyebrow's inner end in and up, 0 to 1, as in worry.

#### Brow_Inward_Right

```luau
FaceControls.Brow_Inward_Right: number
```

Draws the right eyebrow's inner end in and up, 0 to 1, as in worry.

#### Mouth_Smile_Left

```luau
FaceControls.Mouth_Smile_Left: number
```

Pulls the left corner of the mouth up into a smile, 0 to 1.

#### Mouth_Smile_Right

```luau
FaceControls.Mouth_Smile_Right: number
```

Pulls the right corner of the mouth up into a smile, 0 to 1.

#### Mouth_Frown_Left

```luau
FaceControls.Mouth_Frown_Left: number
```

Pulls the left corner of the mouth down, 0 to 1.

#### Mouth_Frown_Right

```luau
FaceControls.Mouth_Frown_Right: number
```

Pulls the right corner of the mouth down, 0 to 1.

#### Mouth_Open

```luau
FaceControls.Mouth_Open: number
```

Parts the lips, 0 to 1, without the jaw (Jaw_Open drops the jaw).

#### Mouth_Wide

```luau
FaceControls.Mouth_Wide: number
```

Stretches the mouth wide, 0 to 1.

#### Mouth_Narrow

```luau
FaceControls.Mouth_Narrow: number
```

Narrows the mouth into a round opening, 0 to 1, as for "oo".

#### Mouth_Pucker

```luau
FaceControls.Mouth_Pucker: number
```

Pushes the lips forward together, 0 to 1, as for a kiss.

#### Mouth_Left

```luau
FaceControls.Mouth_Left: number
```

Moves the whole mouth to the character's left, 0 to 1.

#### Mouth_Right

```luau
FaceControls.Mouth_Right: number
```

Moves the whole mouth to the character's right, 0 to 1.

#### Upper_Lip_Raise

```luau
FaceControls.Upper_Lip_Raise: number
```

Raises the upper lip, 0 to 1.

#### Lower_Lip_Lower

```luau
FaceControls.Lower_Lip_Lower: number
```

Lowers the lower lip, 0 to 1.

#### Upper_Lip_Raise_Left

```luau
FaceControls.Upper_Lip_Raise_Left: number
```

Raises the left side of the upper lip, 0 to 1.

#### Upper_Lip_Raise_Right

```luau
FaceControls.Upper_Lip_Raise_Right: number
```

Raises the right side of the upper lip, 0 to 1.

#### Lower_Lip_Lower_Left

```luau
FaceControls.Lower_Lip_Lower_Left: number
```

Lowers the left side of the lower lip, 0 to 1.

#### Lower_Lip_Lower_Right

```luau
FaceControls.Lower_Lip_Lower_Right: number
```

Lowers the right side of the lower lip, 0 to 1.

#### Lip_Corner_Pull_Left

```luau
FaceControls.Lip_Corner_Pull_Left: number
```

Pulls the left corner of the lips back, 0 to 1.

#### Lip_Corner_Pull_Right

```luau
FaceControls.Lip_Corner_Pull_Right: number
```

Pulls the right corner of the lips back, 0 to 1.

#### Cheek_Raise_Left

```luau
FaceControls.Cheek_Raise_Left: number
```

Raises the left cheek, 0 to 1, as in a broad smile.

#### Cheek_Raise_Right

```luau
FaceControls.Cheek_Raise_Right: number
```

Raises the right cheek, 0 to 1, as in a broad smile.

#### Cheek_Puff

```luau
FaceControls.Cheek_Puff: number
```

Puffs both cheeks out, 0 to 1.

#### Nose_Sneer_Left

```luau
FaceControls.Nose_Sneer_Left: number
```

Wrinkles the left side of the nose up, 0 to 1.

#### Nose_Sneer_Right

```luau
FaceControls.Nose_Sneer_Right: number
```

Wrinkles the right side of the nose up, 0 to 1.

#### Jaw_Open

```luau
FaceControls.Jaw_Open: number
```

Drops the jaw, 0 to 1 (1 is the face profile's jaw limit, 25° by default). Turns the jaw bone when the model has one, and drives a Jaw_Open shape too when it has one.

#### Jaw_Forward

```luau
FaceControls.Jaw_Forward: number
```

Pushes the jaw forward, 0 to 1.

#### Jaw_Left

```luau
FaceControls.Jaw_Left: number
```

Moves the jaw to the character's left, 0 to 1. Turns the jaw bone when the model has one.

#### Jaw_Right

```luau
FaceControls.Jaw_Right: number
```

Moves the jaw to the character's right, 0 to 1. Turns the jaw bone when the model has one.

#### Tongue_Out

```luau
FaceControls.Tongue_Out: number
```

Sticks the tongue out, 0 to 1.

#### Tongue_Up

```luau
FaceControls.Tongue_Up: number
```

Curls the tongue's tip up, 0 to 1.

#### Tongue_Down

```luau
FaceControls.Tongue_Down: number
```

Curls the tongue's tip down, 0 to 1.

### Functions

#### SetExpression

```luau
FaceControls:SetExpression(name: string, intensity: number?, duration: number?, options: { Weight: number?, Priority: number?, Asymmetry: number? }?): ()
```

Shows the expression `name` (a built-in one such as `"Happy"`, `"Sad"` or `"Sleepy"`, or a `*.expression.json` file's name or path) at `intensity` (0 to 1, 1 if left out), blending in over `duration` seconds (the expression's own time if left out). `options` may set `Weight` (0 to 1, how much it counts over the expressions under it), `Priority` (higher goes over lower; equal ones go in the order they were set) and `Asymmetry` (-1 shows only the left side, 1 only the right). Setting a shown expression again changes it.

#### ClearExpression

```luau
FaceControls:ClearExpression(name: string?, duration: number?): ()
```

Fades the expression `name` out over `duration` seconds (its own time if left out), or every expression if `name` is nil.

#### GetExpressions

```luau
FaceControls:GetExpressions(): { { Name: string, Intensity: number, Weight: number, Priority: number, Asymmetry: number } }
```

The expressions being shown, in the order they were set: each a table of `Name`, `Intensity`, `Weight`, `Priority` and `Asymmetry`.

#### SetOverride

```luau
FaceControls:SetOverride(control: string, value: number?): ()
```

Holds the control `control` (such as `"Jaw_Open"`) at `value` (0 to 1) over everything else on the Override layer, such as a character's mouth while eating; nil lets it go.

#### ClearOverrides

```luau
FaceControls:ClearOverrides(): ()
```

Lets go of every control SetOverride holds.

#### SetLayer

```luau
FaceControls:SetLayer(layer: string, weight: number?, blend: string?, mask: (string | { string })?): ()
```

Sets how the face layer `layer` (`"Base"`, `"Expression"`, `"Dialogue"`, `"Procedural"` or `"Override"`) goes over those under it: `weight` from 0 to 1 (1 if left out), `blend` `"Override"` (replaces, the default) or `"Additive"` (adds), and `mask`, the controls it may move: a group (`"Eyes"`, `"Eyebrows"`, `"Mouth"`, `"CheeksAndNose"`, `"JawAndTongue"`), a control, or a list of them; nil for all.

#### PlayLipSync

```luau
FaceControls:PlayLipSync(path: string, sound: Sound?, options: { Speed: number?, Weight: number?, FadeTime: number?, TimePosition: number? }?): ()
```

Plays the lip sync `path` (a `*.lipsync.json` file of timed visemes) on the face's Dialogue layer, through its VisemeMap. Given a Sound, it keeps time with it: it moves on only while the Sound plays (at its PlaybackSpeed) and fades out when the Sound has played and stopped, so call it just before `sound:Play()`. `options` may set `Speed` (1), `Weight` (0 to 1, how much it counts over the layers under it), `FadeTime` (seconds to fade in and out, 0.1) and `TimePosition` (where to start). A new line replaces the one playing.

#### StopLipSync

```luau
FaceControls:StopLipSync(fadeTime: number?): ()
```

Fades the lip sync out over `fadeTime` seconds (its own fade time if left out); the mouth goes back to the layers under it.

#### GetLipSync

```luau
FaceControls:GetLipSync(): { Path: string, TimePosition: number, Speed: number, Weight: number, Sound: Sound? }?
```

The lip sync playing, as a table of `Path`, `TimePosition`, `Speed`, `Weight` and `Sound` (if it keeps time with one), or nil.

