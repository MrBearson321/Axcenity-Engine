//! The editor's debug views: each shows what it names, and the lit view
//! comes back unchanged with nothing kept for them.

use super::level_of_detail::Detailed;
use super::*;
use crate::DebugView;

/// A block filling most of a 64 × 64 view, its front face (facing +Z,
/// toward the camera) in the middle.
fn front_face(material: Material) -> DrawList {
    let mut list = list_of(vec![DrawItem {
        material,
        ..big_block()
    }]);
    list.ambient.intensity = 0.5;
    list
}

fn shown(renderer: &mut TextureRenderer, list: &DrawList, view: DebugView) -> Vec<u8> {
    renderer.set_debug_view(Some(view));
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 3] {
    pixels[(y * 64 + x) * 4..][..3].try_into().unwrap()
}

fn assert_near(seen: [u8; 3], expected: [u8; 3], what: &str) {
    for (seen_channel, expected_channel) in seen.iter().zip(expected) {
        assert!(
            seen_channel.abs_diff(expected_channel) <= 2,
            "{what} came back as {seen:?}, not {expected:?}"
        );
    }
}

#[test]
fn each_material_view_shows_its_own_value() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let list = front_face(Material {
        base_color: [0.5, 0.25, 0.75],
        metalness: 0.3,
        roughness: 0.6,
        ..Material::default()
    });
    let mut middle = |view| pixel(&shown(&mut renderer, &list, view), 20, 32);
    assert_near(middle(DebugView::BaseColor), [128, 64, 191], "base color");
    assert_near(middle(DebugView::Unlit), [128, 64, 191], "unlit");
    // Facing +Z: x and y at 0, z at 1.
    assert_near(middle(DebugView::Normals), [128, 128, 255], "the normal");
    assert_near(middle(DebugView::Metalness), [77; 3], "metalness");
    assert_near(middle(DebugView::Roughness), [153; 3], "roughness");
    // Nothing near the open face hides it.
    let open = middle(DebugView::AmbientOcclusion);
    assert!(open.iter().all(|&channel| channel > 240), "{open:?}");
}

#[test]
fn unlit_adds_a_surfaces_glow_to_its_color() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let list = front_face(Material {
        base_color: [0.2; 3],
        emissive: [0.8, 0.0, 0.0],
        ..Material::default()
    });
    let unlit = pixel(&shown(&mut renderer, &list, DebugView::Unlit), 20, 32);
    let base = pixel(&shown(&mut renderer, &list, DebugView::BaseColor), 20, 32);
    assert!(unlit[0] > base[0] + 50, "unlit {unlit:?}, base {base:?}");
    assert_near(
        [unlit[1], unlit[2], 0],
        [base[1], base[2], 0],
        "unlit green and blue",
    );
}

#[test]
fn the_wireframe_draws_each_triangles_edges_and_leaves_its_inside_dark() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let list = front_face(Material::default());
    let pixels = shown(&mut renderer, &list, DebugView::Wireframe);
    // The face's two triangles meet along one of its diagonals, through
    // the middle; a point off both diagonals is inside one of them.
    let bright = |at: [u8; 3]| at.iter().all(|&channel| channel > 150);
    assert!(
        bright(pixel(&pixels, 32, 32)),
        "{:?}",
        pixel(&pixels, 32, 32)
    );
    let inside = pixel(&pixels, 32, 22);
    assert!(inside.iter().all(|&channel| channel < 60), "{inside:?}");
    let face: Vec<[u8; 3]> = (8..56)
        .flat_map(|y| (8..56).map(move |x| (x, y)))
        .map(|(x, y)| pixel(&pixels, x, y))
        .collect();
    let lines = face.iter().filter(|&&at| bright(at)).count();
    assert!(
        lines > 0 && lines < face.len() / 5,
        "{lines} of {} pixels are lines",
        face.len()
    );
}

#[test]
fn overdraw_is_brighter_where_more_surfaces_are_drawn() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    // A small block in front of the big one: two faces over the middle,
    // one beside it, none in the corner past the big block.
    let mut list = front_face(Material::default());
    list.items
        .push(item(Shape::Block, Vec3::new(0.0, 0.0, -2.0), 1.0));
    let pixels = shown(&mut renderer, &list, DebugView::Overdraw);
    let [twice, once, none] = [
        pixel(&pixels, 32, 32),
        pixel(&pixels, 12, 32),
        pixel(&pixels, 0, 0),
    ];
    assert!(
        twice[0] > once[0] && once[0] > none[0],
        "twice {twice:?}, once {once:?}, none {none:?}"
    );
    assert_eq!(none, [0; 3]);
}

#[test]
fn level_colors_show_an_items_own_mesh_green_and_no_levels_grey() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let mut list = front_face(Material::default());
    let plain = pixel(&shown(&mut renderer, &list, DebugView::LodLevels), 32, 32);
    assert!(
        plain[0].abs_diff(plain[1]) <= 2 && plain[1].abs_diff(plain[2]) <= 2,
        "{plain:?}"
    );
    list.items[0].shape = Shape::Mesh(Arc::new(Detailed { level: 0 }));
    let detailed = pixel(&shown(&mut renderer, &list, DebugView::LodLevels), 32, 32);
    assert!(
        detailed[1] > detailed[0] + 60 && detailed[1] > detailed[2] + 60,
        "{detailed:?}"
    );
}

#[test]
fn switching_back_to_lit_restores_the_frame_and_keeps_nothing() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let list = front_face(Material {
        base_color: [0.6, 0.4, 0.2],
        ..Material::default()
    });
    renderer.render(&list).unwrap();
    let lit = renderer.read_pixels();
    assert!(!renderer.debug_view_alive());
    for view in DebugView::ALL {
        let debug = shown(&mut renderer, &list, view);
        assert!(renderer.debug_view_alive());
        assert_ne!(debug, lit, "{view:?} looks like the lit frame");
    }
    renderer.set_debug_view(None);
    assert!(!renderer.debug_view_alive());
    renderer.render(&list).unwrap();
    let back = renderer.read_pixels();
    let most = lit
        .iter()
        .zip(&back)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(most <= 1, "the lit frame changed by up to {most}");
}
