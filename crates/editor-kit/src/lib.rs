//! The editor's shared UI base: the design system ([`theme`]), its controls
//! ([`widgets`]), layouts that fit narrow panels ([`responsive`]), colour
//! swatches, Lua syntax colours, mouse-wheel and screen-space helpers, and
//! offscreen snapshots for drawing tests.
//!
//! The editor and its panel crates build on it (roadmap G11: splitting the
//! editor into crates, `docs/research/engine-gap-analysis.md`). It knows
//! nothing about the editor's state.

pub mod color_palette;
pub mod lua_syntax;
pub mod responsive;
pub mod screen;
#[cfg(any(test, feature = "test-support"))]
pub mod snapshot;
pub mod theme;
pub mod wheel;
pub mod widgets;
