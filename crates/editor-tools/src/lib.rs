//! The editor's viewport tools, below the editor and its studios so both
//! can use them: the Move, Scale and Rotate gizmo (`gizmo`), snapping to
//! other parts and to their points (`snapping`), the shortcut table
//! (`shortcuts`), where an Attachment is in the world (`attachments`),
//! posing a rig (`posing`) and the posed-model preview (`preview`).
//!
//! It also holds the small editor types and rules the panel crates share
//! with the editor: what a Properties-style panel hands the app (`edit`),
//! the Assets panel's tiles (`asset_entry`, `props`), the rules for editing
//! the Instance tree (`tree`), how a click changes a selection
//! (`selection`) and what can be animated (`animatable`).
//!
//! The editor keeps their old paths (`crate::gizmo`, `crate::shortcuts`,
//! `animation::preview`...) by re-exporting these modules.

// The editor-kit modules under the names this code used inside the editor.
use axcenity_editor_kit::{screen, theme, widgets};

pub mod animatable;
pub mod asset_entry;
pub mod attachments;
pub mod edit;
pub mod gizmo;
pub mod posing;
pub mod preview;
pub mod props;
pub mod selection;
pub mod shortcuts;
pub mod snapping;
pub mod terrain_tool;
pub mod tree;
