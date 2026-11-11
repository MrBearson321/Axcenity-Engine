---
description: Insert parts, move, resize and rotate them with handles and snapping, and use the building tools.
icon: cube
samples: [hello-part, primitives, hello-csg]
---

# Building and moving objects

Worlds are built from **parts**: blocks, balls, cylinders and wedges you can size, colour and give a material, plus imported models (MeshParts). Everything you build is undoable, one step per action.

## Insert a part

On the **Home** tab, the **Parts** group inserts a **Block**, **Sphere**, **Cylinder**, **Wedge**, **Corner Wedge** (a slope rising to one corner, for roof corners) or **Capsule** (a cylinder with round ends). The new part goes in front of the camera, resting on whatever the middle of the view looks at (a part, the baseplate or terrain); looking at the sky, it goes a short way ahead. If that spot is taken it's lifted until it's clear, so inserting several in a row piles them up instead of overlapping. It goes inside the selected Model or Folder (or the Workspace) and becomes the selection. New parts are named `Part`, unanchored and plastic, and are drawn with a grid texture while you edit so their size is easy to read (choose a material to replace it).

The Explorer's right-click **Insert** menu has every kind of object, grouped: parts, Model, Folder and Attachment, then lighting, effects, audio, gameplay, UI, values, scripts and events.

## Select

Click a part in the Scene or a row in the Explorer. **Ctrl**-click adds or removes one; **Shift**-click in the Explorer selects a range. Selected parts get a blue box. Click empty space to clear the selection. A part with **Locked** on is clicked through, so a baseplate never gets in the way; unlock it from its right-click menu in the Explorer.

## Move, resize and rotate

| Key | Tool | Handles |
|---|---|---|
| **1** | Select | none |
| **2** | Move | an arrow per axis |
| **3** | Resize | a ball on each face; the opposite face stays put |
| **4** | Rotate | a ring per axis; **Shift** while dragging turns slowly |
| **5** | Transform | all three at once, with boxes to type Position, Rotation and Size |

Each tool has an options panel at the top left of the Scene: Move and Rotate can use the world's axes, each part's own, or the last selected part's; Resize can move one face or both. You can also drag a part by its body: it slides over whatever is under the mouse and rests on it, so you can stack parts or push them against a wall. While a part moves, a guide line and the distance in studs show how far it has come.

## Snapping

The **Snapping** group on the Home tab:

- **Move** and **Rotate** steps (1 stud and 15° to start), each with its own checkbox.
- **Surfaces** (on to start): a moving part lands flush against faces near it.
- **Snap Points** (on to start): corners, edge middles and face middles click together.
- **Collisions** (off to start): moving parts stop at solid parts instead of going into them.
- **Join Surfaces** (off to start): a moved part is welded to the parts it's left touching.
- **Collision Groups…**: which groups of parts collide with which.

## Duplicate, group and delete

**Ctrl+D** duplicates the selection in place, **Delete** removes it, and **F2** renames. To group parts, insert a **Model** or **Folder** and drag the parts' rows onto it in the Explorer. See [Explorer and hierarchy](explorer-and-hierarchy.md).

## The building tools

The **Model** tab's **Build** group works on every selected part at once:

| Key | Tool | Does |
|---|---|---|
| **V** | Paint | Colour swatches, your saved palettes and a colour picker |
| **N** | Material | Built-in materials, prototype textures, and your project's own materials |
| **M** | Anchor | Switch `Anchored` on or off for the whole selection |
| **U** | Lighting | Add, remove and tune point, spot and rect lights in the selected parts |
| **P** | Emission | Glow colour and strength, and particle emitters |
| **R** | Angular | Copy a part around a bend, for rails, arches and rings |

## Solid modeling and constraints

**Union** joins parts into one shape, **Negate** turns a part into a hole that cuts what it overlaps when you union them, **Intersect** keeps only the overlap and **Separate** undoes a union. The **Constraints** group welds parts together (**Weld**), adds **Attachments**, and places joints and movers (hinges, springs, ropes, motors) with the **Constraint** tool: click a surface on one part, then on another.

## Properties

The **Properties** panel shows the selected object's settings in groups (Data, Appearance, Behavior, Part and so on). Drag a number sideways to change it or click it to type; changes show at once. With several objects selected it shows what they share, and a value that differs shows as `—`. Hover a property's name for what it does, and use the filter box at the top to find one by name.
