//! Fill with Probes (Lights & Sky ▸ Light, roadmap R18.4): a
//! LightProbeVolume sized to the selected parts or model, with its
//! `Spacing` picked so its probes fit a budget. Probes that land inside a
//! solid part are counted, and Bake Probes leaves them out (R18.2's
//! invalid probes), so a filled room's walls don't leak light.

use axcenity_assets::MAX_LIGHT_PROBES_PER_SIDE;
use axcenity_render::glam::Vec3;

use super::*;
use crate::history::Place;

/// The most probes Fill with Probes gives a volume: 64 KB of probes (128
/// KB with the sky's share), an eighth of the most one volume can hold.
pub(super) const FILL_PROBE_BUDGET: u32 = 512;

/// Studs a filled box reaches past what it fills on every side: a volume's
/// default `BlendDistance`, so its light has faded in by the outer faces.
const FILL_MARGIN: f32 = 2.0;

/// The closest probes a fill places, in studs, and the step its spacing is
/// picked in (tidy numbers in Properties).
const FILL_SPACING_STEP: f32 = 0.5;

/// What Fill with Probes made, for its Output line and for tests.
#[derive(Debug)]
pub(super) struct Filled {
    pub(super) part: InstanceId,
    pub(super) volume: InstanceId,
    pub(super) spacing: f32,
    pub(super) probes: u32,
    /// How many stand inside a solid part or under the ground: Bake Probes
    /// leaves them out.
    pub(super) inside_solids: u32,
}

impl EditorApp {
    /// Puts a LightProbeVolume around the selection (its parts, or a
    /// model's), in an invisible part of its own in the Workspace, as one
    /// undo step, and selects it. Says in Output what it made.
    pub(super) fn fill_with_probes(&mut self) -> Option<Filled> {
        let roots: Vec<InstanceId> = self
            .selection_roots()
            .into_iter()
            .filter(|&id| {
                self.world
                    .get(id)
                    .is_some_and(|instance| instance.class() != ClassName::Terrain)
            })
            .collect();
        let Some((low, high)) = super::assets::bounds(&self.world, &roots) else {
            self.output.warning(
                "Fill with Probes fills the box around the selection: select the parts or the \
                 model to light first.",
            );
            return None;
        };
        let low = low - Vec3::splat(FILL_MARGIN);
        let high = high + Vec3::splat(FILL_MARGIN);
        let size = high - low;
        let middle = (low + high) / 2.0;
        let spacing = budget_spacing(size, FILL_PROBE_BUDGET);
        let name = match roots.as_slice() {
            [one] => format!(
                "{}Probes",
                self.world.get(*one).map_or("", |instance| instance.name())
            ),
            _ => "LightProbeBox".to_owned(),
        };
        let workspace = self.world.workspace();
        let name = tree::unique_name(&self.world, workspace, &name);
        let vector = |v: Vec3| Value::Vector3(Vector3::new(v.x, v.y, v.z));
        let part = self.world.create(ClassName::Part).ok()?;
        let volume = self.world.create(ClassName::LightProbeVolume).ok()?;
        for (property, value) in [
            ("Anchored", Value::Bool(true)),
            ("Size", vector(size)),
            ("Position", vector(middle)),
            ("Transparency", Value::Number(1.0)),
            ("CanCollide", Value::Bool(false)),
            ("CanQuery", Value::Bool(false)),
            ("CanTouch", Value::Bool(false)),
            ("CastShadow", Value::Bool(false)),
        ] {
            let _ = self.world.set_property(part, property, value);
        }
        let _ = self.world.set_name(part, &name);
        let _ = self
            .world
            .set_property(volume, "Spacing", Value::Number(f64::from(spacing)));
        let _ = self.world.set_parent(volume, Some(part));
        let end = self.world.get(workspace).map_or(0, |w| w.children().len());
        let place = Place {
            parent: workspace,
            index: end,
        };
        if !self.place(part, Some(place), format!("Fill {name} with probes")) {
            return None;
        }
        self.select_from_viewport(Some(volume));
        let points = axcenity_scene::light_probe_points(&self.world, volume)?;
        let probes = points.points.len() as u32;
        let inside_solids =
            super::probes::invalid_probes(&self.world, &self.assets, volume, &points.points).len()
                as u32;
        let [x, y, z] = points.counts;
        let mut line = format!(
            "Filled {name} with {probes} light probes ({x} × {y} × {z}, {spacing} studs apart)"
        );
        if inside_solids > 0 {
            line.push_str(&format!(
                "; {inside_solids} stand inside solid parts and are left out of the bake"
            ));
        }
        line.push_str(". Bake Probes to light them.");
        self.output.info(line);
        Some(Filled {
            part,
            volume,
            spacing,
            probes,
            inside_solids,
        })
    }
}

/// How many probes a volume of `size` holds at `spacing`: as
/// `axcenity_scene::light_probe_points` places them.
fn probe_count(size: Vec3, spacing: f32) -> u32 {
    size.to_array()
        .map(|side| ((side / spacing).ceil() as u32).clamp(1, MAX_LIGHT_PROBES_PER_SIDE))
        .iter()
        .product()
}

/// The closest spacing, in steps of [`FILL_SPACING_STEP`] studs, at which a
/// volume of `size` holds at most `budget` probes.
pub(super) fn budget_spacing(size: Vec3, budget: u32) -> f32 {
    let mut spacing = FILL_SPACING_STEP;
    // Fewer probes the farther apart, down to one, so this ends.
    while probe_count(size, spacing) > budget.max(1) {
        spacing += FILL_SPACING_STEP;
    }
    spacing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_picks_the_closest_spacing_that_fits() {
        // A 20-stud cube: 8 a side (512) at 2.5 studs; 2 would be 10 a side.
        assert_eq!(budget_spacing(Vec3::splat(20.0), 512), 2.5);
        assert_eq!(probe_count(Vec3::splat(20.0), 2.5), 512);
        // A small box gets the closest spacing there is.
        assert_eq!(budget_spacing(Vec3::splat(1.0), 512), FILL_SPACING_STEP);
        // A long hall: its length is capped at 16 a side, so the budget
        // goes to its width and height.
        let hall = Vec3::new(400.0, 6.0, 6.0);
        let spacing = budget_spacing(hall, 512);
        assert!(probe_count(hall, spacing) <= 512);
        assert!(probe_count(hall, spacing - FILL_SPACING_STEP) > 512);
        // Every budget is kept, however big the box.
        for budget in [1, 8, 100, 4096] {
            let big = Vec3::new(900.0, 120.0, 700.0);
            assert!(probe_count(big, budget_spacing(big, budget)) <= budget);
        }
    }

    /// A room of four walls and a floor with a thick pillar in the middle,
    /// as a Model.
    fn room(app: &mut EditorApp) -> InstanceId {
        let workspace = app.world.workspace();
        let model = app.world.create(ClassName::Model).unwrap();
        app.world.set_name(model, "Room").unwrap();
        app.world.set_parent(model, Some(workspace)).unwrap();
        let walls = [
            (Vec3::new(0.0, -0.5, 0.0), Vec3::new(20.0, 1.0, 20.0)),
            (Vec3::new(-9.5, 5.0, 0.0), Vec3::new(1.0, 10.0, 20.0)),
            (Vec3::new(9.5, 5.0, 0.0), Vec3::new(1.0, 10.0, 20.0)),
            (Vec3::new(0.0, 5.0, -9.5), Vec3::new(20.0, 10.0, 1.0)),
            (Vec3::new(0.0, 5.0, 9.5), Vec3::new(20.0, 10.0, 1.0)),
            (Vec3::new(0.0, 5.0, 0.0), Vec3::new(4.0, 10.0, 4.0)),
        ];
        for (at, size) in walls {
            let wall = app.world.create(ClassName::Part).unwrap();
            let vector = |v: Vec3| Value::Vector3(Vector3::new(v.x, v.y, v.z));
            app.world.set_property(wall, "Position", vector(at)).unwrap();
            app.world.set_property(wall, "Size", vector(size)).unwrap();
            app.world
                .set_property(wall, "Anchored", Value::Bool(true))
                .unwrap();
            app.world.set_parent(wall, Some(model)).unwrap();
        }
        model
    }

    #[test]
    fn filling_a_model_sizes_a_volume_to_it_within_the_budget_and_counts_probes_in_its_walls() {
        let mut app = EditorApp::new(None);
        let model = room(&mut app);
        app.select_from_viewport(Some(model));
        let filled = app.fill_with_probes().expect("filled");

        let part = app.world.get(filled.part).unwrap();
        assert_eq!(part.name(), "RoomProbes");
        assert_eq!(part.parent(), Some(app.world.workspace()));
        let vector = |name| part.property(name).and_then(Value::as_vector3).unwrap();
        // The room's box (20 × 11 × 20) and 2 studs more on every side.
        let size = vector("Size");
        assert_eq!((size.x, size.y, size.z), (24.0, 15.0, 24.0));
        let at = vector("Position");
        assert_eq!((at.x, at.y, at.z), (0.0, 4.5, 0.0));
        assert_eq!(part.property("CanCollide"), Some(&Value::Bool(false)));
        assert_eq!(part.property("Transparency"), Some(&Value::Number(1.0)));

        let volume = app.world.get(filled.volume).unwrap();
        assert_eq!(volume.class(), ClassName::LightProbeVolume);
        assert_eq!(volume.parent(), Some(filled.part));
        assert_eq!(
            volume.property("Spacing").and_then(Value::as_number),
            Some(f64::from(filled.spacing))
        );
        assert!(filled.probes <= FILL_PROBE_BUDGET, "{filled:?}");
        assert!(
            filled.probes > FILL_PROBE_BUDGET / 2,
            "the budget is used: {filled:?}"
        );
        // The pillar is solid: the probes standing in it are counted, and
        // the bake leaves them out. The room's air holds most of them.
        assert!(filled.inside_solids > 0, "{filled:?}");
        assert!(filled.inside_solids < filled.probes / 2, "{filled:?}");
        assert_eq!(app.selected, Some(filled.volume));

        // One undo step takes it all away.
        assert_eq!(
            app.history.undo_label(),
            Some("Fill RoomProbes with probes")
        );
        app.undo();
        assert!(app.world.get(filled.part).is_none_or(|p| p.parent().is_none()));
    }

    #[test]
    fn filling_with_nothing_selected_explains_itself() {
        let mut app = EditorApp::new(None);
        app.selection.clear();
        app.selected = None;
        let before = app.world.descendants(app.world.workspace()).len();
        assert!(app.fill_with_probes().is_none());
        assert_eq!(app.world.descendants(app.world.workspace()).len(), before);
        assert!(
            app.output
                .lines()
                .iter()
                .any(|line| line.text.contains("select the parts or the model")),
        );
    }
}
