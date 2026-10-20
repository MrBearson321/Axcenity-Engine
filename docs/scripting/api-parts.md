# Part properties

Transparency, collision, glow, wind, and Anchored.

- Parts (Phase 35) also have `Transparency` (0–1), `CanCollide` (off: passed through, still fires `Touched`), `EmissiveColor` and `Emission` (a glow of any color). `WindAffected` (Part and MeshPart, default false) lets `workspace.GlobalWind` (a Vector3 in studs/s, default zero, saved in the world file only when set) push an unanchored part.
- `Part.Anchored` (default `false`): unanchored parts fall under physics. Setting `Position` or `Orientation` on one teleports it and stops it; physics writes both back as the part moves. See Physics in `docs/architecture/physics.md`.
