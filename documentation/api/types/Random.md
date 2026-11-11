---
description: "A random number generator from Random.new(seed)."
---

# Random

A random number generator from `Random.new(seed)`. The same seed gives the same numbers on every run and platform.

### Functions

#### NextNumber

```luau
Random:NextNumber(min: number?, max: number?): number
```

A number from `min` (left out: 0) up to `max` (left out: 1).

#### NextInteger

```luau
Random:NextInteger(min: number, max: number): number
```

A whole number from `min` to `max`, both included.

#### NextUnitVector

```luau
Random:NextUnitVector(): Vector3
```

A direction one stud long, every direction equally likely.

#### Shuffle

```luau
Random:Shuffle(list: { any }): ()
```

Puts the items of a list in a random order, in place.

#### Clone

```luau
Random:Clone(): Random
```

A new generator that gives the same numbers this one would from here.

#### new

```luau
Random.new(seed: number?): Random
```

A generator. The same `seed` gives the same numbers on every run and platform; left out, each generator is different.

