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

#[test]
fn a_models_embedded_textures_are_cooked_in_place_when_it_wasnt_imported() {
    let folder = project("in-place");
    let model = "assets/models/DamagedHelmet.glb";
    std::fs::create_dir_all(folder.join("assets/models")).unwrap();
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-pbr/assets/models/DamagedHelmet/DamagedHelmet.glb");
    // Dropped into assets/ by hand: nothing copied out.
    fs::copy(&example, folder.join(model)).unwrap();
    let decoded = Assets::new(&folder).model_blocking(model).expect("loads");
    let map = |model: &axcenity_assets::Model| {
        model.primitives[0]
            .material
            .base_color_texture
            .clone()
            .expect("a color map")
    };
    assert!(map(&decoded).cooked().is_none());
    let pixels = map(&decoded).rgba().to_vec();
    drop(decoded);

    let stop = AtomicBool::new(false);
    let report = cook_project(&folder, &stop, |_, _| {});
    assert!(report.cooked >= 4, "{report:?}");
    assert!(report.failed.is_empty(), "{report:?}");
    let files = axcenity_assets::embedded_cooked_files(&folder, model);
    let copies: Vec<&String> = files
        .iter()
        .filter(|file| file.ends_with(".axtex"))
        .collect();
    assert_eq!(copies.len(), report.cooked, "{files:?}");
    assert_eq!(files.len(), copies.len() + 1, "and the model's note");
    // Noted as done: the next pass doesn't read the model again.
    let again = cook_project(&folder, &stop, |_, _| {});
    assert_eq!((again.cooked, again.failed.len()), (0, 0), "{again:?}");

    let cooked = Assets::new(&folder).model_blocking(model).expect("loads");
    let image = map(&cooked);
    assert!(image.cooked().is_some(), "from its cooked copy");
    assert!(
        image.rgba() == pixels,
        "decoded from inside the model if asked"
    );
    // Every map cooked, so the whole model is kept, and read back with
    // the same cooked maps.
    assert_eq!(axcenity_assets::cooked_models(&folder, model).len(), 1);
    let whole = Assets::new(&folder).model_blocking(model).expect("loads");
    assert!(map(&whole).cooked().is_some());
    assert_eq!(whole.primitives[0].indices, cooked.primitives[0].indices);
    drop((cooked, whole, image));

    // Replaced by a model with other images: the old copies go once no
    // model uses them.
    let other = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/primitives/assets/models/AWP.glb");
    fs::copy(other, folder.join(model)).unwrap();
    let replaced = cook_project(&folder, &stop, |_, _| {});
    assert_eq!(replaced.removed, copies.len(), "{replaced:?}");
    for copy in &copies {
        assert!(!folder.join(copy).exists(), "{copy}");
    }
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn models_embedding_the_same_image_share_one_cooked_copy() {
    let folder = project("shared-embedded");
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-pbr/assets/models/DamagedHelmet/DamagedHelmet.glb");
    fs::create_dir_all(folder.join("assets/a")).unwrap();
    fs::create_dir_all(folder.join("assets/b")).unwrap();
    let (first, second) = ("assets/a/Helmet.glb", "assets/b/Helmet.glb");
    fs::copy(&example, folder.join(first)).unwrap();
    fs::copy(&example, folder.join(second)).unwrap();

    let stop = AtomicBool::new(false);
    let report = cook_project(&folder, &stop, |_, _| {});
    assert!(report.failed.is_empty(), "{report:?}");
    let copies = |model: &str| -> Vec<String> {
        axcenity_assets::embedded_cooked_files(&folder, model)
            .into_iter()
            .filter(|file| file.ends_with(".axtex"))
            .collect()
    };
    // Cooked once, for both.
    assert_eq!(copies(first), copies(second));
    assert_eq!(report.cooked, copies(first).len(), "{report:?}");

    // Loaded together, they share each map, so it's uploaded once.
    let assets = Assets::new(&folder);
    let (a, b) = (
        assets.model_blocking(first).unwrap(),
        assets.model_blocking(second).unwrap(),
    );
    let color = |model: &axcenity_assets::Model| {
        model.primitives[0]
            .material
            .base_color_texture
            .clone()
            .unwrap()
    };
    assert!(color(&a).cooked().is_some());
    assert!(std::sync::Arc::ptr_eq(&color(&a), &color(&b)));
    drop((a, b));
    // Read back as whole cooked models, still one image.
    let assets = Assets::new(&folder);
    let (a, b) = (
        assets.model_blocking(first).unwrap(),
        assets.model_blocking(second).unwrap(),
    );
    assert_eq!(axcenity_assets::cooked_models(&folder, second).len(), 1);
    assert!(std::sync::Arc::ptr_eq(&color(&a), &color(&b)));
    drop((a, b));

    // With one model gone, the other keeps the copies; with both, they go.
    fs::remove_file(folder.join(first)).unwrap();
    let kept = cook_project(&folder, &stop, |_, _| {});
    assert!(
        kept.removed >= 1,
        "the first model's note and caches: {kept:?}"
    );
    assert!(
        copies(second)
            .iter()
            .all(|copy| folder.join(copy).is_file())
    );
    let shared = copies(second);
    fs::remove_file(folder.join(second)).unwrap();
    let gone = cook_project(&folder, &stop, |_, _| {});
    for copy in &shared {
        assert!(!folder.join(copy).exists(), "{copy}");
    }
    assert!(gone.removed > shared.len(), "{gone:?}");
    let _ = fs::remove_dir_all(folder);
}

/// Each map of each primitive: its slot, channel, whether it's cooked, and
/// what it holds.
fn maps_of(model: &axcenity_assets::Model) -> Vec<(String, bool, bool, Vec<u8>)> {
    let mut maps = Vec::new();
    for (index, primitive) in model.primitives.iter().enumerate() {
        let material = &primitive.material;
        let channel = |slot: &str, map: &Option<axcenity_assets::ChannelTexture>| {
            map.as_ref().map(|map| {
                (
                    format!("{slot} {:?} {}", map.channel, map.invert),
                    map.image.clone(),
                )
            })
        };
        let plain = |slot: &str, image: &Option<std::sync::Arc<axcenity_assets::Image>>| {
            image.as_ref().map(|image| (slot.to_owned(), image.clone()))
        };
        let all = [
            plain("color", &material.base_color_texture),
            plain("normal", &material.normal_texture),
            channel("metalness", &material.metalness_texture),
            channel("roughness", &material.roughness_texture),
            channel("occlusion", &material.occlusion_texture),
            plain("emissive", &material.emissive_texture),
            channel("opacity", &material.opacity_texture),
            plain("specular", &material.specular_texture),
        ];
        for (slot, image) in all.into_iter().flatten() {
            maps.push((
                format!("{index} {slot} {}x{}", image.width, image.height),
                image.cooked().is_some(),
                image.has_alpha(),
                image.rgba().to_vec(),
            ));
        }
    }
    maps
}

#[test]
fn a_model_with_embedded_maps_loads_the_same_cooked_as_uncooked() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for (example, name) in [
        (
            "hello-pbr/assets/models/DamagedHelmet/DamagedHelmet.glb",
            "Helmet.glb",
        ),
        ("primitives/assets/models/AWP.glb", "AWP.glb"),
        // Its image is a `data:` URI.
        ("hello-mesh/assets/models/pyramid.gltf", "pyramid.gltf"),
        // An FBX with its maps inside.
        (
            "hello-character/assets/models/Walking/Walking.fbx",
            "Walking.fbx",
        ),
    ] {
        let folder = project(&format!("parity-{name}"));
        let model = format!("assets/models/{name}");
        fs::create_dir_all(folder.join("assets/models")).unwrap();
        fs::copy(examples.join(example), folder.join(&model)).unwrap();
        let uncooked = Assets::new(&folder).model_blocking(&model).expect("loads");
        let before = maps_of(&uncooked);
        assert!(!before.is_empty(), "{name} has maps");
        drop(uncooked);

        let stop = AtomicBool::new(false);
        let report = cook_project(&folder, &stop, |_, _| {});
        assert!(report.failed.is_empty(), "{name}: {report:?}");
        let cooked = Assets::new(&folder).model_blocking(&model).expect("loads");
        let after = maps_of(&cooked);
        assert_eq!(after.len(), before.len(), "{name}: the same maps");
        for (old, new) in before.iter().zip(&after) {
            assert_eq!(old.0, new.0, "{name}: the same slots, channels and sizes");
            assert!(!old.1, "{name} {}: uncooked first", old.0);
            let (width, height) = old
                .0
                .rsplit(' ')
                .next()
                .and_then(|size| size.split_once('x'))
                .map(|(w, h)| (w.parse::<u32>().unwrap(), h.parse::<u32>().unwrap()))
                .unwrap();
            let whole_blocks = width % 4 == 0 && height % 4 == 0;
            assert_eq!(
                new.1, whole_blocks,
                "{name} {}: cooked when it can be",
                old.0
            );
            assert_eq!(old.2, new.2, "{name} {}: the same alpha", old.0);
            assert!(old.3 == new.3, "{name} {}: the same pixels", old.0);
        }
        assert_eq!(cooked.primitives.len(), {
            let again = Assets::new(&folder).model_blocking(&model).unwrap();
            again.primitives.len()
        });
        let _ = fs::remove_dir_all(folder);
    }
}

/// What a cooked-only game decodes for the CPU (G12) is close to the source:
/// BC7 within a few steps a channel, BC5's red and green likewise, with blue
/// worked out from them and alpha opaque.
#[test]
fn a_cooked_top_level_decodes_close_to_its_source() {
    let source = gradient(64, 32, 200);
    let cooked = cook_rgba(64, 32, &source).unwrap();
    let decoded = axcenity_assets::decode_top_level(&cooked).unwrap();
    assert_eq!(decoded.len(), source.len());
    let worst = source
        .iter()
        .zip(&decoded)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(worst <= 6, "BC7 differs by {worst}");

    // A flat normal pointing out of the surface.
    let up = [128, 128, 255, 255].repeat(16 * 16);
    let cooked = cook_normal_rgba(16, 16, &up).unwrap();
    let decoded = axcenity_assets::decode_top_level(&cooked).unwrap();
    for pixel in decoded.chunks(4) {
        assert!(pixel[0].abs_diff(128) <= 1 && pixel[1].abs_diff(128) <= 1);
        assert!(pixel[2] >= 253, "blue worked out as about 1: {pixel:?}");
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn import_presets_pick_the_format_the_size_or_no_cooking_and_a_change_cooks_again() {
    let folder = project("presets");
    fs::create_dir_all(folder.join("assets/images")).unwrap();
    let rgba = gradient(64, 32, 255);
    let rock = "assets/textures/stone/rock.png";
    let button = "assets/images/button.png";
    write_png(&folder.join(rock), 64, 32, &rgba);
    write_png(&folder.join(button), 64, 32, &rgba);
    // The images folder is kept exact; rock gets a size limit of its own.
    fs::write(
        folder.join(axcenity_assets::PRESETS_FILE),
        r#"{ "format_version": 1, "folders": { "assets/images": "UI and pixel art" } }"#,
    )
    .unwrap();
    axcenity_assets::give_asset_id(&folder, rock).unwrap();
    let sidecar = |preset: Option<&str>| {
        let bytes = axcenity_assets::sidecar_with_preset(&folder, rock, preset).unwrap();
        fs::write(folder.join(axcenity_assets::import_path(rock)), bytes).unwrap();
    };
    sidecar(Some("Small (1024)"));
    fs::write(
        folder.join(axcenity_assets::PRESETS_FILE),
        r#"{ "format_version": 1,
             "presets": { "Small (1024)": { "compression": "normal_map", "max_size": 16 } },
             "folders": { "assets/images": "UI and pixel art" } }"#,
    )
    .unwrap();

    let stop = AtomicBool::new(false);
    let report = cook_project(&folder, &stop, |_, _| {});
    assert_eq!((report.cooked, report.uncompressed), (1, 1), "{report:?}");
    assert!(read_cooked(&folder, button).is_none(), "kept uncompressed");
    let cooked = read_cooked(&folder, rock).expect("cooked");
    assert_eq!(cooked.format, CookedFormat::Bc5, "the project's own preset");
    assert_eq!((cooked.width, cooked.height), (16, 8), "halved to fit 16");
    assert_eq!(cooked.levels.len(), CookedTexture::level_count(16, 8));
    let again = cook_project(&folder, &stop, |_, _| {});
    assert_eq!((again.cooked, again.current, again.uncompressed), (0, 1, 1));

    // Loading uses the smaller copy, and its pixels match its size.
    let assets = Assets::new(&folder);
    let image = assets.texture_blocking(rock).expect("loads");
    assert_eq!((image.width, image.height), (16, 8));
    assert_eq!(image.rgba().len(), 16 * 8 * 4);
    let button_image = assets.texture_blocking(button).expect("loads");
    assert!(button_image.cooked().is_none());
    assert_eq!(button_image.rgba(), &rgba[..], "every pixel exact");

    // Picking Default instead (or undoing the pick) brings it back in line.
    sidecar(None);
    assert_eq!(refresh_texture(&folder, rock), Some(Outcome::Cooked));
    let cooked = read_cooked(&folder, rock).expect("cooked");
    assert_eq!(
        (cooked.format, cooked.width, cooked.height),
        (CookedFormat::Bc7, 64, 32)
    );
    assert_eq!(refresh_texture(&folder, rock), None, "already in line");
    sidecar(Some("UI and pixel art"));
    assert_eq!(refresh_texture(&folder, rock), Some(Outcome::Uncompressed));
    assert!(!axcenity_assets::cooked_path(&folder, rock).exists());
    let _ = fs::remove_dir_all(folder);
}
