use super::*;
use crate::check_assets;
use axcenity_anim::{Motion, StateDef};
use axcenity_core::ClassName;

fn scratch(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-asset-fix-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join(crate::MANIFEST_FILE),
        format!(
            r#"{{"name": "T", "format_version": {}}}"#,
            crate::FORMAT_VERSION
        ),
    )
    .unwrap();
    fs::write(folder.join(crate::WORLD_FILE), r#"{"format_version": 1}"#).unwrap();
    folder
}

fn write(folder: &Path, path: &str, bytes: &[u8]) {
    let disk = folder.join(path);
    fs::create_dir_all(disk.parent().unwrap()).unwrap();
    fs::write(disk, bytes).unwrap();
}

fn add(world: &mut World, class: ClassName, name: &str, property: &str, value: &str) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    world
        .set_property(id, property, Value::String(value.into()))
        .unwrap();
    id
}

fn text(write: &FileWrite) -> String {
    String::from_utf8(write.after.clone().unwrap()).unwrap()
}

#[test]
fn a_moved_model_is_suggested_and_one_fix_points_every_part_at_it() {
    let folder = scratch("moved");
    write(&folder, "assets/models/props/chair.glb", b"moved here");
    write(&folder, "assets/other/chair.glb", b"not this one");
    let mut world = World::new();
    let first = add(
        &mut world,
        ClassName::MeshPart,
        "A",
        "MeshId",
        "assets/models/chair.glb#Seat",
    );
    let second = add(
        &mut world,
        ClassName::MeshPart,
        "B",
        "MeshId",
        "assets/models/chair.glb#Seat",
    );

    let report = check_assets(&folder, &world);
    let missing: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|finding| finding.kind == FindingKind::MissingFile)
        .collect();
    assert_eq!(missing.len(), 2, "{:?}", report.findings);
    assert!(missing[0].is_like(missing[1]));

    let files = list_project_files(&folder);
    let suggested = suggestion(missing[0], &files).expect("a suggestion");
    assert_eq!(suggested, "assets/models/props/chair.glb#Seat");

    let plan = plan_fix(&folder, &missing, &Fix::Use(suggested.clone())).unwrap();
    assert!(plan.files.is_empty());
    let mut fixed: Vec<_> = plan.properties.iter().map(|fix| fix.id).collect();
    fixed.sort();
    let mut wanted = vec![first, second];
    wanted.sort();
    assert_eq!(fixed, wanted);
    assert!(
        plan.properties
            .iter()
            .all(|fix| fix.property == "MeshId" && fix.value == suggested)
    );

    // A different model keeps no pick.
    let plan = plan_fix(
        &folder,
        &missing[..1],
        &Fix::Use("assets/other/chair2.glb".into()),
    )
    .unwrap();
    assert_eq!(plan.properties[0].value, "assets/other/chair2.glb");
    let wrong = plan_fix(&folder, &missing[..1], &Fix::Use("assets/sky.png".into()));
    assert!(wrong.is_err());
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn locating_a_file_outside_the_project_copies_it_in_once() {
    let folder = scratch("copy-in");
    let outside = scratch("copy-in-source");
    fs::write(outside.join("logo.png"), b"png bytes").unwrap();
    let mut world = World::new();
    add(
        &mut world,
        ClassName::ImageLabel,
        "One",
        "Image",
        "assets/images/gone.png",
    );
    add(
        &mut world,
        ClassName::ImageLabel,
        "Two",
        "Image",
        "assets/images/gone.png",
    );
    let report = check_assets(&folder, &world);
    let findings: Vec<&Finding> = report.findings.iter().collect();
    assert_eq!(findings.len(), 2, "{findings:?}");

    let plan = plan_fix(&folder, &findings, &Fix::CopyIn(outside.join("logo.png"))).unwrap();
    assert_eq!(plan.files.len(), 1, "{:?}", plan.files);
    assert_eq!(plan.files[0].path, "assets/images/logo.png");
    assert_eq!(plan.files[0].before, None);
    assert_eq!(plan.files[0].after.as_deref(), Some(&b"png bytes"[..]));
    assert!(
        plan.properties
            .iter()
            .all(|fix| fix.value == "assets/images/logo.png")
    );
    assert_eq!(plan.properties.len(), 2);

    // A different file of that name gets a number; the same bytes are reused.
    write(&folder, "assets/images/logo.png", b"another picture");
    let plan = plan_fix(
        &folder,
        &findings[..1],
        &Fix::CopyIn(outside.join("logo.png")),
    )
    .unwrap();
    assert_eq!(plan.files[0].path, "assets/images/logo2.png");
    write(&folder, "assets/images/logo.png", b"png bytes");
    let plan = plan_fix(
        &folder,
        &findings[..1],
        &Fix::CopyIn(outside.join("logo.png")),
    )
    .unwrap();
    assert!(plan.files.is_empty());
    assert_eq!(plan.properties[0].value, "assets/images/logo.png");

    // A file inside the project is used where it is.
    let plan = plan_fix(
        &folder,
        &findings[..1],
        &Fix::CopyIn(folder.join("assets/images/logo.png")),
    )
    .unwrap();
    assert!(plan.files.is_empty());
    assert_eq!(plan.properties[0].value, "assets/images/logo.png");

    // The wrong kind of file is refused.
    fs::write(outside.join("notes.txt"), b"hello").unwrap();
    assert!(
        plan_fix(
            &folder,
            &findings[..1],
            &Fix::CopyIn(outside.join("notes.txt"))
        )
        .is_err()
    );
    fs::remove_dir_all(folder).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn a_models_missing_texture_is_copied_next_to_the_model() {
    let folder = scratch("model-texture");
    write(&folder, "assets/textures/wood.png", b"wood");
    write(&folder, "assets/textures/wood.jpg", b"not a png");
    let finding = Finding {
        kind: FindingKind::MissingFile,
        owner: "assets/models/crate/crate.fbx".into(),
        path: "assets/models/crate/crate.fbx".into(),
        message: "the model is drawn without its texture `C:\\Art\\wood.png`".into(),
        site: Site::ModelTexture {
            model: "assets/models/crate/crate.fbx".into(),
            texture: "C:\\Art\\wood.png".into(),
        },
        wants: Some(AssetKind::Texture),
    };
    let files = list_project_files(&folder);
    assert_eq!(
        suggestion(&finding, &files).as_deref(),
        Some("assets/textures/wood.png")
    );
    let plan = plan_fix(
        &folder,
        &[&finding],
        &Fix::Use("assets/textures/wood.png".into()),
    )
    .unwrap();
    assert_eq!(plan.files.len(), 1);
    assert_eq!(plan.files[0].path, "assets/models/crate/textures/wood.png");
    assert_eq!(plan.files[0].after.as_deref(), Some(&b"wood"[..]));
    assert!(plan.properties.is_empty());
    let wrong = plan_fix(
        &folder,
        &[&finding],
        &Fix::Use("assets/textures/wood.jpg".into()),
    );
    assert!(wrong.unwrap_err().contains(".png"));
    assert!(!finding.can_clear());
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_missing_clip_in_a_state_machine_and_in_a_prefab_can_be_replaced_or_cleared() {
    let folder = scratch("files");
    let machine = StateMachineFile {
        default_state: "Walk".into(),
        states: vec![StateDef::new(
            "Walk",
            Motion::Clip("assets/animations/hero/Walk.anim.json".into()),
            [0.0, 0.0],
        )],
        ..StateMachineFile::default()
    };
    write(
        &folder,
        "assets/animations/hero/hero.statemachine.json",
        machine.to_json().as_bytes(),
    );
    write(
        &folder,
        "assets/prefabs/Door.prefab.json",
        br#"{
  "format_version": 1,
  "instances": [
    {
      "class": "Part",
      "name": "Door",
      "children": [
        {
          "class": "Animation",
          "name": "Open",
          "properties": { "Clip": "assets/animations/Door/Open.anim.json" }
        }
      ]
    }
  ]
}
"#,
    );
    let report = check_assets(&folder, &World::new());
    let state = report
        .findings
        .iter()
        .find(|finding| matches!(finding.site, Site::StateClip { .. }))
        .expect("the state's clip");
    let prefab = report
        .findings
        .iter()
        .find(|finding| matches!(finding.site, Site::Prefab { .. }))
        .expect("the prefab's clip");
    assert_eq!(
        prefab.site,
        Site::Prefab {
            file: "assets/prefabs/Door.prefab.json".into(),
            place: vec![0, 0],
            property: "Clip",
        }
    );

    let run = "assets/animations/hero/Run.anim.json";
    let plan = plan_fix(&folder, &[state], &Fix::Use(run.into())).unwrap();
    assert_eq!(plan.files.len(), 1);
    let rewritten = StateMachineFile::from_json(&text(&plan.files[0])).unwrap();
    assert_eq!(rewritten.states[0].motion, Motion::Clip(run.into()));
    assert_eq!(
        plan.files[0].before.as_deref(),
        Some(machine.to_json().as_bytes())
    );

    let plan = plan_fix(&folder, &[prefab], &Fix::Clear).unwrap();
    assert_eq!(plan.files.len(), 1);
    let mut world = World::new();
    let workspace = world.workspace();
    let roots = parse_prefab(&text(&plan.files[0]), &mut world, workspace).unwrap();
    let open = world.get(roots[0]).unwrap().children()[0];
    let clip = world.get(open).unwrap().property("Clip").cloned();
    assert!(
        clip.is_none() || clip == Some(Value::String(String::new())),
        "{clip:?}"
    );
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_damaged_file_can_be_deleted_and_findings_ignored() {
    let folder = scratch("delete-ignore");
    write(&folder, "assets/models/broken.glb", b"not a glb");
    let report = check_assets(&folder, &World::new());
    let broken = &report.findings[0];
    assert!(broken.can_delete(), "{broken:?}");
    assert!(!broken.can_point_elsewhere());
    let plan = plan_fix(&folder, &[broken], &Fix::DeleteFile).unwrap();
    assert_eq!(
        plan.files,
        vec![FileWrite {
            path: "assets/models/broken.glb".into(),
            before: Some(b"not a glb".to_vec()),
            after: None,
        }]
    );
    assert!(
        folder.join("assets/models/broken.glb").exists(),
        "planning deletes nothing"
    );

    assert!(read_ignored(&folder).is_empty());
    write_ignored(&folder, &[broken.ignore_key(), broken.ignore_key()]).unwrap();
    assert_eq!(read_ignored(&folder), vec![broken.ignore_key()]);
    write_ignored(&folder, &[]).unwrap();
    assert!(!folder.join(IGNORED_FILE).exists());
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn an_absolute_path_to_a_real_file_can_be_copied_in() {
    let folder = scratch("absolute");
    let outside = scratch("absolute-source");
    fs::write(outside.join("chime.wav"), b"RIFF").unwrap();
    let mut world = World::new();
    let absolute = outside.join("chime.wav").to_string_lossy().into_owned();
    add(&mut world, ClassName::Sound, "Chime", "SoundId", &absolute);
    let report = check_assets(&folder, &world);
    let finding = &report.findings[0];
    assert_eq!(finding.kind, FindingKind::InvalidReference);
    assert_eq!(finding.outside_file(), Some(PathBuf::from(&absolute)));
    let plan = plan_fix(&folder, &[finding], &Fix::CopyIn(PathBuf::from(&absolute))).unwrap();
    assert_eq!(plan.files[0].path, "assets/audio/chime.wav");
    assert_eq!(plan.properties[0].value, "assets/audio/chime.wav");
    fs::remove_dir_all(folder).unwrap();
    fs::remove_dir_all(outside).unwrap();
}
