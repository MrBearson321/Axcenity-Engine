---
description: Animators, clips and state machines, the Animation tab, keying parts and UI, posing rigs, importing animated models and cutscenes.
icon: person
samples: [hello-animation, hello-character, hello-cutscene]
---

# Animation

Axcenity animates two kinds of things with the same tools: **skinned models** (characters with a skeleton, imported from FBX) and **any object's properties** (a door's Position, a light's Brightness, a button's colour). Both play from **clips**, which are `.anim.json` files under `assets/animations/`.

## The pieces

| Piece | What it is |
|---|---|
| Clip | A file of keyframes over time. A **linked** clip plays a take from an imported model; a **keyed** clip holds keys you set yourself. |
| Animation | An Instance that plays one clip on its parent. Properties: **Clip**, **AutoPlay**, **Enabled**, **Speed**, **Looped**. |
| Animator | An Instance that runs a **state machine** on its parent: which clip plays when, and how they blend. |
| State machine | A `.statemachine.json` file of states (each plays a clip or a blend tree) joined by transitions with conditions. |

## Animate anything in one click

Select a part, model or UI element and press **Animate** (in the Explorer's right-click menu, at the top of Properties, or on the empty Animation tab). In one undo step it adds an Animator, makes a keyed clip named after the object and puts an Animation in it that plays the clip. The **Animation** tab opens on the new clip.

## The Animation tab

The Animation tab (beside the Scene) follows the selection: select something that animates and it shows that object's state machine or clips.

- The **outline** on the left lists the object's Animator, its state machine's states and its Animations. The **breadcrumb** on top says where you are, such as **Hero ▸ State Machine ▸ Walk**.
- The **preview** shows a copy of the object, so changes here don't move the real one until you save. Drag to turn the view and scroll to zoom.
- Above the timeline, the **transport** plays the clip like a media player: go to the start, back a frame, play (pause, then resume), on a frame, go to the end and stop; then where the playhead is, a seek bar to click or drag, the clip's length and the frame number. The rate box (0.25× to 4×) changes only how fast the preview plays, not the clip's own Speed.
- With the mouse over the clip and nothing being typed, **Space** plays or pauses, **Left** and **Right** step a frame (**Shift** for ten), and **Home** and **End** go to the start and end. They're listed in **Settings > Shortcuts** under Animation clip.
- The **timeline** shows the playhead and the clip's length. Under it, the **dope sheet** shows a row per animated property or bone, with a diamond for each key.
- Move, rotate or resize the copy in the preview with the buttons above it. With **Record** on, each change sets keys at the playhead. With Record off, **Key selected** (**K**) keys the object where it is, and **Add track** keys any other property.
- **Dope sheet · Curves** switches to curves: the values over time, X red, Y green and Z blue. Drag points and handles to shape the motion.
- In the dope sheet, drag a box to select keys, drag them to move them, and copy (**Ctrl+C**), cut (**Ctrl+X**) and paste at the playhead (**Ctrl+V**). **Snap** keeps keys on whole frames.

## Posing characters

On a skinned model the preview shows the skeleton's joints. Click a joint (or a bone in the list) and turn it with the gizmo. **Move** drags a joint; for a joint that isn't the root, the bones above it turn so it gets there (inverse kinematics), and **Chain** sets how many bones may bend. Key the pose with Record on.

## State machines

Open the Animator's **State Machine** in the outline to see its graph. Each state plays a clip or a **blend tree** that mixes clips by a number (walk to run by speed, for example). Transitions move between states when their conditions hold. Scripts drive the conditions:

```lua
local animator = workspace.Hero.Animator
animator:SetNumber("Speed", 12)
animator:SetBool("Grounded", true)
animator:SetTrigger("Jump")
```

`Animator.StateChanged` fires when the state changes, and `Animator.AnimationEvent` fires for events placed on a clip's timeline. Layers with weights (`Animator:SetLayerWeight()`) play upper-body clips over the legs.

## Playing clips from scripts

An Animation with **AutoPlay** starts when the game starts. Scripts call `Animation:Play()`, `Animation:Pause()`, `Animation:Resume()` and `Animation:Stop()`, and `Animation.Completed` fires when a clip that doesn't loop ends. For a one-off clip on an Animator, `Animator:LoadAnimation()` returns an **AnimationTrack** to play, stop and fade.

## Importing animated models

Import an `.fbx` with a skeleton or takes (**File > Import…**) and the Import dialog shows an **Animation** tab: the bones, each take, and a table of clips to make, with start and end frames and looping. Import writes the clips, a state machine and an Animator in one step. See [Assets: import and manage](assets.md).

## Cutscenes

A **Sequence** Instance plays a cutscene: camera cuts, clips, property tracks and sounds on one timeline, saved as `assets/sequences/<name>.sequence.json`. Edit it in the **Sequencer** tab, which shares the Animation tab's dope sheet and curves. Tick **AutoPlay**, or call `Sequence:Play()` from a script; **Skippable** lets the player skip it and `Sequence.Completed` fires at the end. The **hello-cutscene** sample has one.

> **Known limits.** Animation in glTF files isn't imported yet: use FBX for animated models.
