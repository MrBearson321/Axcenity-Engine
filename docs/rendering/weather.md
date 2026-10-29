# Weather

Rain and snow, and wet and snow-covered surfaces (graphics Phase 27). One state holds it all: Lighting's `Weather`. `scene/src/weather.rs` reads it and eases toward it, and `render::Weather` (`DrawList::weather`) is how wet and snowy surfaces are. The shaders read it from one uniform (`Frame::weather`, `weathered` in lighting.wgsl).

## The world's weather

A `Weather` inside Lighting (the Lighting ribbon's **Weather**, beside Clouds, or the Explorer's Insert menu; it always goes into Lighting). The first one is used. Without one, surfaces are dry and nothing falls.

- `Precipitation` (`Enum.Precipitation`: None, Rain or Snow; default Rain): what falls around the camera.
- `Intensity` (0 to 1, default 0.5): how hard it falls. Rain wets surfaces at least this much, and falling snow covers them at least this much.
- `Wetness` and `SnowCover` (0 to 1, default 0): how wet surfaces are and how much snow lies on them, whatever is falling, such as puddles after the rain or old snow.
- `TransitionTime` (seconds, default 10): how long a change takes to ease in. Rain starting and stopping, surfaces wetting and drying, and snow settling and melting each move at most one whole change per `TransitionTime`. 0 changes at once.
- The Workspace's `GlobalWind` blows the rain and snow.

## Easing

`settled` is the weather once eased in. `build_draw_list` puts it into the list, so a world drawn on its own (a still, a test, a probe bake) shows it settled. The host's `Particles` (player and editor viewport) owns a `WeatherState`. `Particles::update` eases it toward the world's weather, and `Particles::add_weather` puts the eased weather and the rain or snow into the host's draw list. The first update takes the world's weather as it is, so a world that starts snowy is already white. `Particles::clear` (when Play starts or stops) starts it again the same way. The editor keeps drawing frames while rain or snow falls or the weather is easing (`Particles::is_active`).

## Surfaces

`light_surface` runs `weathered` first, so parts, models, surface shaders and terrain all get it. See-through items get it too. Emission isn't changed.

- **Wet**: a non-metal's color is darkened toward half (water fills its pores), and the roughness is lowered toward 0.08 (a film of water), so the sun's reflection is sharper. Undersides stay drier (by the surface's own normal).
- **Snow**: covers surfaces by their own normal's height, `clamp(1.5 × cover − 2.5 × (1 − up))`. A flat top is fully white from two thirds cover, full cover reaches down to slopes of about 37°, and nothing steeper than about 66° gets any, so walls stay bare. Snow is a rough (0.8), non-metal off-white, and it smooths a normal map's bumps.

Nothing covers what's under a roof: a floor indoors gets wet or snowy too, and rain falls through roofs.

## Rain and snow

They fall in a 64 × 48 × 64 stud box around the camera (`BOX`). Each drop wraps from one side to the other, so the camera never runs out of them. There are up to 2500 raindrops and 2000 snowflakes. As many are drawn as the eased intensity says, as sprites that fade toward the box's sides. Raindrops fall at 60 studs a second (±20% each) and show a thin streak (`rain_streak`, a built-in picture). Snowflakes fall at 5 studs a second, sway 1.5 studs a second, and show the soft Circle picture. Both are carried by `GlobalWind`. Like other sprites they're unlit. They move only while some are drawn.

## Cost

Weather adds no pass, target or draw call apart from the sprites while something falls. Every lit pixel skips it on one uniform test while surfaces are dry and bare.

## Tests

- `render/src/renderer/tests/weather.rs` (GPU): wet surfaces are darker, and the sun's reflection on a wet floor is brighter. Snow whitens a block's top more as the cover grows, and leaves its wall exactly as it was. Clear weather draws the frame exactly as before, and weather adds no draw calls or target bytes.
- `scene/src/weather.rs`: a new world is dry; rain wets, `Wetness` and `SnowCover` set floors, and Intensity is held to 1. Wetness eases in over `TransitionTime` and dries after, and 0 changes at once. Drops lie in the box around the camera, fall and are blown by the wind, and as many show as the intensity says.
- `player/src/weather_tests.rs`: in a host's frames (GPU), rain darkens a floor over time, and it lightens back to exactly dry after. A script makes snowy Weather, `Enum.Precipitation` compares, and it saves, reloads and draws the same.
