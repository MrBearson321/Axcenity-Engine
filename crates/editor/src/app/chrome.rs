//! Menus, the ribbon, dock panels and keyboard shortcuts.
//!
//! Drawing a panel may change the selection. The frame in `app.rs` applies
//! viewport results after the dock pass. These are methods of [`super::EditorApp`].

use super::*;

mod menus;
mod panes;
mod ribbon_menus;
mod toolbar;
mod windows;

pub(super) use menus::HISTORY_ROWS;
use windows::*;
pub(super) use windows::{paused_panel, playing_badge};

impl EditorApp {}
