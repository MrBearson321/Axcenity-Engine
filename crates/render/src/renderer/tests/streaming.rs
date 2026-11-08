//! World streaming's background uploads (Phase 80): meshes and textures
//! come in on their own thread, and what stops being drawn leaves the GPU.

use super::*;
use glam::Mat4;

struct Tile {
    key: u64,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl MeshSource for Tile {
    fn key(&self) -> u64 {
        self.key
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }
    fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &[]
    }
    fn indices(&self) -> &[u32] {
        &self.indices
    }
}

/// A 4 x 4 image.
struct Picture(u64, Vec<u8>);

impl TextureSource for Picture {
    fn key(&self) -> u64 {
        self.0
    }
    fn size(&self) -> (u32, u32) {
        (4, 4)
    }
    fn rgba(&self) -> &[u8] {
        &self.1
    }
}

fn mesh_item(key: u64) -> DrawItem {
    let tile = Tile {
        key,
        positions: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
    };
    let mut item = big_block();
    item.shape = Shape::Mesh(Arc::new(tile));
    item.transform = Mat4::from_translation(Vec3::new(0.0, 0.0, -6.0));
    item
}

/// Drawing `list` every frame until nothing is on its way.
fn until_uploaded(renderer: &mut TextureRenderer, list: &DrawList) {
    for _ in 0..500 {
        renderer.render(list).unwrap();
        if renderer.pending_uploads() == 0 {
            renderer.render(list).unwrap();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the uploads never finished");
}

#[test]
fn a_mesh_is_uploaded_in_the_background_and_drawn_a_few_frames_later() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_background_uploads(true);
    let list = list_of(vec![mesh_item(901)]);
    renderer.render(&list).unwrap();
    assert!(
        renderer.scene.gpu_mesh(MeshKey::Custom(901)).is_none(),
        "the frame that first draws it doesn't wait for it"
    );
    assert_eq!(renderer.pending_uploads(), 1);
    until_uploaded(&mut renderer, &list);
    assert!(renderer.scene.gpu_mesh(MeshKey::Custom(901)).is_some());
}

#[test]
fn what_stops_being_drawn_leaves_the_gpu_and_uploads_again_on_the_way_back() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_background_uploads(true);
    let shown = list_of(vec![mesh_item(902)]);
    until_uploaded(&mut renderer, &shown);
    let bytes = renderer.scene.mesh_bytes.total();
    assert!(bytes > 0);

    // A long time passes without it being drawn.
    let empty = list_of(Vec::new());
    renderer.scene.frame += KEEP_UNDRAWN_FRAMES;
    renderer.render(&empty).unwrap();
    assert!(
        renderer.scene.gpu_mesh(MeshKey::Custom(902)).is_none(),
        "unloaded once it's been out of range a while"
    );

    until_uploaded(&mut renderer, &shown);
    assert!(
        renderer.scene.gpu_mesh(MeshKey::Custom(902)).is_some(),
        "loaded again when it comes back"
    );
}

#[test]
fn a_texture_comes_in_on_the_thread_and_replaces_the_white_it_drew_with() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    renderer.set_background_uploads(true);
    let pixels = [200u8, 30, 30, 255].repeat(16);
    let item = textured_block(Arc::new(Picture(903, pixels)));
    let list = list_of(vec![item]);
    renderer.render(&list).unwrap();
    assert!(
        !renderer.scene.textures.contains_key(&903),
        "not on the GPU yet: it draws white"
    );
    until_uploaded(&mut renderer, &list);
    assert!(renderer.scene.textures.contains_key(&903));
}
