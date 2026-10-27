use std::path::PathBuf;

use axcenity_core::{ClassName, Value};

use super::*;
use crate::check_assets;

const MACHINE: &str = "assets/animations/noodle/noodle.statemachine.json";
const WALK: &str = "assets/animations/noodle/Walk.anim.json";

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

/// An empty project folder for `name`.
fn empty_project(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-package-{name}-{}", std::process::id()));
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

/// noodle's state machine from hello-animation, written to a package and
/// read back.
fn noodle_package(name: &str) -> Package {
    let source = example("hello-animation");
    let contents = package_contents(&source, &[MACHINE.to_owned()], &[]).unwrap();
    let file = std::env::temp_dir().join(format!("axcenity-{name}-{}.axpkg", std::process::id()));
    write_package(&file, "noodle", &[MACHINE.to_owned()], &contents.files).unwrap();
    let package = read_package(&file).unwrap();
    let _ = fs::remove_file(&file);
    package
}

/// Writes the plan's files, as the editor does.
fn apply(folder: &Path, plan: &ImportPlan) {
    for (path, bytes) in &plan.writes {
        let disk = folder.join(path);
        fs::create_dir_all(disk.parent().unwrap()).unwrap();
        fs::write(disk, bytes).unwrap();
    }
}

fn paths(writes: &[(String, Vec<u8>)]) -> Vec<&str> {
    writes.iter().map(|(path, _)| path.as_str()).collect()
}

#[test]
fn a_state_machine_brings_its_clips_and_the_model_they_link_to() {
    let contents =
        package_contents(&example("hello-animation"), &[MACHINE.to_owned()], &[]).unwrap();
    let files: Vec<&str> = contents.files.keys().map(String::as_str).collect();
    assert_eq!(
        files,
        [
            "assets/animations/noodle/Hop.anim.json",
            "assets/animations/noodle/Idle.anim.json",
            "assets/animations/noodle/Run.anim.json",
            WALK,
            "assets/animations/noodle/Wave.anim.json",
            MACHINE,
            "assets/models/noodle.fbx",
        ]
    );
    assert!(contents.missing.is_empty());
}

#[test]
fn a_prefab_brings_every_asset_it_uses_and_a_model_its_whole_folder() {
    let folder = empty_project("prefab-needs");
    for path in [
        "assets/models/crate/crate.gltf",
        "assets/models/crate/crate.bin",
        "assets/models/crate/textures/wood.png",
        "assets/textures/scratches.png",
        "assets/audio/thud.wav",
        "assets/textures/unused.png",
    ] {
        let disk = folder.join(path);
        fs::create_dir_all(disk.parent().unwrap()).unwrap();
        fs::write(disk, path).unwrap();
    }
    let mut world = World::new();
    let crate_part = world.create(ClassName::MeshPart).unwrap();
    let set = |world: &mut World, id, property, path: &str| {
        world
            .set_property(id, property, Value::String(path.to_owned()))
            .unwrap();
    };
    set(
        &mut world,
        crate_part,
        "MeshId",
        "assets/models/crate/crate.gltf#Box",
    );
    world
        .set_parent(crate_part, Some(world.workspace()))
        .unwrap();
    let surface = world.create(ClassName::SurfaceAppearance).unwrap();
    set(
        &mut world,
        surface,
        "ColorMap",
        "assets/textures/scratches.png",
    );
    world.set_parent(surface, Some(crate_part)).unwrap();
    let sound = world.create(ClassName::Sound).unwrap();
    set(&mut world, sound, "SoundId", "assets/audio/thud.wav");
    world.set_parent(sound, Some(crate_part)).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    set(
        &mut world,
        animator,
        "StateMachine",
        "assets/animations/gone.statemachine.json",
    );
    world.set_parent(animator, Some(crate_part)).unwrap();
    let prefab = (
        "assets/prefabs/Crate.prefab.json".to_owned(),
        crate::write_prefab(&world, &[crate_part]).into_bytes(),
    );

    let contents = package_contents(&folder, &[], &[prefab]).unwrap();
    let files: Vec<&str> = contents.files.keys().map(String::as_str).collect();
    assert_eq!(
        files,
        [
            "assets/audio/thud.wav",
            "assets/models/crate/crate.bin",
            "assets/models/crate/crate.gltf",
            "assets/models/crate/textures/wood.png",
            "assets/prefabs/Crate.prefab.json",
            "assets/textures/scratches.png",
        ]
    );
    assert_eq!(
        contents.missing,
        ["assets/animations/gone.statemachine.json"]
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_package_reads_back_as_written_and_refuses_paths_outside_assets() {
    let package = noodle_package("round-trip");
    assert_eq!(package.manifest.name, "noodle");
    assert_eq!(package.manifest.roots, [MACHINE]);
    assert_eq!(package.files.len(), 7);
    let fbx = fs::read(example("hello-animation").join("assets/models/noodle.fbx")).unwrap();
    assert_eq!(package.files["assets/models/noodle.fbx"], fbx);

    let folder = empty_project("unsafe-package");
    for bad in ["../evil.txt", "world.json", "assets/../world.json"] {
        let file = folder.join("bad.axpkg");
        let files = BTreeMap::from([(bad.to_owned(), b"x".to_vec())]);
        write_package(&file, "bad", &[], &files).unwrap();
        assert!(
            matches!(read_package(&file), Err(PackageError::UnsafePath(_))),
            "{bad}"
        );
    }
    assert!(matches!(
        package_contents(&folder, &["assets/nothing.png".to_owned()], &[]),
        Err(PackageError::MissingRoot(_))
    ));
    let _ = fs::remove_dir_all(&folder);
}

/// Hero's state machine (here, noodle's), exported from one project and
/// imported into an empty one, plays the same: its clips and model came
/// with it, so the same world poses the same in both.
#[test]
fn an_imported_state_machine_plays_the_same_as_in_the_project_it_came_from() {
    let source = example("hello-animation");
    let folder = empty_project("plays-the-same");
    let plan = plan_import(
        &folder,
        &noodle_package("plays-the-same"),
        Clashes::KeepBoth,
    );
    assert_eq!(plan.writes.len(), 7);
    assert!(plan.clashes.is_empty() && plan.identical.is_empty());
    apply(&folder, &plan);
    assert!(check_assets(&folder, &World::new()).is_ok());

    // The same object in each: noodle with its Animator.
    let text = fs::read_to_string(source.join(crate::WORLD_FILE)).unwrap();
    let pose = |folder: &Path| {
        let mut world = crate::parse_world(&text).unwrap();
        assert!(check_assets(folder, &world).is_ok());
        let assets = axcenity_assets::Assets::new(folder);
        let mut animators = axcenity_anim::Animators::new();
        let mut poses = Vec::new();
        for _ in 0..30 {
            animators.update(&mut world, &assets, 1.0 / 30.0);
            assert!(animators.take_errors().is_empty());
            let mut frame: Vec<_> = animators
                .poses()
                .iter()
                .map(|(id, posed)| {
                    let model = assets.model("assets/models/noodle.fbx").unwrap();
                    let corners: Vec<_> = posed.primitives(&model).into_iter().cloned().collect();
                    (*id, corners)
                })
                .collect();
            frame.sort_by_key(|(id, _)| *id);
            poses.push(frame);
        }
        poses
    };
    let there = pose(&source);
    assert!(!there.last().unwrap().is_empty(), "noodle is posed");
    assert_eq!(pose(&folder), there);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn importing_the_same_package_twice_adds_nothing_the_second_time() {
    let folder = empty_project("twice");
    let package = noodle_package("twice");
    apply(&folder, &plan_import(&folder, &package, Clashes::KeepBoth));
    for clashes in [Clashes::KeepBoth, Clashes::Replace, Clashes::SkipExisting] {
        let again = plan_import(&folder, &package, clashes);
        assert!(
            again.writes.is_empty(),
            "{clashes:?}: {:?}",
            paths(&again.writes)
        );
        assert!(again.clashes.is_empty() && again.renamed.is_empty());
        assert_eq!(again.identical.len(), 7);
    }
    let _ = fs::remove_dir_all(&folder);
}

/// After the package is imported, the project's Walk clip is changed.
/// Importing again: Keep both brings a numbered copy of the folder that
/// names its own clips, Replace writes the package's Walk over the
/// project's, and Skip existing leaves it. Asset Check finds nothing
/// broken after any of them.
#[test]
fn keep_both_replace_and_skip_existing_each_do_what_they_say() {
    let package = noodle_package("clashes");
    let ours = package.files[WALK].clone();
    let changed = String::from_utf8(ours.clone())
        .unwrap()
        .replace("\"Walk\"", "\"Walk\"  ")
        .into_bytes();
    assert_ne!(changed, ours);
    let project = |name: &str| {
        let folder = empty_project(name);
        apply(&folder, &plan_import(&folder, &package, Clashes::KeepBoth));
        fs::write(folder.join(WALK), &changed).unwrap();
        folder
    };

    // Replace: the package's Walk over the project's.
    let folder = project("replace");
    let plan = plan_import(&folder, &package, Clashes::Replace);
    assert_eq!(plan.clashes, [WALK]);
    assert_eq!(plan.identical.len(), 6);
    assert_eq!(paths(&plan.writes), [WALK]);
    apply(&folder, &plan);
    assert_eq!(fs::read(folder.join(WALK)).unwrap(), ours);
    assert!(check_assets(&folder, &World::new()).is_ok());
    let _ = fs::remove_dir_all(&folder);

    // Skip existing: the project's Walk stays.
    let folder = project("skip");
    let plan = plan_import(&folder, &package, Clashes::SkipExisting);
    assert_eq!(plan.clashes, [WALK]);
    assert!(plan.writes.is_empty());
    apply(&folder, &plan);
    assert_eq!(fs::read(folder.join(WALK)).unwrap(), changed);
    assert!(check_assets(&folder, &World::new()).is_ok());
    let _ = fs::remove_dir_all(&folder);

    // Keep both: noodle2 beside noodle, its state machine naming its own
    // clips; the model was the same, so it's shared.
    let folder = project("keep-both");
    let plan = plan_import(&folder, &package, Clashes::KeepBoth);
    assert_eq!(plan.clashes, [WALK]);
    assert_eq!(
        plan.renamed,
        [(
            "assets/animations/noodle".to_owned(),
            "assets/animations/noodle2".to_owned()
        )]
    );
    assert_eq!(
        paths(&plan.writes),
        [
            "assets/animations/noodle2/Hop.anim.json",
            "assets/animations/noodle2/Idle.anim.json",
            "assets/animations/noodle2/Run.anim.json",
            "assets/animations/noodle2/Walk.anim.json",
            "assets/animations/noodle2/Wave.anim.json",
            "assets/animations/noodle2/noodle.statemachine.json",
        ]
    );
    apply(&folder, &plan);
    assert_eq!(fs::read(folder.join(WALK)).unwrap(), changed);
    let machine =
        fs::read_to_string(folder.join("assets/animations/noodle2/noodle.statemachine.json"))
            .unwrap();
    assert!(machine.contains("\"assets/animations/noodle2/Walk.anim.json\""));
    assert!(!machine.contains("\"assets/animations/noodle/"));
    assert_eq!(
        fs::read(folder.join("assets/animations/noodle2/Walk.anim.json")).unwrap(),
        ours
    );
    assert!(check_assets(&folder, &World::new()).is_ok());
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn keep_both_numbers_a_file_and_rewrites_what_names_it() {
    let folder = empty_project("keep-both-file");
    let incoming = BTreeMap::from([
        ("assets/textures/wood.png".to_owned(), b"new wood".to_vec()),
        (
            "assets/prefabs/Crate.prefab.json".to_owned(),
            br#"{"x": "assets/textures/wood.png", "y": "assets/textures/wood.png2"}"#.to_vec(),
        ),
    ]);
    let package = Package {
        manifest: PackageManifest {
            format: PACKAGE_FORMAT,
            name: "Crate".to_owned(),
            roots: Vec::new(),
            files: incoming.keys().cloned().collect(),
        },
        files: incoming,
    };
    fs::create_dir_all(folder.join("assets/textures")).unwrap();
    fs::write(folder.join("assets/textures/wood.png"), b"old wood").unwrap();
    fs::write(folder.join("assets/textures/wood2.png"), b"taken").unwrap();

    let plan = plan_import(&folder, &package, Clashes::KeepBoth);
    assert_eq!(
        plan.renamed,
        [(
            "assets/textures/wood.png".to_owned(),
            "assets/textures/wood3.png".to_owned()
        )]
    );
    let writes: BTreeMap<_, _> = plan.writes.into_iter().collect();
    assert_eq!(writes["assets/textures/wood3.png"], b"new wood");
    assert_eq!(
        writes["assets/prefabs/Crate.prefab.json"],
        br#"{"x": "assets/textures/wood3.png", "y": "assets/textures/wood.png2"}"#
    );
    let _ = fs::remove_dir_all(&folder);
}
