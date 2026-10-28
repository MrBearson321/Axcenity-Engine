//! The world's clouds (graphics Phase 26): Lighting's first `Clouds`,
//! carried by the Workspace's `GlobalWind`. Without one there are none.

use axcenity_core::{ClassName, Instance, Value, World};
use axcenity_render::Clouds;

/// The clouds the Lighting holds.
pub(crate) fn clouds_of(world: &World, lighting: &Instance) -> Clouds {
    let Some(clouds) = lighting
        .children()
        .iter()
        .filter_map(|&id| world.get(id))
        .find(|child| child.class() == ClassName::Clouds)
    else {
        return Clouds::default();
    };
    let number_of = |name, default: f32| match clouds.property(name) {
        Some(Value::Number(value)) if value.is_finite() => value.clamp(0.0, 1.0) as f32,
        _ => default,
    };
    let defaults = Clouds::default();
    Clouds {
        cover: number_of("Cover", 0.5),
        density: number_of("Density", defaults.density),
        color: clouds
            .property("Color")
            .and_then(Value::as_color3)
            .map_or(defaults.color, |color| [color.r, color.g, color.b]),
        wind: crate::particles::global_wind(world),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{Color3, Vector3};

    fn clouds_in(world: &World) -> Clouds {
        clouds_of(world, world.get(world.lighting()).unwrap())
    }

    #[test]
    fn a_new_world_has_no_clouds() {
        assert!(!clouds_in(&World::new()).is_drawn());
    }

    #[test]
    fn lightings_clouds_take_their_properties_and_the_wind() {
        let mut world = World::new();
        let clouds = world.create(ClassName::Clouds).unwrap();
        world.set_parent(clouds, Some(world.lighting())).unwrap();
        let drawn = clouds_in(&world);
        assert!(drawn.is_drawn() && !drawn.is_moving(), "{drawn:?}");
        assert_eq!((drawn.cover, drawn.density), (0.5, 0.7));
        assert_eq!(drawn.color, [1.0; 3]);

        let set = |world: &mut World, id, name: &str, value| {
            world.set_property(id, name, value).unwrap();
        };
        set(&mut world, clouds, "Cover", Value::Number(2.0));
        set(&mut world, clouds, "Density", Value::Number(0.25));
        set(
            &mut world,
            clouds,
            "Color",
            Value::Color3(Color3::new(1.0, 0.5, 0.0)),
        );
        let workspace = world.workspace();
        set(
            &mut world,
            workspace,
            "GlobalWind",
            Value::Vector3(Vector3::new(4.0, 0.0, -3.0)),
        );
        let drawn = clouds_in(&world);
        assert_eq!(drawn.cover, 1.0, "held to overcast");
        assert_eq!(drawn.density, 0.25);
        assert_eq!(drawn.color, [1.0, 0.5, 0.0]);
        assert_eq!(drawn.wind.to_array(), [4.0, 0.0, -3.0]);
        assert!(drawn.is_moving());

        set(&mut world, clouds, "Cover", Value::Number(0.0));
        assert!(!clouds_in(&world).is_drawn(), "no cover, no clouds");
        world.destroy(clouds).unwrap();
        assert_eq!(clouds_in(&world), Clouds::default());
    }
}
