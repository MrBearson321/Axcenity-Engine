# Portability

Which platforms the engine targets, and how file access stays portable.

Target Windows, macOS and Linux. All runtime file access goes through the asset system (the `assets` crate, since Phase 17), never direct `std::fs` calls in gameplay code, so web and mobile stay possible later.
