//! The viewport's View modes (View > View Mode): the lit scene, one of the
//! renderer's debug views, or the lit scene with the culling overlay's
//! bounds. Editor only: the player is built without the renderer's
//! `debug-views` feature, so it never has them. Not saved between runs.

use axcenity_render::DebugView;

/// What the viewport shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewMode {
    /// The scene as the game draws it.
    #[default]
    Lit,
    /// One of the renderer's debug views in the scene's place.
    Debug(DebugView),
    /// The lit scene, with culling held where the camera is when it's
    /// picked and each item's bounds outlined, as View > Overlays > Culling
    /// does.
    CullingBounds,
}

impl ViewMode {
    /// Every mode, in menu order.
    pub fn all() -> impl Iterator<Item = ViewMode> {
        std::iter::once(ViewMode::Lit)
            .chain(DebugView::ALL.map(ViewMode::Debug))
            .chain([ViewMode::CullingBounds])
    }

    /// The debug view the renderer draws, or `None` for the lit scene.
    pub fn debug_view(self) -> Option<DebugView> {
        match self {
            ViewMode::Debug(view) => Some(view),
            ViewMode::Lit | ViewMode::CullingBounds => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Lit => "Lit",
            ViewMode::Debug(view) => match view {
                DebugView::Unlit => "Unlit",
                DebugView::Wireframe => "Wireframe",
                DebugView::Normals => "Normals",
                DebugView::BaseColor => "Base Color",
                DebugView::Metalness => "Metalness",
                DebugView::Roughness => "Roughness",
                DebugView::AmbientOcclusion => "Ambient Occlusion",
                DebugView::Overdraw => "Overdraw",
                DebugView::LodLevels => "LOD Levels",
            },
            ViewMode::CullingBounds => "Culling Bounds",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            ViewMode::Lit => "The scene as the game draws it.",
            ViewMode::Debug(view) => match view {
                DebugView::Unlit => "Each surface's color and glow, with no light or shadow.",
                DebugView::Wireframe => "Each triangle's edges.",
                DebugView::Normals => {
                    "The way each surface faces, normal maps included: X red, Y green, Z blue."
                }
                DebugView::BaseColor => "Each surface's base color, its color map included.",
                DebugView::Metalness => "Metalness as grey: black is 0, white is 1.",
                DebugView::Roughness => "Roughness as grey: black is 0, white is 1.",
                DebugView::AmbientOcclusion => {
                    "How much surrounding light reaches each surface, from occlusion maps and \
                     the Ambient Occlusion graphics setting: white is wholly open."
                }
                DebugView::Overdraw => {
                    "How many surfaces are drawn over each pixel, hidden ones included: \
                     brighter is more."
                }
                DebugView::LodLevels => {
                    "Each model's level of detail: green is its own mesh, then yellow, orange \
                     and red for simpler copies. Grey has none."
                }
            },
            ViewMode::CullingBounds => {
                "Holds what's drawn to the current view, and outlines what's drawn (green) and \
                 culled (red)."
            }
        }
    }

    /// Whether culling is held and outlined, as by the culling overlay.
    pub fn holds_culling(self) -> bool {
        self == ViewMode::CullingBounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_is_listed_once_with_its_own_name() {
        let modes: Vec<ViewMode> = ViewMode::all().collect();
        assert_eq!(modes.len(), 2 + DebugView::ALL.len());
        let mut labels: Vec<&str> = modes.iter().map(|mode| mode.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), modes.len());
        assert_eq!(modes[0], ViewMode::Lit);
        assert_eq!(ViewMode::default(), ViewMode::Lit);
    }

    #[test]
    fn only_debug_modes_change_what_the_renderer_draws() {
        assert_eq!(ViewMode::Lit.debug_view(), None);
        assert_eq!(ViewMode::CullingBounds.debug_view(), None);
        assert!(ViewMode::CullingBounds.holds_culling());
        for view in DebugView::ALL {
            assert_eq!(ViewMode::Debug(view).debug_view(), Some(view));
            assert!(!ViewMode::Debug(view).holds_culling());
        }
    }

    /// hello-graphics (the graphics reference scene) from the player's
    /// starting camera, in every mode, then lit again. With
    /// `AXCENITY_SNAPSHOTS` naming a folder, each mode's frame is saved
    /// there to look at.
    #[test]
    fn each_mode_draws_hello_graphics_its_own_way_and_lit_comes_back() {
        use axcenity_render::TextureRenderer;
        use axcenity_render::glam::Vec3;
        let instance = axcenity_render::adapters::instance();
        let Some((device, queue)) =
            pollster::block_on(axcenity_render::adapters::adapter(&instance, None))
                .ok()
                .and_then(|adapter| {
                    pollster::block_on(adapter.request_device(&Default::default())).ok()
                })
        else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let folder =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-graphics");
        let project = axcenity_project::load(&folder).unwrap();
        let assets = axcenity_assets::Assets::new(&folder);
        let camera = axcenity_scene::FlyCamera::looking_at(Vec3::new(12.0, 8.0, 16.0), Vec3::ZERO);
        let list =
            axcenity_scene::build_draw_list(&project.world, &assets, camera.to_render_camera());
        let (width, height) = (480, 270);
        let mut renderer = TextureRenderer::new(&device, &queue, width, height);
        // Models load the first time they're drawn, so each is drawn twice.
        let mut draw = |mode: ViewMode| {
            renderer.set_debug_view(mode.debug_view());
            renderer.render(&list).unwrap();
            renderer.render(&list).unwrap();
            renderer.read_pixels()
        };
        let lit = draw(ViewMode::Lit);
        let changed = |a: &[u8], b: &[u8]| {
            a.chunks(4)
                .zip(b.chunks(4))
                .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(a, b)| a.abs_diff(*b) > 8))
                .count()
        };
        let pixels = (width * height) as usize;
        let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from);
        let mut drawn: Vec<(ViewMode, Vec<u8>)> = Vec::new();
        for mode in ViewMode::all().filter(|mode| mode.debug_view().is_some()) {
            let frame = draw(mode);
            if let Some(folder) = &snapshots {
                let name = format!("view-mode-{}.png", mode.label().replace(' ', "-"));
                crate::snapshot::save_png(&folder.join(name), (width, height), &frame);
            }
            assert!(
                changed(&frame, &lit) > pixels / 10,
                "{} looks like the lit frame",
                mode.label()
            );
            for (other, seen) in &drawn {
                assert!(
                    changed(&frame, seen) > pixels / 100,
                    "{} looks like {}",
                    mode.label(),
                    other.label()
                );
            }
            drawn.push((mode, frame));
        }
        let back = draw(ViewMode::Lit);
        assert_eq!(changed(&back, &lit), 0, "the lit frame came back changed");
    }
}
