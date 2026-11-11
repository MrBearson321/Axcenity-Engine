---
description: Sounds, music, spatial (3D) sound, rolloff and occlusion, sound groups, the Mixer and effects such as reverb and echo.
icon: speaker
samples: [hello-audio, hello-spatial, hello-mixer]
---

# Audio and spatial sound

Sounds are **Sound** Instances. Put one anywhere in the world (a part for a sound that comes from that part, or **SoundService** for music and interface sounds) and give it a **SoundId**: a WAV, FLAC, Ogg Vorbis or MP3 file in the project, such as `assets/audio/chime.wav`.

## Playing a sound

| Property | What it does |
|---|---|
| **SoundId** | The sound file. |
| **Volume** | 0 is silent, 1 is as recorded. |
| **Looped** | Start again at the end. |
| **PlaybackSpeed** | 2 plays twice as fast and an octave higher (0.01 to 20). |
| **Playing** | Tick it to play when the game starts. |

From a script:

```lua
local chime = workspace.Bell.Chime
chime:Play()
chime.Ended:Connect(function()
	print("The bell stopped ringing")
end)
```

`Sound:Stop()` stops and rewinds; `Sound:Pause()` and `Sound:Resume()` keep the place.

## Spatial sound

Tick **Spatial** on a Sound and it plays from a place in the world: it gets quieter with distance from the camera and pans left and right as you turn. The place is the part (or model part, or terrain) the Sound is inside, or its **Position** when it isn't in one. **Viewport > Overlays > Emitters** marks where sounds are in the editor.

- **RollOffMinDistance** (10 studs to start) is how far it stays at full volume, and **RollOffMaxDistance** (150) where it can no longer be heard.
- **RollOffMode** picks the curve: **Inverse** (natural, the default), **Linear**, **LinearSquare** (quieter sooner) or **InverseTapered** (natural, fading out smoothly at the end).
- **SoundService.Occlusion** (on to start) muffles sounds behind walls: each anchored solid part or terrain between the sound and the camera, up to three, halves it and makes it duller. Characters and loose parts don't count.
- A **ReverbZone** is a see-through part whose inside echoes like a room, a hall or a cave (**ReverbType**), fading in over **FadeTime** as the camera enters. It changes spatial sounds only, not music.

## Sound groups and the Mixer

A **SoundGroup** is a bus: point a Sound's **SoundGroup** property at one and the group's **Volume** scales it. Keep groups in **SoundService**, and put groups inside groups (Master with Music and SFX inside it). When a group has **UserAdjustable** on, the engine makes an **AudioSettings** screen with a volume slider for it and for the master volume; show it from a pause menu with `workspace.AudioSettings.Enabled = true`.

The **Mixer** panel (View > Panels) shows each group's level during Play, with mute and solo buttons to hear one group alone.

## Effects

Put effect Instances inside a Sound or a SoundGroup: **ReverbSoundEffect**, **EqualizerSoundEffect**, **CompressorSoundEffect**, **EchoSoundEffect**, **DistortionSoundEffect**, **PitchShiftSoundEffect**, **LowPassSoundEffect** and **HighPassSoundEffect**. Effects on a group apply to every sound in it. The **hello-mixer** sample has groups with effects.

> **Known limits.** Spatial sound pans in stereo; there's no HRTF (sound from above or behind) yet. Occlusion counts walls, it doesn't trace how sound travels around them.
