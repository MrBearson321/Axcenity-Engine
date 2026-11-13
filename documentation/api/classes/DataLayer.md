---
description: "A data layer (roadmap R16): the Instances tagged with its Name (CollectionService), wherever they are in the tree."
---

# DataLayer

```luau
Instance.new("DataLayer")
```

A data layer (roadmap R16): the Instances tagged with its Name (CollectionService), wherever they are in the tree. The editor's Layers panel shows, hides and locks them together; renaming the layer renames the tag.

It also has every member of [Instance](Instance.md).

### Properties

#### Color

```luau
DataLayer.Color: Color3
```

The colour the editor's Layers panel marks the layer with (roadmap R16).

#### ActiveOnStart

```luau
DataLayer.ActiveOnStart: boolean
```

If false, a game starts with the layer's Instances set aside, until a script makes it active (roadmap R16.4). The editor shows every layer whatever this is; hiding one there is the Layers panel's eye.

