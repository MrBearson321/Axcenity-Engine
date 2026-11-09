//! Skinning on the GPU: a skin moves its mesh's corners, items posed
//! differently share one mesh and batch, and culling uses the posed box.

use super::*;
use crate::Skin;

/// A unit square facing +Z, 30 studs to the right of where it's drawn
/// until its skin moves it: every corner is weighted to one joint.
pub(super) struct SkinnedSquare {
    joints: [[u16; 4]; 4],
}

impl MeshSource for SkinnedSquare {
    fn key(&self) -> u64 {
        91 + u64::from(self.joints[0][0])
    }
    fn positions(&self) -> &[[f32; 3]] {
        &[
            [29.5, -0.5, 0.0],
            [30.5, -0.5, 0.0],
            [30.5, 0.5, 0.0],
            [29.5, 0.5, 0.0],
        ]
    }
    fn normals(&self) -> &[[f32; 3]] {
        &[[0.0, 0.0, 1.0]; 4]
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &[]
    }
    fn indices(&self) -> &[u32] {
        &[0, 1, 2, 0, 2, 3]
    }
    fn skin_joints(&self) -> &[[u16; 4]] {
        &self.joints
    }
    fn skin_weights(&self) -> &[[f32; 4]] {
        &[[1.0, 0.0, 0.0, 0.0]; 4]
    }
}

/// A skin of `count` joints, the last moving its corners by `offset`.
pub(super) fn skin(count: usize, offset: Vec3) -> Arc<Skin> {
    let mut joints = vec![Mat4::IDENTITY; count];
    joints[count - 1] = Mat4::from_translation(offset);
    Arc::new(Skin::new(joints))
}

/// The square, glowing white so it shows however it's lit.
pub(super) fn square(joint: u16, skin: Option<Arc<Skin>>) -> DrawItem {
    let mut item = item(
        Shape::Mesh(Arc::new(SkinnedSquare {
            joints: [[joint, 0, 0, 0]; 4],
        })),
        Vec3::new(0.0, 0.0, -3.0),
        1.0,
    );
    item.material.emissive = [1.0; 3];
    DrawItem { skin, ..item }
}

#[test]
fn a_skin_moves_its_mesh_into_view_and_is_not_culled_where_it_rested() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    let middle = |renderer: &mut TextureRenderer, list: &DrawList| {
        renderer.render(list).unwrap();
        let pixels = renderer.read_pixels();
        let at = (16 * 32 + 16) * 4;
        [pixels[at], pixels[at + 1], pixels[at + 2]]
    };
    // Unposed, the square is off to the side: culled, and the sky shows.
    let sky = middle(&mut renderer, &list_of(vec![square(0, None)]));
    assert_eq!(renderer.stats().items_culled, 1);

    // Its one joint moves it back to the middle of the view.
    let posed = list_of(vec![square(0, Some(skin(1, Vec3::new(-30.0, 0.0, 0.0))))]);
    let drawn = middle(&mut renderer, &posed);
    let stats = renderer.stats();
    assert_eq!((stats.items_drawn, stats.items_culled), (1, 0));
    assert!(
        drawn.iter().zip(sky).any(|(a, b)| a.abs_diff(b) > 30),
        "the posed square isn't drawn: {drawn:?} vs sky {sky:?}"
    );

    // A skin with more joints than the frame's joints first had room for.
    let many = list_of(vec![square(
        99,
        Some(skin(100, Vec3::new(-30.0, 0.0, 0.0))),
    )]);
    assert_eq!(middle(&mut renderer, &many), drawn);
}

#[test]
fn items_posed_differently_share_one_mesh_and_batch() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    let list = list_of(vec![
        square(0, Some(skin(1, Vec3::new(-30.6, 0.0, 0.0)))),
        square(0, Some(skin(1, Vec3::new(-29.4, 0.0, 0.0)))),
    ]);
    renderer.render(&list).unwrap();
    let stats = renderer.stats();
    assert_eq!((stats.items_drawn, stats.batches), (2, 1));
    // Two skins of one joint each.
    let plan = plan_with(&list, |_| Bounds::UNIT);
    assert_eq!(plan.joints.len(), 2);
    let skins: Vec<[u32; 2]> = plan
        .instances
        .iter()
        .flatten()
        .map(|raw| raw.skin)
        .collect();
    assert_eq!(skins, [[0, 1], [1, 1]]);
}

#[test]
fn a_skins_box_holds_each_corner_wherever_its_joints_move_it() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    let posed = skin(1, Vec3::new(-30.0, 2.0, 0.0));
    renderer
        .render(&list_of(vec![square(0, Some(posed.clone()))]))
        .unwrap();
    let key = MeshKey::Custom(91);
    assert_eq!(
        renderer.scene.mesh_bounds(key),
        Bounds {
            min: Vec3::new(29.5, -0.5, 0.0),
            max: Vec3::new(30.5, 0.5, 0.0),
        }
    );
    assert_eq!(
        renderer.scene.item_bounds(key, Some(&posed)),
        Bounds {
            min: Vec3::new(-0.5, 1.5, 0.0),
            max: Vec3::new(0.5, 2.5, 0.0),
        }
    );
}
