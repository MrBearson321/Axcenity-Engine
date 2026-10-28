//! Parts combined into a UnionOperation (Phase 50). The union keeps the
//! parts it was made of inside it, for Separate, but they're out of the
//! world: not drawn, not colliding and not hit by queries. The union's own
//! model stands for them.

use crate::{ClassName, InstanceId, World};

/// True when `id` is inside a UnionOperation (not the union itself), so
/// it's one of the parts kept for Separate.
pub fn in_union(world: &World, id: InstanceId) -> bool {
    let mut at = world.get(id).and_then(|instance| instance.parent());
    while let Some(current) = at {
        let Some(instance) = world.get(current) else {
            return false;
        };
        if instance.class() == ClassName::UnionOperation {
            return true;
        }
        at = instance.parent();
    }
    false
}

/// True when `id` is out of the world: in a put-away Tool (Phase 45) or
/// kept inside a UnionOperation.
pub fn out_of_world(world: &World, id: InstanceId) -> bool {
    in_union(world, id) || crate::tool::is_stored(world, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_inside_a_union_are_out_of_the_world() {
        let mut world = World::new();
        let union = world.create(ClassName::UnionOperation).unwrap();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(union)).unwrap();
        world.set_parent(union, Some(world.workspace())).unwrap();
        assert!(in_union(&world, part));
        assert!(out_of_world(&world, part));
        assert!(!in_union(&world, union));
    }
}
