---
description: Block scripts for people new to code, the Blocks and Graph views, switching between blocks and Lua, and what each block category does.
icon: puzzle
samples: [hello-touched-blocks]
---

# Blocks: visual programming

Blocks let you script without typing: snap blocks together like puzzle pieces. They aren't a separate system: **every block becomes Lua**, the game runs that Lua, and you can switch a script between Code, Blocks and Graph whenever you like.

## Three views of one script

The bar above a Script or LocalScript tab picks the view:

- **Code**: the Lua, typed.
- **Blocks**: stacks of blocks under an event block. A palette by category is on the left; each category has its own colour and icon, and event blocks start with the same icon.
- **Graph**: the same blocks as nodes and wires, Blueprint-style. White **exec** wires say what runs next; coloured wires carry values (green numbers, pink text, red true or false, gold vectors, blue colours).

Switching from Code to Blocks turns each line into the friendliest block that writes the same code, so nothing is lost. Switching back writes Lua again.

## Building a script

1. Open a block script, such as `pad` in the **hello-touched-blocks** sample.
2. Drag an **event** block (a "hat", such as **when part is touched**) onto the canvas. Only stacks under an event run; a loose stack says so.
3. Drag action blocks under it. They snap in and run from the top down. C-shaped blocks (**if**, **repeat**, **forever**) hold a stack inside them.
4. Drop round **value** blocks (numbers, text, maths) and pointed **true/false** blocks into the slots.
5. Press **View as Lua** to see the code the blocks become.

**Search blocks** above the palette finds a block by what it does ("destroy", "Raycast"), and hovering a palette block shows how to use it and its Lua. **Ctrl+Z** and **Ctrl+Y** undo and redo inside the block script under the mouse.

## The categories

| Category | Examples |
|---|---|
| Events | when the game starts, when a part is touched, when a key is pressed, when a button is clicked, when a prompt is triggered |
| Control | wait, repeat, forever, repeat until, if/else, for each child, destroy after a time |
| Motion | move to, move by, turn to look at, set anchored, push with a force, patrol a spline, set the camera |
| Looks | set colour, set text, show, hide, show a page, set any property, set the time of day |
| Sound | play, stop, pause, resume, set volume |
| Variables | set, change by, and the variable's value |
| More | print, destroy, clone, save and load the game, load a level, host or join a game, send a chat message |
| Operators | maths, pick random, compare, and/or/not, join text, round |
| Sensing | touching part, part under the mouse, key pressed, a property's value, raycast |
| Animation | set a trigger, bool or number of an Animator, when it changes state, play a cutscene |
| Tweens | tween a property, slide in, fade in or out |
| Luau | `local`, `while`, `for`, functions, `:Connect` and any call, for everything else |

Every block matches a real API call: there are no block-only features, so anything you learn in blocks works the same in Lua.

## When something goes wrong

A runtime error in a block script highlights the block it came from in red, and the Output says what happened in plain words.

> **Good to know.** Any Lua script can switch to Blocks: what has no friendly block becomes a Luau block that writes the same code. Long names such as `row.Binding.Text` are typed into a slot rather than built from blocks, and a function deep inside another value is easiest to edit in the Graph view.
