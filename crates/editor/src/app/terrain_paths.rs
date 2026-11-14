//! TerrainPaths in the editor (procedural worlds PW4): Apply flattens and
//! paints the ground under a path, and Route lays its Spline's points over
//! the terrain within its MaxGrade. Each is one undo step. Scatter rules
//! keep off a path by themselves (`procedural::terrain_scatter`), so moving
//! its points needs nothing here.

use std::sync::Arc;

use axcenity_core::spline::SplineCurve;
use axcenity_core::{TerrainData, Value, Vector3};
use axcenity_procedural::paths::{PathSettings, spline_of};
use axcenity_terrain::Placement;

use super::*;

impl EditorApp {
    /// The TerrainPath `id`'s settings, its Spline and their names, or
    /// `None` (with a warning) when it's off or not in a Spline.
    fn terrain_path(&mut self, id: InstanceId) -> Option<(PathSettings, InstanceId, String)> {
        let instance = self.world.get(id)?;
        if instance.class() != ClassName::TerrainPath {
            return None;
        }
        let name = instance.name().to_owned();
        let Some(settings) = PathSettings::of(instance) else {
            self.output
                .warning(format!("{name} is switched off (Enabled)."));
            return None;
        };
        let Some(spline) = spline_of(&self.world, id) else {
            self.output.warning(format!(
                "{name} isn't in a Spline: put it in one, beside its road's Spline Mesh."
            ));
            return None;
        };
        Some((settings, spline, name))
    }

    /// The terrains in the Workspace, with where they are.
    fn terrains(&self) -> Vec<(InstanceId, Placement)> {
        self.world
            .descendants(self.world.workspace())
            .into_iter()
            .filter(|&id| self.world.terrain(id).is_some())
            .filter_map(|id| Some((id, Placement::of(self.world.get(id)?)?)))
            .collect()
    }

    /// Apply to Terrain: every terrain under the TerrainPath `id`'s Spline
    /// flattened to its curve (with `Flatten`) and painted with its
    /// `Layer`, `Width` wide and blending in over `Falloff`, as one undo
    /// step.
    pub(super) fn apply_terrain_path(&mut self, id: InstanceId) {
        if self.playing() {
            return;
        }
        let Some((settings, spline, name)) = self.terrain_path(id) else {
            return;
        };
        let Some(curve) = SplineCurve::of(&self.world, spline) else {
            self.output
                .warning(format!("{name}'s Spline has no points to apply along."));
            return;
        };
        if !settings.flatten && settings.layer.is_empty() {
            self.output.warning(format!(
                "{name} has Flatten off and no Layer, so there's nothing to apply."
            ));
            return;
        }
        let terrains = self.terrains();
        if terrains.is_empty() {
            self.output
                .error("Insert a Terrain first: Apply flattens and paints the ground.");
            return;
        }
        let before = self.world.clone();
        let mut changed = 0;
        let mut missing_layer = false;
        for (terrain, placement) in terrains {
            let layer = if settings.layer.is_empty() {
                None
            } else {
                let found = axcenity_terrain::materials::layers(&self.world, terrain)
                    .iter()
                    .position(|&layer| {
                        self.world
                            .get(layer)
                            .is_some_and(|l| l.name() == settings.layer)
                    });
                missing_layer |= found.is_none();
                found
            };
            let Some(mut data) = self.world.terrain(terrain).map(|d| TerrainData::clone(d)) else {
                continue;
            };
            let set =
                axcenity_procedural::apply_path(&mut data, &placement, &curve, &settings, layer);
            if set > 0 {
                changed += set;
                self.world.set_terrain(terrain, Arc::new(data));
            }
        }
        if missing_layer {
            self.output.warning(format!(
                "{name}'s Layer {} isn't one of the terrain's layers, so it wasn't painted.",
                settings.layer
            ));
        }
        if changed == 0 {
            self.world = before;
            self.output.warning(format!(
                "{name} isn't over a Terrain, so nothing was changed."
            ));
            return;
        }
        self.history
            .record_world(&mut self.world, format!("Apply {name}"), before);
        self.output.info(format!(
            "Applied {name} to {changed} terrain samples, {} studs wide.",
            settings.width
        ));
    }

    /// Route: the TerrainPath `id`'s Spline laid over the terrain from its
    /// first point to its last, no stretch steeper than `MaxGrade`; the
    /// points between are replaced, the ends put on the ground. One undo
    /// step.
    pub(super) fn route_terrain_path(&mut self, id: InstanceId) {
        if self.playing() {
            return;
        }
        let Some((settings, spline, name)) = self.terrain_path(id) else {
            return;
        };
        let closed = self
            .world
            .get(spline)
            .is_some_and(|s| s.property("Closed") == Some(&Value::Bool(true)));
        let points = crate::splines::points(&self.world, spline);
        let ends = (points.first().copied(), points.last().copied());
        let (Some(first), Some(last)) = ends else {
            return;
        };
        if closed || first == last {
            self.output.warning(format!(
                "Route needs an open Spline with two or more points: {name} goes from its \
                 first point to its last."
            ));
            return;
        }
        let position = |app: &Self, point: InstanceId| {
            app.world
                .get(point)
                .and_then(|p| p.property("Position"))
                .and_then(Value::as_vector3)
                .map(|v| axcenity_render::glam::Vec3::new(v.x, v.y, v.z))
        };
        let (Some(from), Some(to)) = (position(self, first), position(self, last)) else {
            return;
        };
        let terrain = self.terrains().into_iter().find(|(terrain, placement)| {
            self.world.terrain(*terrain).is_some_and(|data| {
                placement.ground_y(data, from.x, from.z).is_some()
                    && placement.ground_y(data, to.x, to.z).is_some()
            })
        });
        let Some((terrain, placement)) = terrain else {
            self.output.warning(format!(
                "Both ends of {name}'s Spline have to be over the same Terrain to route it."
            ));
            return;
        };
        let Some(data) = self.world.terrain(terrain) else {
            return;
        };
        let Some(route) = axcenity_procedural::route(
            data,
            &placement,
            (from, to),
            settings.max_grade,
            settings.width,
        ) else {
            self.output.warning(format!(
                "No way between {name}'s ends climbs less than {} degrees: raise its Max \
                 Grade or move an end.",
                settings.max_grade
            ));
            return;
        };
        let before = self.world.clone();
        for &point in &points[1..points.len() - 1] {
            let _ = self.world.destroy(point);
        }
        let at = |p: axcenity_render::glam::Vec3| Value::Vector3(Vector3::new(p.x, p.y, p.z));
        let _ = self.world.set_property(first, "Position", at(route[0]));
        let _ = self
            .world
            .set_property(last, "Position", at(route[route.len() - 1]));
        for (index, &p) in route[1..route.len() - 1].iter().enumerate() {
            let point =
                super::splines::new_point(&mut self.world, spline, Vector3::new(p.x, p.y, p.z));
            let place = self
                .world
                .get(spline)
                .and_then(|s| s.children().iter().position(|&c| c == first))
                .map_or(0, |i| i + 1 + index);
            let _ = self.world.set_parent_at(point, spline, place);
        }
        self.history
            .record_world(&mut self.world, format!("Route {name}"), before);
        let length = route
            .windows(2)
            .map(|pair| pair[0].distance(pair[1]))
            .sum::<f32>();
        self.output.info(format!(
            "Routed {name}: {} points, {length:.0} studs, no steeper than {} degrees.",
            route.len(),
            settings.max_grade
        ));
        self.generate_frame();
    }
}
