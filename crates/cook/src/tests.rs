use std::fs;
use std::sync::atomic::AtomicBool;

use axcenity_assets::{Assets, decode_cooked, encode_cooked, read_cooked};

use super::*;

/// A `width`×`height` image: red rising left to right, green top to bottom.
fn gradient(width: u32, height: u32, alpha: u8) -> Vec<u8> {
    (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                [
                    (x * 255 / width.max(2).saturating_sub(1).max(1)) as u8,
                    (y * 255 / height.max(2).saturating_sub(1).max(1)) as u8,
                    64,
                    alpha,
                ]
            })
        })
        .collect()
}

fn project(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-cook-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(folder.join("assets/textures/stone")).unwrap();
    folder
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    let file = fs::File::create(path).unwrap();
    let mut encoder = png_encoder(file, width, height);
    encoder.write_image_data(rgba).unwrap();
}

fn png_encoder(file: fs::File, width: u32, height: u32) -> png::Writer<fs::File> {
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap()
}

#[test]
fn a_texture_cooks_into_bc7_levels_down_to_one_pixel() {
    let cooked = cook_rgba(64, 16, &gradient(64, 16, 255)).expect("whole blocks");
    assert_eq!(cooked.format, CookedFormat::Bc7);
    assert_eq!(cooked.levels.len(), 7, "64, 32, 16, 8, 4, 2 and 1 wide");
    for (level, bytes) in cooked.levels.iter().enumerate() {
        let (width, height) = cooked.level_size(level);
        assert_eq!(bytes.len(), CookedFormat::Bc7.level_bytes(width, height));
    }
    assert!(!cooked.has_alpha);
    let stamp = SourceStamp {
        len: 1,
        modified: 2,
        hash: 3,
    };
    let (_, read) = decode_cooked(&encode_cooked(stamp, &cooked)).unwrap();
    assert_eq!(read.levels, cooked.levels);
    assert!(cook_rgba(64, 16, &gradient(64, 16, 100)).unwrap().has_alpha);
}

#[test]
fn a_texture_that_isnt_whole_blocks_stays_rgba() {
    assert!(cook_rgba(30, 16, &gradient(30, 16, 255)).is_none());
    assert!(cook_rgba(16, 2, &gradient(16, 2, 255)).is_none());
}

#[test]
fn halving_matches_the_renderers_box_filter() {
    // 3×1: the odd last column is averaged with itself.
    let rgba = [0, 0, 0, 255, 100, 0, 0, 255, 201, 0, 0, 255];
    let (width, height, half) = half_size(3, 1, &rgba);
    assert_eq!((width, height), (1, 1));
    assert_eq!(half, [50, 0, 0, 255]);
}

#[test]
fn cooking_a_project_writes_copies_that_assets_then_loads() {
    let folder = project("project");
    let rgba = gradient(32, 32, 255);
    write_png(
        &folder.join("assets/textures/stone/Color.png"),
        32,
        32,
        &rgba,
    );
    write_png(
        &folder.join("assets/textures/odd.png"),
        30,
        30,
        &gradient(30, 30, 255),
    );
    fs::write(folder.join("assets/textures/broken.png"), b"not a png").unwrap();
    fs::write(folder.join("assets/notes.txt"), b"not a texture").unwrap();

    let stop = AtomicBool::new(false);
    let report = cook_project(&folder, &stop, |_, _| {});
    assert_eq!(report.cooked, 1, "{report:?}");
    assert_eq!(report.skipped, 1, "odd.png isn't whole blocks");
    assert_eq!(report.failed.len(), 1, "broken.png");
    let again = cook_project(&folder, &stop, |_, _| {});
    assert_eq!((again.cooked, again.current), (0, 1), "only once");

    let path = "assets/textures/stone/Color.png";
    assert!(read_cooked(&folder, path).is_some());
    let assets = Assets::new(&folder);
    let image = assets.texture_blocking(path).expect("loads");
    assert!(image.cooked().is_some(), "from the cooked copy");
    assert_eq!(image.file.as_deref(), Some(path));
    // The pixels still come from the file, when something asks.
    assert_eq!(image.rgba(), &rgba[..]);
    assert!(!image.has_alpha());

    // Edited: the cooked copy is out of date, so the file is decoded.
    let red = [255, 0, 0, 255].repeat(32 * 32);
    write_png(&folder.join(path), 32, 32, &red);
    let assets = Assets::new(&folder);
    let image = assets.texture_blocking(path).expect("loads");
    assert!(image.cooked().is_none());
    assert_eq!(image.rgba(), &red[..]);

    let stopped = AtomicBool::new(true);
    assert_eq!(cook_project(&folder, &stopped, |_, _| {}).cooked, 0);

    // Deleted: its cooked copy goes at the next cook.
    fs::remove_file(folder.join(path)).unwrap();
    let gone = cook_project(&folder, &stop, |_, _| {});
    assert_eq!(gone.removed, 1, "{gone:?}");
    assert!(!axcenity_assets::cooked_path(&folder, path).exists());
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn a_models_embedded_textures_load_from_their_cooked_copies() {
    let folder = project("embedded");
    let helmet = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-pbr/assets/models/DamagedHelmet/DamagedHelmet.glb");
    // Importing copies the .glb's embedded images out as files.
    let (mesh_id, imported) = axcenity_assets::import_model(&folder, &helmet).unwrap();
    let stop = AtomicBool::new(false);
    let report = cook_project(&folder, &stop, |_, _| {});
    assert!(report.cooked >= 4, "{report:?}");
    assert!(report.failed.is_empty(), "{report:?}");

    let decoded = imported.primitives[0]
        .material
        .base_color_texture
        .clone()
        .unwrap();
    assert!(decoded.cooked().is_none());
    let (file, alpha, pixels) = (
        decoded.file.clone(),
        decoded.has_alpha(),
        decoded.rgba().to_vec(),
    );
    // Loaded images embedded in a model are shared while they're alive.
    drop((imported, decoded));

    let assets = Assets::new(&folder);
    let model = assets.model_blocking(&mesh_id).expect("loads");
    let cooked = model.primitives[0]
        .material
        .base_color_texture
        .as_ref()
        .expect("a color map");
    assert!(cooked.cooked().is_some(), "from the cooked copy");
    assert_eq!(cooked.file, file);
    // Without decoding the cooked one, it still tells it has no alpha.
    assert_eq!(cooked.has_alpha(), alpha);
    assert!(cooked.rgba() == pixels, "decoded from the copied-out file");

    // With every map cooked, the whole model is kept too, and read back
    // with the same cooked maps.
    assert_eq!(axcenity_assets::cooked_models(&folder, &mesh_id).len(), 1);
    let again = Assets::new(&folder)
        .model_blocking(&mesh_id)
        .expect("loads");
    let map = again.primitives[0]
        .material
        .base_color_texture
        .as_ref()
        .expect("a color map");
    assert!(map.cooked().is_some());
    assert_eq!(map.file, file);
    assert_eq!(again.primitives[0].indices, model.primitives[0].indices);
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn a_normal_map_by_its_name_cooks_into_bc5_of_its_red_and_green() {
    assert_eq!(format_for("assets/rock/Rock_Normal.png"), CookedFormat::Bc5);
    assert_eq!(format_for("assets/T_Bench_N.png"), CookedFormat::Bc5);
    assert_eq!(
        format_for("assets/rock/Rock_BaseColor.png"),
        CookedFormat::Bc7
    );
    assert_eq!(format_for("assets/rock/rock_ORM.png"), CookedFormat::Bc7);
    assert_eq!(format_for("assets/sign.png"), CookedFormat::Bc7);

    let cooked = cook_normal_rgba(16, 8, &gradient(16, 8, 255)).expect("whole blocks");
    assert_eq!(cooked.format, CookedFormat::Bc5);
    assert_eq!(cooked.levels.len(), 5);
    for (level, bytes) in cooked.levels.iter().enumerate() {
        let (width, height) = cooked.level_size(level);
        assert_eq!(bytes.len(), CookedFormat::Bc5.level_bytes(width, height));
    }
    assert!(!cooked.has_alpha, "BC5 keeps no alpha");
    // A flat block: each of red's and green's BC4 halves starts with its
    // two ends, here the same value.
    let flat = cook_normal_rgba(4, 4, &[100, 200, 255, 255].repeat(16)).unwrap();
    let block = &flat.levels[0];
    assert_eq!((block[0], block[1]), (100, 100));
    assert_eq!((block[8], block[9]), (200, 200));
}

#[test]
fn cooking_a_project_cooks_normal_maps_as_bc5_and_redoes_older_bc7_copies() {
    let folder = project("normals");
    let path = "assets/textures/stone/Stone_Normal.png";
    write_png(&folder.join(path), 16, 16, &gradient(16, 16, 255));
    // Cooked as BC7 by an older editor.
    let bytes = fs::read(folder.join(path)).unwrap();
    let stamp = SourceStamp::of(&folder.join(path), &bytes);
    let old = cook_rgba(16, 16, &gradient(16, 16, 255)).unwrap();
    axcenity_assets::write_cooked(&folder, path, stamp, &old).unwrap();
    assert_eq!(
        axcenity_assets::cooked_format(&folder, path),
        Some(CookedFormat::Bc7)
    );

    let stop = AtomicBool::new(false);
    let report = cook_project(&folder, &stop, |_, _| {});
    assert_eq!((report.cooked, report.current), (1, 0), "{report:?}");
    assert_eq!(
        axcenity_assets::cooked_format(&folder, path),
        Some(CookedFormat::Bc5)
    );
    let again = cook_project(&folder, &stop, |_, _| {});
    assert_eq!((again.cooked, again.current), (0, 1));
    let _ = fs::remove_dir_all(folder);
}
