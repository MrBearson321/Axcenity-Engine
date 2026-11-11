---
description: "Loads files ahead of time, so they don't load the moment they're first needed, from game:GetService(\"ContentProvider\")."
---

# ContentProvider

Loads files ahead of time, so they don't load the moment they're first needed, from `game:GetService("ContentProvider")`. Both sides have it.

### Properties

#### RequestQueueSize

```luau
ContentProvider.RequestQueueSize: number
```

How many files are still waiting or loading in the background. Read-only.

### Functions

#### PreloadAsync

```luau
ContentProvider:PreloadAsync(contentIds: { string | Instance }, callback: ((string, EnumAssetFetchStatus) -> ())?): ()
```

Pauses the script until everything in the list has loaded or failed; the game and other scripts go on meanwhile. An entry is a file path in the project (such as `"assets/models/chair.glb"`) or an Instance, which stands for the files it and its descendants use. A file that can't be loaded isn't an error here: why goes to the Output, and `callback`, if given, is called for each file with its path and `Enum.AssetFetchStatus.Success` or `Failure`.

