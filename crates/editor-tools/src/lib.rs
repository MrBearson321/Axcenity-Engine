//! The editor's viewport tools, below the editor and its studios so both
//! can use them: the Move, Scale and Rotate gizmo (`gizmo`), snapping to
//! other parts and to their points (`snapping`), the shortcut table
//! (`shortcuts`), where an Attachment is in the world (`attachments`),
//! posing a rig (`posing`) and the posed-model preview (`preview`).
//!
//! The editor keeps their old paths (`crate::gizmo`, `crate::shortcuts`,
//! `animation::preview`...) by re-exporting these modules.

// The editor-kit modules under the names this code used inside the editor.
use axcenity_editor_kit::{screen, theme, widgets};

pub mod attachments;
pub mod gizmo;
pub mod posing;
pub mod preview;
pub mod shortcuts;
pub mod snapping;
pub mod terrain_tool;
