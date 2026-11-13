---
description: "A color that changes over time from 0 to 1, such as a particle's color over its life."
---

# ColorSequence

A color that changes over time from 0 to 1, such as a particle's color over its life.

### Properties

#### Keypoints

```luau
ColorSequence.Keypoints: { ColorSequenceKeypoint }
```

Its keypoints, the first at time 0 and the last at 1.

### Functions

#### new

```luau
ColorSequence.new(colorOrKeypoints: Color3 | { ColorSequenceKeypoint }, endColor: Color3?): ColorSequence
```

The same color throughout, a start and an end color, or a list of 2 to 20 ColorSequenceKeypoints in time order from 0 to 1.

