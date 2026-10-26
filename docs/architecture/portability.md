# Portability

Which platforms the engine targets, and how file access stays portable.

Target Windows, macOS and Linux. All runtime file access goes through the asset system (the `assets` crate, since Phase 17), never direct `std::fs` calls in gameplay code, so web and mobile stay possible later. The one exception is save slots, which the `save` crate reads and writes in the player's data folder (`save::game_folder`, per OS; see `docs/architecture/saves.md`).
