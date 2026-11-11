---
description: How sound plays, supported files, spatial sound with rolloff curves and occlusion, reverb zones, sound groups as buses, each effect and its settings, and volume settings for players.
icon: speaker
samples: [hello-audio, hello-spatial, hello-mixer]
---

# Audio

Audio plays through **kira** on its own thread, so sound keeps playing smoothly whatever the frame rate. Scripts and the world say what should play; the engine syncs every Sound each frame. See [Audio and spatial sound](../learn/audio.md) for a walkthrough.

## Files

WAV, FLAC, Ogg Vorbis and MP3, anywhere in the project (usually `assets/audio/`). `Sound.SoundId` names the file by its path.

## Sound

| Member | Notes |
|---|---|
| `Sound.Volume` | A multiplier: 1 is as recorded, 0 silent. |
| `Sound.PlaybackSpeed` | Speed and pitch together, 0.01 to 20. |
| `Sound.Looped` | Repeat the whole file. |
| `Sound.Playing` | True while playing; setting it pauses or resumes. Saved true, it starts with the game. |
| `Sound:Play()`, `Sound:Stop()` | From the start; stop and rewind. |
| `Sound:Pause()`, `Sound:Resume()` | Keep the place. |
| `Sound.Ended` | A sound that doesn't loop reached its end (not after `Stop`). |

Changes to volume, speed and looping apply to a sound that's already playing.

## Spatial sound

With `Sound.Spatial` on, a Sound plays from its part (or `Sound.Position` when it isn't in one) and is heard from the **camera**: it pans in stereo and fades with distance.

| Rolloff mode | Curve |
|---|---|
| Inverse (default) | Full volume within `Sound.RollOffMinDistance` (10), then falling as min ÷ distance, as sound does in the open. Silent from `Sound.RollOffMaxDistance` (150). |
| Linear | A straight fade from min to max distance. |
| LinearSquare | Quieter sooner. |
| InverseTapered | Inverse, fading smoothly to silence near the max distance. |

**Occlusion** (`SoundService.Occlusion`, on to start): each anchored, solid part or terrain between a spatial sound and the camera, up to three, halves its volume and lowers a low-pass filter (2,000, 1,000, then 500 Hz). Loose parts, characters and parts with CanCollide off don't count.

**Reverb zones**: while the camera is inside a **ReverbZone** part, its preset (`ReverbZone.ReverbType`: Room, StoneRoom, Hallway, Cave, ConcertHall, Arena, SewerPipe) blends onto every spatial sound over `ReverbZone.FadeTime`. Music (non-spatial sounds) isn't changed.

## Buses

A **SoundGroup** is a bus: `Sound.SoundGroup` routes a sound into one, and groups inside groups multiply down (Master, then Music and SFX). Keep them in **SoundService** so the world file and save slots keep their volumes. The **Mixer** panel shows each group's level in Play, with mute and solo.

## Effects

Effects are children of a Sound or a SoundGroup, applied in **Priority** order; each has **Enabled**.

| Effect | Settings |
|---|---|
| `ReverbSoundEffect` | DecayTime, Density, DryLevel, WetLevel |
| `EqualizerSoundEffect` | LowGain, MidGain, HighGain (dB), MidRange |
| `CompressorSoundEffect` | Threshold, Ratio, Attack, Release, GainMakeup |
| `EchoSoundEffect` | Delay, Feedback, DryLevel, WetLevel |
| `DistortionSoundEffect` | Level |
| `PitchShiftSoundEffect` | Octave (0.5 to 2), same speed |
| `LowPassSoundEffect`, `HighPassSoundEffect` | Cutoff, Resonance |

## Volume settings for players

`UserGameSettings.MasterVolume` is the game's master volume. A SoundGroup with `SoundGroup.UserAdjustable` on makes the engine build an **AudioSettings** ScreenGui with a slider per adjustable group; show it with `workspace.AudioSettings.Enabled = true`. Any Slider with an ObjectValue named `SoundGroup` inside it becomes a volume slider for that group.

> **Known limits.** Panning is stereo only (no HRTF). Occlusion counts walls between the sound and the camera; it doesn't model sound travelling around them.

More: [Audio in the engineering wiki](https://github.com/MrBearson321/axcenity-engine/blob/main/wiki/Engine-Systems/Audio.md).
