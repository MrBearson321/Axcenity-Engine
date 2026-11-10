# Portability

Which platforms the engine targets, and how file access stays portable.

Desktop comes first, with Windows and Linux as the primary platforms. macOS still builds and ships in releases, but it's secondary: a change doesn't have to be verified there. Mobile and web aren't immediate priorities ([development policy](../development-policy.md#platforms)), so don't give up a desktop capability for them, but avoid platform-specific dependencies where practical. All runtime file access goes through the asset system (the `assets` crate, since Phase 17), never direct `std::fs` calls in gameplay code, so web and mobile stay possible later. The one exception is save slots, which the `save` crate reads and writes in the player's data folder (`save::game_folder`, per OS; see `docs/architecture/saves.md`).
