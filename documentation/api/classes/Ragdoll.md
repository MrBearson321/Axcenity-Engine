---
description: "A ragdoll for the skinned MeshPart it's in: a capsule Part per main bone, named after the bone, and a BallSocketConstraint per joint, which Build (in its Properties) makes from the skeleton with a humanoid preset's limits."
---

# Ragdoll

```luau
Instance.new("Ragdoll")
```

A ragdoll for the skinned MeshPart it's in: a capsule Part per main bone, named after the bone, and a BallSocketConstraint per joint, which Build (in its Properties) makes from the skeleton with a humanoid preset's limits. While Enabled, physics moves the capsules and they pose the bones; otherwise they follow the animation and physics leaves them out.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
Ragdoll.Enabled: boolean
```

If true, physics moves the ragdoll's capsules and they pose the bones (the MeshPart follows the hips across the ground). Setting it false blends back to the Animator's pose over BlendTime, after playing GetUpState.

#### BlendTime

```luau
Ragdoll.BlendTime: number
```

Seconds to blend from where the ragdoll lies back to the Animator's pose once Enabled is false.

#### GetUpState

```luau
Ragdoll.GetUpState: string
```

The name of the Animator's state to play (with no blend of its own) when Enabled turns false, such as a getting-up clip, or empty to carry on with the state playing.

### Functions

#### ApplyImpulse

```luau
Ragdoll:ApplyImpulse(bone: string, impulse: Vector3): ()
```

Pushes the body of the bone named `bone` (`"Head"` finds `mixamorig:Head`) through its center of mass: its velocity changes by `impulse` over its mass. Turn the ragdoll on in the same frame (or before) for the push to knock it.

