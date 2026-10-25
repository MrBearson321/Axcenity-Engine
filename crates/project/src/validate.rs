//! One pass over an open project that lists missing files, invalid
//! references, import failures, and broken dependencies. The editor shows
//! the list; this module does not write or delete anything.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use axcenity_anim::{
    ANIMATIONS_FOLDER, CLIP_EXTENSION, ClipFile, Motion, STATE_MACHINE_EXTENSION, StateMachineFile,
};
use axcenity_assets::{
    AUDIO_EXTENSIONS, Assets, MODELS_FOLDER, ModelError, ModelFormat, Pick, TEXTURE_EXTENSIONS,
    load_models, load_takes, safe_path,
};
use axcenity_core::{ClassName, InstanceId, Value, World};

use crate::world_file::parse_prefab;

/// Folders at the project root that are not part of the game.
const SKIP_ROOT_DIRS: &[&str] = &[".axcenity", ".git", "target", "node_modules"];
pub(crate) const PREFABS_FOLDER: &str = "assets/prefabs";
pub(crate) const PREFAB_SUFFIX: &str = ".prefab.json";

const SURFACE_MAPS: [&str; 8] = [
    "ColorMap",
    "NormalMap",
    "MetalnessMap",
    "RoughnessMap",
    "EmissiveMap",
    "AmbientOcclusionMap",
    "OpacityMap",
    "SpecularMap",
];

const LAYER_MAPS: [&str; 6] = [
    "ColorMap",
    "NormalMap",
    "RoughnessMap",
    "MetalnessMap",
    "AmbientOcclusionMap",
    "HeightMap",
];

/// What kind of problem a finding is.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FindingKind {
    /// A path that should name a file (or folder) that isn't there.
    MissingFile,
    /// A path that isn't a legal project path, or names the wrong kind of
    /// file, or an object/take the file doesn't have.
    InvalidReference,
    /// A file that's there but can't be loaded.
    ImportFailure,
    /// An asset file that names another asset that isn't there.
    BrokenDependency,
}

impl FindingKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::MissingFile => "Missing file",
            Self::InvalidReference => "Invalid reference",
            Self::ImportFailure => "Import failure",
            Self::BrokenDependency => "Broken dependency",
        }
    }
}

/// One problem the scan found.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Finding {
    pub kind: FindingKind,
    /// The Instance path or asset file that holds the reference.
    pub owner: String,
    /// The path the owner names, or the file that failed to load.
    pub path: String,
    pub message: String,
}

/// Everything the scan found. Empty means the project has none of those
/// problems.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.findings.is_empty()
    }
}

#[derive(Clone, Copy)]
pub(crate) enum AssetKind {
    Model,
    Texture,
    Audio,
    Clip,
    StateMachine,
    Folder,
}

impl AssetKind {
    fn expected(self) -> &'static str {
        match self {
            Self::Model => "a .gltf, .glb or .fbx file",
            Self::Texture => "a PNG, JPEG, TGA or BMP file",
            Self::Audio => "a WAV, OGG, MP3 or FLAC file",
            Self::Clip => "a .anim.json clip",
            Self::StateMachine => "a .statemachine.json file",
            Self::Folder => "a folder inside the project",
        }
    }

    fn matches(self, path: &str) -> bool {
        match self {
            Self::Model => ModelFormat::of(Path::new(path)).is_some(),
            Self::Texture => has_extension(path, &TEXTURE_EXTENSIONS),
            Self::Audio => has_extension(path, &AUDIO_EXTENSIONS),
            Self::Clip => path.to_ascii_lowercase().ends_with(CLIP_EXTENSION),
            Self::StateMachine => path.to_ascii_lowercase().ends_with(STATE_MACHINE_EXTENSION),
            Self::Folder => true,
        }
    }
}

/// Scans `folder` and `world` without writing. `world` is the open tree
/// (including unsaved edits); prefabs, clips, state machines and models are
/// read from disk.
pub fn check_assets(folder: &Path, world: &World) -> Report {
    let mut scan = Scan {
        folder: folder.to_path_buf(),
        loader: Assets::new(folder),
        findings: Vec::new(),
        models: HashMap::new(),
    };
    scan.world(world, None);
    scan.prefabs();
    scan.clips();
    scan.state_machines();
    scan.unreferenced_models();
    scan.findings.sort();
    scan.findings.dedup();
    Report {
        findings: scan.findings,
    }
}

struct Scan {
    folder: PathBuf,
    loader: Assets,
    findings: Vec<Finding>,
    /// Whole-file model loads, keyed by the file path (no `#` pick).
    models: HashMap<String, Result<Vec<String>, String>>,
}

impl Scan {
    fn world(&mut self, world: &World, prefab: Option<&str>) {
        for id in world.descendants(world.root()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let owner = owner_name(world, id, prefab);
            let from_asset = prefab.is_some();
            for (property, kind) in refs(instance.class()) {
                let Some(path) = string_prop(instance.property(property)) else {
                    continue;
                };
                self.reference(&owner, property, path, kind, from_asset);
            }
        }
    }

    fn prefabs(&mut self) {
        for path in files_under(&self.folder, PREFABS_FOLDER) {
            if !path.to_ascii_lowercase().ends_with(PREFAB_SUFFIX) {
                continue;
            }
            let disk = self
                .folder
                .join(safe_path(&path).unwrap_or_else(|| PathBuf::from(&path)));
            let text = match fs::read_to_string(&disk) {
                Ok(text) => text,
                Err(err) => {
                    self.push(
                        FindingKind::ImportFailure,
                        path.clone(),
                        path,
                        format!("could not read the prefab: {err}"),
                    );
                    continue;
                }
            };
            let mut world = World::new();
            let workspace = world.workspace();
            match parse_prefab(&text, &mut world, workspace) {
                Ok(_) => self.world(&world, Some(&path)),
                Err(err) => self.push(
                    FindingKind::ImportFailure,
                    path.clone(),
                    path,
                    format!("could not load the prefab: {err}"),
                ),
            }
        }
    }

    fn clips(&mut self) {
        for path in files_under(&self.folder, ANIMATIONS_FOLDER) {
            if !path.to_ascii_lowercase().ends_with(CLIP_EXTENSION) {
                continue;
            }
            let disk = self
                .folder
                .join(safe_path(&path).unwrap_or_else(|| PathBuf::from(&path)));
            let text = match fs::read_to_string(&disk) {
                Ok(text) => text,
                Err(err) => {
                    self.push(
                        FindingKind::ImportFailure,
                        path.clone(),
                        path,
                        format!("could not read the clip: {err}"),
                    );
                    continue;
                }
            };
            let clip = match ClipFile::from_json(&text) {
                Ok(clip) => clip,
                Err(err) => {
                    self.push(
                        FindingKind::ImportFailure,
                        path.clone(),
                        path,
                        format!("could not load the clip: {err}"),
                    );
                    continue;
                }
            };
            let Some(link) = clip.link else {
                continue;
            };
            self.reference(&path, "source", &link.source, AssetKind::Model, true);
            if let Some(file) = safe_path(&link.source) {
                let file = file.to_string_lossy().replace('\\', "/");
                if self.folder.join(&file).is_file() {
                    self.take_exists(&path, &link.source, &link.take);
                }
            }
        }
    }

    fn state_machines(&mut self) {
        for path in files_under(&self.folder, ANIMATIONS_FOLDER) {
            if !path.to_ascii_lowercase().ends_with(STATE_MACHINE_EXTENSION) {
                continue;
            }
            let disk = self
                .folder
                .join(safe_path(&path).unwrap_or_else(|| PathBuf::from(&path)));
            let text = match fs::read_to_string(&disk) {
                Ok(text) => text,
                Err(err) => {
                    self.push(
                        FindingKind::ImportFailure,
                        path.clone(),
                        path,
                        format!("could not read the state machine: {err}"),
                    );
                    continue;
                }
            };
            let machine = match StateMachineFile::from_json(&text) {
                Ok(machine) => machine,
                Err(err) => {
                    self.push(
                        FindingKind::ImportFailure,
                        path.clone(),
                        path,
                        format!("could not load the state machine: {err}"),
                    );
                    continue;
                }
            };
            for state in &machine.states {
                for clip in state.motion.clips() {
                    let property = match &state.motion {
                        Motion::Clip(_) => "clip",
                        Motion::BlendTree(_) => "blend tree",
                    };
                    self.reference(&path, property, clip, AssetKind::Clip, true);
                }
            }
        }
    }

    fn unreferenced_models(&mut self) {
        for path in files_under(&self.folder, MODELS_FOLDER) {
            if ModelFormat::of(Path::new(&path)).is_none() {
                continue;
            }
            if self.findings.iter().any(|finding| {
                finding.path == path || finding.path.starts_with(&format!("{path}#"))
            }) {
                continue;
            }
            match self.model_file(&path) {
                Ok(warnings) => {
                    for warning in warnings {
                        self.push(
                            FindingKind::MissingFile,
                            path.clone(),
                            path.clone(),
                            format!("the model {warning}"),
                        );
                    }
                }
                Err(err) => self.push(
                    FindingKind::ImportFailure,
                    path.clone(),
                    path,
                    format!("could not load the model: {err}"),
                ),
            }
        }
    }

    fn reference(
        &mut self,
        owner: &str,
        property: &str,
        path: &str,
        kind: AssetKind,
        from_asset: bool,
    ) {
        if path.is_empty() {
            return;
        }
        let (file, pick) = match kind {
            AssetKind::Model => Pick::of_mesh_id(path),
            _ => (path, Pick::Whole),
        };
        let Some(relative) = safe_path(file) else {
            self.push(
                FindingKind::InvalidReference,
                owner.to_owned(),
                path.to_owned(),
                format!("`{property}` `{path}` is not a path inside the project"),
            );
            return;
        };
        if !kind.matches(file) {
            self.push(
                FindingKind::InvalidReference,
                owner.to_owned(),
                path.to_owned(),
                format!("`{property}` must name {}, not `{file}`", kind.expected()),
            );
            return;
        }
        let disk = self.folder.join(&relative);
        let present = match kind {
            AssetKind::Folder => disk.is_dir(),
            _ => disk.is_file(),
        };
        if !present {
            let kind_name = if from_asset {
                FindingKind::BrokenDependency
            } else {
                FindingKind::MissingFile
            };
            self.push(
                kind_name,
                owner.to_owned(),
                path.to_owned(),
                format!("`{property}` names `{path}`, which isn't there"),
            );
            return;
        }
        match kind {
            AssetKind::Model => self.model_ref(owner, property, path, file, &pick),
            AssetKind::Texture => {
                if self.loader.texture(path).is_none() {
                    let _ = self.loader.take_errors();
                    self.push(
                        FindingKind::ImportFailure,
                        owner.to_owned(),
                        path.to_owned(),
                        format!("could not load the texture `{path}`"),
                    );
                }
            }
            AssetKind::Clip => {
                if let Err(err) = read_clip(&self.folder, path) {
                    self.push(
                        FindingKind::ImportFailure,
                        owner.to_owned(),
                        path.to_owned(),
                        format!("`{property}` names `{path}`, which could not be loaded: {err}"),
                    );
                }
            }
            AssetKind::StateMachine => {
                if let Err(err) = read_state_machine(&self.folder, path) {
                    self.push(
                        FindingKind::ImportFailure,
                        owner.to_owned(),
                        path.to_owned(),
                        format!("`{property}` names `{path}`, which could not be loaded: {err}"),
                    );
                }
            }
            AssetKind::Audio | AssetKind::Folder => {}
        }
    }

    fn model_ref(&mut self, owner: &str, property: &str, mesh_id: &str, file: &str, pick: &Pick) {
        match load_model_pick(&self.folder, file, pick) {
            Ok(warnings) => {
                for warning in warnings {
                    self.push(
                        FindingKind::MissingFile,
                        owner.to_owned(),
                        mesh_id.to_owned(),
                        format!("the model `{mesh_id}` {warning}"),
                    );
                }
            }
            Err(ModelError::NoObject(object)) => self.push(
                FindingKind::InvalidReference,
                owner.to_owned(),
                mesh_id.to_owned(),
                format!("`{property}` has no object `{object}` in `{file}`"),
            ),
            Err(err) => self.push(
                FindingKind::ImportFailure,
                owner.to_owned(),
                mesh_id.to_owned(),
                format!("could not load the model `{file}`: {}", causes(&err)),
            ),
        }
    }

    fn take_exists(&mut self, owner: &str, source: &str, take: &str) {
        let (file, _) = Pick::of_mesh_id(source);
        let Some(relative) = safe_path(file) else {
            return;
        };
        let format = match ModelFormat::of(&relative) {
            Some(format) => format,
            None => return,
        };
        let bytes = match fs::read(self.folder.join(&relative)) {
            Ok(bytes) => bytes,
            Err(_) => return,
        };
        if let Ok(takes) = load_takes(&bytes, format)
            && !takes.iter().any(|found| found.name == take)
        {
            self.push(
                FindingKind::InvalidReference,
                owner.to_owned(),
                source.to_owned(),
                format!("the clip's take `{take}` isn't in `{file}`"),
            );
        }
    }

    fn model_file(&mut self, path: &str) -> Result<Vec<String>, String> {
        if let Some(result) = self.models.get(path) {
            return result.clone();
        }
        let result = match load_model_pick(&self.folder, path, &Pick::Whole) {
            Ok(warnings) => Ok(warnings),
            Err(err) => Err(causes(&err)),
        };
        self.models.insert(path.to_owned(), result.clone());
        result
    }

    fn push(&mut self, kind: FindingKind, owner: String, path: String, message: String) {
        self.findings.push(Finding {
            kind,
            owner,
            path,
            message,
        });
    }
}

pub(crate) fn refs(class: ClassName) -> Vec<(&'static str, AssetKind)> {
    match class {
        ClassName::MeshPart => vec![("MeshId", AssetKind::Model)],
        ClassName::Sound => vec![("SoundId", AssetKind::Audio)],
        ClassName::ImageLabel => vec![("Image", AssetKind::Texture)],
        ClassName::ParticleEmitter => vec![("Texture", AssetKind::Texture)],
        ClassName::Animator => vec![("StateMachine", AssetKind::StateMachine)],
        ClassName::Animation => vec![("Clip", AssetKind::Clip)],
        ClassName::Terrain => vec![("DataFolder", AssetKind::Folder)],
        ClassName::SurfaceAppearance => SURFACE_MAPS
            .into_iter()
            .map(|name| (name, AssetKind::Texture))
            .collect(),
        ClassName::TerrainLayer => LAYER_MAPS
            .into_iter()
            .map(|name| (name, AssetKind::Texture))
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn string_prop(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::String(text)) => Some(text.as_str()),
        _ => None,
    }
}

fn owner_name(world: &World, id: InstanceId, prefab: Option<&str>) -> String {
    let name = world.full_name(id).unwrap_or_else(|| "?".to_owned());
    match prefab {
        Some(file) => {
            let local = name.strip_prefix("Workspace.").unwrap_or(&name);
            format!("{file} ({local})")
        }
        None => name,
    }
}

fn load_model_pick(folder: &Path, file: &str, pick: &Pick) -> Result<Vec<String>, ModelError> {
    let relative = safe_path(file).ok_or_else(|| ModelError::OutsideFolder(file.to_owned()))?;
    let format = ModelFormat::of(&relative).ok_or(ModelError::Damaged)?;
    let path = folder.join(&relative);
    let bytes = fs::read(&path).map_err(|_| ModelError::MissingFile(file.to_owned()))?;
    let base = path.parent().unwrap_or(folder);
    let mut models = load_models(&bytes, base, format, std::slice::from_ref(pick))?;
    Ok(models.remove(0).warnings)
}

fn read_clip(folder: &Path, path: &str) -> Result<ClipFile, String> {
    let relative = safe_path(path).ok_or_else(|| "it isn't inside the project".to_owned())?;
    let text = fs::read_to_string(folder.join(relative)).map_err(|err| err.to_string())?;
    ClipFile::from_json(&text).map_err(|err| err.to_string())
}

fn read_state_machine(folder: &Path, path: &str) -> Result<StateMachineFile, String> {
    let relative = safe_path(path).ok_or_else(|| "it isn't inside the project".to_owned())?;
    let text = fs::read_to_string(folder.join(relative)).map_err(|err| err.to_string())?;
    StateMachineFile::from_json(&text).map_err(|err| err.to_string())
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extensions
                .iter()
                .any(|wanted| extension.eq_ignore_ascii_case(wanted))
        })
}

pub(crate) fn files_under(root: &Path, relative: &str) -> Vec<String> {
    let mut found = Vec::new();
    walk(root, &root.join(relative), &mut found);
    found
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if dir == root && SKIP_ROOT_DIRS.contains(&name.as_ref()) {
            continue;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            walk(root, &path, found);
        } else if let Some(shown) = shown_path(root, &path) {
            found.push(shown);
        }
    }
}

fn shown_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

fn causes(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_anim::StateDef;
    use axcenity_core::Value;
    use std::collections::BTreeSet;

    fn examples_folder() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
    }

    fn temp_folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-validate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn scratch(name: &str) -> PathBuf {
        let folder = temp_folder(name);
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

    fn listing(folder: &Path) -> BTreeSet<(String, u64)> {
        let mut files = BTreeSet::new();
        fn add(root: &Path, dir: &Path, files: &mut BTreeSet<(String, u64)>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    add(root, &path, files);
                } else if let Some(shown) = shown_path(root, &path) {
                    let len = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                    files.insert((shown, len));
                }
            }
        }
        add(folder, folder, &mut files);
        files
    }

    #[test]
    fn a_clean_example_reports_nothing() {
        let project = crate::load(&examples_folder().join("hello-part")).unwrap();
        let report = check_assets(&project.folder, &project.world);
        assert!(
            report.is_ok(),
            "{:?}",
            report
                .findings
                .iter()
                .map(|finding| &finding.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_missing_model_an_invalid_path_a_failed_import_and_a_broken_clip_are_listed() {
        let folder = scratch("kinds");
        fs::create_dir_all(folder.join("assets/models")).unwrap();
        fs::create_dir_all(folder.join("assets/animations/hero")).unwrap();
        fs::create_dir_all(folder.join("assets/prefabs")).unwrap();
        fs::write(folder.join("assets/models/broken.glb"), b"not a glb").unwrap();

        let machine = StateMachineFile {
            default_state: "Walk".into(),
            states: vec![StateDef::new(
                "Walk",
                Motion::Clip("assets/animations/hero/Walk.anim.json".into()),
                [0.0, 0.0],
            )],
            ..StateMachineFile::default()
        };
        fs::write(
            folder.join("assets/animations/hero/hero.statemachine.json"),
            machine.to_json(),
        )
        .unwrap();

        let mut world = World::new();
        let missing = world.create(ClassName::MeshPart).unwrap();
        world.set_name(missing, "Missing").unwrap();
        world.set_parent(missing, Some(world.workspace())).unwrap();
        world
            .set_property(
                missing,
                "MeshId",
                Value::String("assets/models/gone.glb".into()),
            )
            .unwrap();

        let invalid = world.create(ClassName::MeshPart).unwrap();
        world.set_name(invalid, "Invalid").unwrap();
        world.set_parent(invalid, Some(world.workspace())).unwrap();
        world
            .set_property(invalid, "MeshId", Value::String("../secret.glb".into()))
            .unwrap();

        let damaged = world.create(ClassName::MeshPart).unwrap();
        world.set_name(damaged, "Damaged").unwrap();
        world.set_parent(damaged, Some(world.workspace())).unwrap();
        world
            .set_property(
                damaged,
                "MeshId",
                Value::String("assets/models/broken.glb".into()),
            )
            .unwrap();

        let before = listing(&folder);
        let report = check_assets(&folder, &world);
        assert_eq!(listing(&folder), before, "the scan must not write files");

        let missing_file = report
            .findings
            .iter()
            .find(|finding| finding.kind == FindingKind::MissingFile)
            .expect("missing file");
        assert_eq!(missing_file.owner, "Workspace.Missing");
        assert!(missing_file.path.contains("gone.glb"), "{missing_file:?}");

        let invalid_ref = report
            .findings
            .iter()
            .find(|finding| finding.kind == FindingKind::InvalidReference)
            .expect("invalid reference");
        assert_eq!(invalid_ref.owner, "Workspace.Invalid");

        let import = report
            .findings
            .iter()
            .find(|finding| finding.kind == FindingKind::ImportFailure)
            .expect("import failure");
        assert!(
            import.owner.contains("Damaged") || import.path.contains("broken.glb"),
            "{import:?}"
        );

        let broken = report
            .findings
            .iter()
            .find(|finding| finding.kind == FindingKind::BrokenDependency)
            .expect("broken dependency");
        assert!(
            broken.owner.contains("hero.statemachine.json"),
            "{broken:?}"
        );
        assert!(broken.path.contains("Walk.anim.json"), "{broken:?}");

        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_prefab_that_names_a_missing_clip_is_a_broken_dependency() {
        let folder = scratch("prefab-clip");
        fs::create_dir_all(folder.join("assets/prefabs")).unwrap();
        fs::write(
            folder.join("assets/prefabs/Door.prefab.json"),
            r#"{
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
        )
        .unwrap();
        let world = World::new();
        let report = check_assets(&folder, &world);
        let broken = report
            .findings
            .iter()
            .find(|finding| finding.kind == FindingKind::BrokenDependency)
            .expect("broken dependency");
        assert!(broken.owner.contains("Door.prefab.json"), "{broken:?}");
        assert!(broken.path.contains("Open.anim.json"), "{broken:?}");
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_missing_file_still_leaves_the_rest_of_the_project_open() {
        let folder = scratch("still-open");
        fs::create_dir_all(folder.join("scripts")).unwrap();
        fs::write(folder.join("scripts/main.server.lua"), "print('ok')").unwrap();
        let mut world_text = String::from(
            r#"{
  "format_version": 1,
  "workspace": {
    "children": [
      {
        "class": "MeshPart",
        "name": "Hero",
        "properties": { "MeshId": "assets/models/missing.glb" }
      }
    ]
  }
}
"#,
        );
        // Keep the JSON valid if the writer added a trailing comma in a draft.
        world_text = world_text.replace(",\n      }\n    ]", "\n      }\n    ]");
        fs::write(folder.join(crate::WORLD_FILE), world_text).unwrap();

        let project = crate::load(&folder).unwrap();
        assert!(project.world_loaded);
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        assert_eq!(project.scripts.len(), 1);
        let report = check_assets(&project.folder, &project.world);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.kind == FindingKind::MissingFile
                    && finding.owner.contains("Hero")),
            "{:?}",
            report.findings
        );
        fs::remove_dir_all(folder).unwrap();
    }
}
