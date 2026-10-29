//! Screen-space outlines (graphics Phase 31): Lighting's first enabled
//! `OutlineEffect`. Without one there are none.

use axcenity_core::{ClassName, Value, World};
use axcenity_render::Edges;

/// The edges Lighting's `OutlineEffect` draws, if it has an enabled one.
pub(crate) fn edges_of(world: &World) -> Option<Edges> {
    let lighting = world.get(world.lighting())?;
    let effect = lighting
        .children()
        .iter()
        .filter_map(|&id| world.get(id))
        .find(|child| child.class() == ClassName::OutlineEffect)?;
    if effect.property("Enabled") == Some(&Value::Bool(false)) {
        return None;
    }
    let defaults = Edges::default();
    Some(Edges {
        color: effect
            .property("Color")
            .and_then(Value::as_color3)
            .map_or(defaults.color, |color| [color.r, color.g, color.b]),
        thickness: match effect.property("Thickness") {
            Some(Value::Number(value)) if value.is_finite() => value.max(0.0) as f32,
            _ => defaults.thickness,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::Color3;

    #[test]
    fn lightings_outline_effect_draws_edges_unless_disabled() {
        let mut world = World::new();
        assert_eq!(edges_of(&world), None, "a new world has none");
        let effect = world.create(ClassName::OutlineEffect).unwrap();
        world.set_parent(effect, Some(world.lighting())).unwrap();
        assert_eq!(edges_of(&world), Some(Edges::default()));

        let set = |world: &mut World, name: &str, value| {
            world.set_property(effect, name, value).unwrap();
        };
        set(
            &mut world,
            "Color",
            Value::Color3(Color3::new(1.0, 0.5, 0.0)),
        );
        set(&mut world, "Thickness", Value::Number(3.0));
        assert_eq!(
            edges_of(&world),
            Some(Edges {
                color: [1.0, 0.5, 0.0],
                thickness: 3.0,
            })
        );
        set(&mut world, "Enabled", Value::Bool(false));
        assert_eq!(edges_of(&world), None);

        // One anywhere else does nothing.
        world.set_parent(effect, Some(world.workspace())).unwrap();
        set(&mut world, "Enabled", Value::Bool(true));
        assert_eq!(edges_of(&world), None);
    }
}
