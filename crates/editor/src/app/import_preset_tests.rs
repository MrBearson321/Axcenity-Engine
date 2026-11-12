//! Import presets picked in the Assets panel (R12): one undo step that
//! cooks the texture again as the preset wants, and undo cooks it back.

use axcenity_assets::{CookedFormat, read_cooked, read_preset};

use super::EditorApp;
use super::tests::copy_of_example;

const ARROW: &str = "assets/textures/arrow.png";

#[test]
fn picking_a_preset_cooks_the_texture_that_way_and_undo_puts_it_back() {
    let folder = copy_of_example("hello-decals", "import-preset");
    let mut app = EditorApp::new(Some(&folder));
    // Cooked as it is when the project opened (the editor's own cooker
    // isn't running in tests).
    assert_eq!(
        axcenity_cook::refresh_texture(&folder, ARROW),
        Some(axcenity_cook::Outcome::Cooked)
    );
    let full = read_cooked(&folder, ARROW).unwrap();
    assert_eq!((full.format, full.width), (CookedFormat::Bc7, 128));

    app.set_import_preset(ARROW, Some("UI and pixel art"));
    assert_eq!(
        read_preset(&folder, ARROW).as_deref(),
        Some("UI and pixel art")
    );
    assert!(read_cooked(&folder, ARROW).is_none(), "kept uncompressed");
    let image = app.assets.texture_blocking(ARROW).unwrap();
    assert!(image.cooked().is_none(), "drawn from its exact pixels");

    app.undo();
    assert_eq!(read_preset(&folder, ARROW), None);
    let back = read_cooked(&folder, ARROW).expect("cooked again");
    assert_eq!((back.format, back.width), (CookedFormat::Bc7, 128));
    assert!(
        app.assets
            .texture_blocking(ARROW)
            .unwrap()
            .cooked()
            .is_some()
    );

    app.redo();
    assert!(read_cooked(&folder, ARROW).is_none());
    let _ = std::fs::remove_dir_all(&folder);
}
