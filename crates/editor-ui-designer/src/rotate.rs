//! Turning the selected element with the knobs just outside its corners:
//! its `Rotation` follows the pointer around its middle. Shift snaps it to
//! 15° steps. One drag is one undo step.

use axcenity_core::{InstanceId, World};
use eframe::egui;

use crate::ui_handles::Handles;

/// The step Shift snaps to, in degrees.
pub const SNAP_DEGREES: f64 = 15.0;

/// An element being turned by a rotate knob.
#[derive(Clone, Debug)]
pub struct RotateDrag {
    pub id: InstanceId,
    /// What it turns around, on screen, in points.
    centre: egui::Pos2,
    /// Its `Rotation` when the drag started.
    start: f64,
    /// The pointer's angle around the centre last frame, in degrees.
    last: f32,
    /// How far the pointer has gone around since the start, in degrees,
    /// counting whole turns.
    turned: f32,
}

impl RotateDrag {
    /// Starts turning the element `handles` are on, from `pointer`.
    pub fn start(handles: &Handles, pointer: egui::Pos2, world: &World) -> Option<RotateDrag> {
        let id = handles.element.id;
        let start = world
            .get(id)?
            .property("Rotation")?
            .as_number()
            .filter(|n| n.is_finite())?;
        let centre = handles.centre();
        Some(RotateDrag {
            id,
            centre,
            start,
            last: angle(centre, pointer),
            turned: 0.0,
        })
    }

    /// The `Rotation` with the pointer at `pointer`, in tenths of a degree,
    /// or in [`SNAP_DEGREES`] steps when `snap`.
    pub fn update(&mut self, pointer: egui::Pos2, snap: bool) -> f64 {
        let now = angle(self.centre, pointer);
        // The shorter way from last frame's angle, so passing straight
        // left (where the angle jumps from 180° to -180°) carries on.
        self.turned += (now - self.last + 540.0).rem_euclid(360.0) - 180.0;
        self.last = now;
        let rotation = self.start + f64::from(self.turned);
        if snap {
            (rotation / SNAP_DEGREES).round() * SNAP_DEGREES
        } else {
            (rotation * 10.0).round() / 10.0
        }
    }
}

/// The angle of `point` around `centre`, in degrees clockwise from the
/// right (screen y runs down).
fn angle(centre: egui::Pos2, point: egui::Pos2) -> f32 {
    let from = point - centre;
    from.y.atan2(from.x).to_degrees()
}

/// What a `Rotation` reads as on the artboard while turning: `30°`.
pub fn label(rotation: f64) -> String {
    let rounded = (rotation * 10.0).round() / 10.0;
    format!("{rounded}°")
}
