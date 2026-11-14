---
description: Every class, data type, enum, library and global the engine's scripts can use, generated from the engine itself.
---

# API reference

Everything Luau scripts can use: 138 classes, 46 data types and 2442 properties, events, functions and enum items, with what each does and 45 usage examples.

These pages are generated from the engine's own API description, the one the editor's **API Reference** page and the VS Code definitions (`definitions/axcenity.d.luau`) come from, so they always match the engine. Don't edit them by hand: run `cargo run -p axcenity-script --bin lua-definitions` after changing the API.

* [Classes](classes.md): parts, models, lights, sounds, UI, scripts and everything else in the tree.
* [Data types](types.md): Vector3, CFrame, Color3, UDim2, Signal and the services that aren't Instances.
* [Enums](enums.md): fixed lists of choices.
* [Libraries](libraries.md): global tables of functions, such as `task`.
* [Globals](globals.md): `game`, `workspace`, `script`, `require` and the rest.

Every class also has the members of [Instance](classes/Instance.md).
