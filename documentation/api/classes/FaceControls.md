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

