---
description: "A number that changes over time from 0 to 1, such as a particle's size over its life: keypoints in time order, with straight lines between them."
---

# NumberSequence

A number that changes over time from 0 to 1, such as a particle's size over its life: keypoints in time order, with straight lines between them.

### Properties

#### Keypoints

```luau
NumberSequence.Keypoints: { NumberSequenceKeypoint }
```

Its keypoints, the first at time 0 and the last at 1.

### Functions

#### new

```luau
NumberSequence.new(valueOrKeypoints: number | { NumberSequenceKeypoint }, endValue: number?): NumberSequence
```

The same number throughout (`NumberSequence.new(2)`), a start and an end (`NumberSequence.new(1, 0)`), or a list of 2 to 20 NumberSequenceKeypoints in time order from 0 to 1.

