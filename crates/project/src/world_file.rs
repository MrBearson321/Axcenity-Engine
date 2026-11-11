//! The world file (`world.json`): the Instances that make up the world.
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "workspace": {
//!     "children": [
//!       {
//!         "class": "Part",
//!         "name": "Block",
//!         "properties": { "Position": [0, 1, 0], "Color": [0.8, 0.2, 0.2] },
//!         "children": []
//!       }
//!     ]
//!   },
//!   "replicated_storage": {
//!     "children": [ { "class": "RemoteEvent", "name": "Jump" } ]
//!   }
//! }
//! ```
//!
//! Only the Workspace, ReplicatedStorage, Lighting (its settings and
//! what's inside it, such as an Atmosphere), and the settings of Players,
//! StarterPlayer and Teams (with its Teams) are saved.
//! The Workspace's own properties (`GlobalWind`) are only written when they
//! aren't their defaults, so `"workspace": {}` stays as short as ever.
//! `replicated_storage` can be left out, and is only written when it has
//! something in it. `lighting` (`{ "properties": { "ShadowsEnabled": false } }`,
//! with `children` like the Workspace's) can be left out too, and is only
//! written when a setting isn't its default or something is inside it.
//! So are `players` (`CharacterAutoLoads`, `RespawnTime`,
//! `LeaderboardEnabled`), `starter_player` (`StarterCharacter`) and `teams`
//! (its Team children), in the same shape (Phase 47), and `sound_service`
//! (its SoundGroups, with their effects, Phase 52). `server_storage`
//! (Phase 64) is saved like `replicated_storage`, and only when it has
//! something in it.
//!
//! `Locked` and `BlueprintTexture` are only written when on, so a Folder
//! that's never been locked still has no `properties` at all.
//!
//! Property values are plain JSON. Their meaning comes from the class: the
//! same `[0, 1, 0]` is a `Vector3` for `Position` and a `Color3` for `Color`,
//! `[0, 120]` is a `Vector2`, `[0, 0, 1, 0]` is a `UDim2` (X scale, X offset, Y scale, Y offset), and
//! enum items are strings such as `"Neon"`. Properties that are left out keep
//! their default values.
//!
//! A `CFrame` is `[x, y, z, qx, qy, qz, qw]`: the position, then the
//! rotation as a quaternion. `CustomPhysicalProperties` is `null` (the
//! material's) or `{ "Density": 1, "Friction": 0.5, "Elasticity": 0.25,
//! "FrictionWeight": 1, "ElasticityWeight": 1 }`; numbers left out are
//! Plastic's, and each is kept in its range. An `ObjectValue`'s `Value` is the path of
//! names to the Instance it points at (`["Workspace", "Door"]`; in a
//! prefab, from one of the prefab's own Instances), or `null`. A path
//! through siblings with the same name finds the first.
//!
//! An Instance can also have `"attributes"` and `"tags"`, both left out
//! when empty:
//!
//! ```json
//! { "class": "Part", "name": "Lava",
//!   "attributes": { "Damage": 10, "Spawn": { "Vector3": [0, 5, 0] } },
//!   "tags": ["Lava"] }
//! ```
//!
//! Booleans, numbers and strings are plain JSON; any other attribute value
//! is an object with one field naming its type (`Vector3`, `Vector2`,
//! `Color3`, `UDim2`, `CFrame`, or `EnumItem` as `"Material.Neon"`).
//!
//! [`write_world`] saves every property and attribute, sorted by name, so
//! saving the same world always produces the same text.

use std::sync::Arc;

use axcenity_core::prefab::{Overrides, PrefabLink, Segment, child_segments, overrides};
use axcenity_core::{
    CFrame, ClassName, EnumItem, InstanceId, Material, PartType, ParticleType, TextureChannel,
    ToneMapping, Value, ValueKind, World,
};
use serde::Deserialize;
use serde_json::{Map, Value as Json};

/// `world.json` and prefab files. The manifest version is separate: a project
/// upgrade can leave these files unchanged. This is the newest version read.
const WORLD_FORMAT_VERSION: u32 = SCRIPTS_FORMAT_VERSION;
/// What a file without scripts says, so those files stay as they were
/// before scripts inside objects (G2).
const PLAIN_FORMAT_VERSION: u32 = 1;
/// What a file holding a script node says: an older engine then asks to be
/// updated instead of refusing the script.
const SCRIPTS_FORMAT_VERSION: u32 = 2;
/// Editor-only switches that are off nearly everywhere.
const WRITTEN_ONLY_WHEN_ON: [&str; 2] = ["Locked", "BlueprintTexture"];

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    /// The text isn't valid JSON, or doesn't have the expected fields.
    /// The message includes the line and column.
    #[error(transparent)]
    Json(serde_json::Error),
    #[error(
        "world format {found} is newer than this engine understands ({WORLD_FORMAT_VERSION}); update Axcenity to open it"
    )]
    TooNew { found: u32 },
    #[error("{path}: there is no class named `{class}`")]
    UnknownClass { path: String, class: String },
    #[error("{path}: {class} is a service and can't be listed as a child")]
    Service { path: String, class: ClassName },
    #[error(
        "{path}: a script in the world file needs a `SourceFile` naming the file its code is in (such as `objects/Door/Open.server.lua`); a script in a service is a file in the `scripts` folder instead (`.server.lua` for a Script, `.client.lua` for a LocalScript, `.module.lua` for a ModuleScript)"
    )]
    Script { path: String },
    #[error("{path}: Players are made when someone joins the game, so they can't be saved")]
    Player { path: String },
    #[error("{path}: {class} has no property named `{property}`")]
    UnknownProperty {
        path: String,
        class: ClassName,
        property: String,
    },
    #[error("{path}.{property}: {message}")]
    InvalidProperty {
        path: String,
        property: String,
        message: String,
    },
    #[error("{path}: attribute `{attribute}`: {message}")]
    InvalidAttribute {
        path: String,
        attribute: String,
        message: String,
    },
}

// `deny_unknown_fields` turns typos such as "childern" into errors that name
// the line, instead of being silently ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldFile {
    format_version: u32,
    #[serde(default)]
    workspace: ServiceNode,
    #[serde(default)]
    replicated_storage: ServiceNode,
    // Phase 64: what only the server uses.
    #[serde(default)]
    server_storage: ServiceNode,
    #[serde(default)]
    lighting: ServiceNode,
    // Phase 47: spawning settings, the StarterCharacter, and the Teams.
    #[serde(default)]
    players: ServiceNode,
    #[serde(default)]
    starter_player: ServiceNode,
    #[serde(default)]
    teams: ServiceNode,
    // Phase 52: the SoundGroups.
    #[serde(default)]
    sound_service: ServiceNode,
    // Phase 67: the default chat's settings.
    #[serde(default)]
    text_chat_service: ServiceNode,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceNode {
    #[serde(default)]
    properties: Map<String, Json>,
    #[serde(default)]
    children: Vec<InstanceNode>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstanceNode {
    class: String,
    /// Defaults to the class name.
    name: Option<String>,
    #[serde(default)]
    properties: Map<String, Json>,
    #[serde(default)]
    attributes: Map<String, Json>,
    #[serde(default)]
    tags: Vec<String>,
    /// The prefab this is a placed copy of (Phase 49).
    prefab: Option<String>,
    /// Which of that prefab's top-level Instances, when it has several.
    #[serde(default)]
    prefab_index: usize,
    /// Inside a copy: added by the copy, not from its prefab.
    #[serde(default)]
    added: bool,
    /// Inside a copy: what it sets differently from its prefab.
    #[serde(default)]
    overrides: Vec<String>,
    /// Inside a copy: the prefab's children it took out.
    #[serde(default)]
    removed: Vec<SegmentNode>,
    #[serde(default)]
    children: Vec<InstanceNode>,
}

/// A child of a prefab by name: `"Wheel"` is the first named Wheel,
/// `["Wheel", 2]` the third.
#[derive(Deserialize)]
#[serde(untagged)]
enum SegmentNode {
    First(String),
    Nth(String, usize),
}

impl SegmentNode {
    fn segment(&self) -> Segment {
        match self {
            Self::First(name) => Segment::new(name.clone(), 0),
            Self::Nth(name, nth) => Segment::new(name.clone(), *nth),
        }
    }
}

/// The overrides names that aren't properties: the Instance's attributes
/// and tags are the copy's own.
const ATTRIBUTES_OVERRIDE: &str = "attributes";
const TAGS_OVERRIDE: &str = "tags";

/// Where an Instance being read or written is, for prefab copies.
#[derive(Clone, Copy)]
enum Region<'a> {
    /// Not inside a copy (or added to one).
    Outside,
    /// Part of a copy, with its overrides (`None`: it has none).
    Inside(Option<&'a Overrides>),
}

/// An `ObjectValue` (or other Instance property) read from a file, waiting
/// for every Instance to exist before its path is looked up.
struct Reference {
    id: InstanceId,
    property: String,
    path: Vec<String>,
}

/// Points each of `references` at the Instance its path names, starting
/// from one of `roots` by name. A path that names nothing stays `nil`.
fn resolve(world: &mut World, roots: &[InstanceId], references: Vec<Reference>) {
    for reference in references {
        let Some((first, rest)) = reference.path.split_first() else {
            continue;
        };
        let mut target = roots
            .iter()
            .copied()
            .find(|&root| world.get(root).is_some_and(|r| r.name() == first));
        for name in rest {
            target = target.and_then(|parent| world.find_first_child(parent, name));
        }
        if let Some(target) = target {
            let _ = world.set_property(
                reference.id,
                &reference.property,
                Value::Instance(Some(target)),
            );
        }
    }
}

/// The path of names from one of `roots` (included) down to `target`, as
/// JSON, or `null` when `target` is none, gone, or not inside any of them.
fn reference_json(world: &World, target: Option<InstanceId>, roots: &[InstanceId]) -> String {
    let mut names = Vec::new();
    let mut current = target;
    while let Some(id) = current {
        let Some(instance) = world.get(id) else {
            break;
        };
        names.push(json_string(instance.name()));
        if roots.contains(&id) {
            names.reverse();
            return format!("[{}]", names.join(", "));
        }
        current = instance.parent();
    }
    "null".to_owned()
}

/// The Instances a world file's references start from: the services.
fn world_roots(world: &World) -> Vec<InstanceId> {
    vec![
        world.workspace(),
        world.replicated_storage(),
        world.server_storage(),
        world.sound_service(),
    ]
}

/// Builds a [`World`] from the text of a world file.
pub fn parse_world(text: &str) -> Result<World, WorldError> {
    let file: WorldFile = serde_json::from_str(text).map_err(WorldError::Json)?;
    if file.format_version > WORLD_FORMAT_VERSION {
        return Err(WorldError::TooNew {
            found: file.format_version,
        });
    }

    let mut world = World::new();
    let mut references = Vec::new();
    let services = [
        (world.workspace(), &file.workspace),
        (world.replicated_storage(), &file.replicated_storage),
        (world.server_storage(), &file.server_storage),
        (world.lighting(), &file.lighting),
        (world.players(), &file.players),
        (world.starter_player(), &file.starter_player),
        (world.teams(), &file.teams),
        (world.sound_service(), &file.sound_service),
        (world.text_chat_service(), &file.text_chat_service),
    ];
    for (service, node) in services {
        apply_properties(&mut world, service, &node.properties, &mut references)?;
        for child in &node.children {
            add_instance(&mut world, service, child, &mut references, None)?;
        }
    }
    let roots = world_roots(&world);
    resolve(&mut world, &roots, references);
    Ok(world)
}

/// A prefab file (`assets/prefabs/*.prefab.json`): Instances saved to be
/// placed again, written like the world file's children.
///
/// ```json
/// { "format_version": 1, "instances": [ { "class": "Model", "name": "Lamp", "children": [] } ] }
/// ```
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrefabFile {
    format_version: u32,
    #[serde(default)]
    instances: Vec<InstanceNode>,
}

/// Adds the Instances of a prefab file's text to `world`, inside `parent`,
/// and returns the top-level ones in file order. Nothing is added if the
/// text has any error.
pub fn parse_prefab(
    text: &str,
    world: &mut World,
    parent: InstanceId,
) -> Result<Vec<InstanceId>, WorldError> {
    let file: PrefabFile = serde_json::from_str(text).map_err(WorldError::Json)?;
    if file.format_version > WORLD_FORMAT_VERSION {
        return Err(WorldError::TooNew {
            found: file.format_version,
        });
    }
    // Checked in a world of its own first, so a bad file leaves `world` alone.
    let mut check = World::new();
    let workspace = check.workspace();
    for node in &file.instances {
        add_instance(&mut check, workspace, node, &mut Vec::new(), None)?;
    }
    let mut added = Vec::new();
    let mut references = Vec::new();
    for node in &file.instances {
        added.push(add_instance(world, parent, node, &mut references, None)?);
    }
    resolve(world, &added, references);
    Ok(added)
}

/// The text of a prefab file holding `ids` and everything inside them
/// (scripts are left out as in the world file, unless they're inside
/// objects).
pub fn write_prefab(world: &World, ids: &[InstanceId]) -> String {
    let ids: Vec<InstanceId> = ids
        .iter()
        .copied()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|i| saved(i) && !i.class().is_service())
        })
        .collect();
    let version = format_version(world, &ids);
    let mut out = format!("{{\n  \"format_version\": {version},\n  \"instances\": [\n");
    for (i, &id) in ids.iter().enumerate() {
        out.push_str("    ");
        write_instance(world, id, 2, &ids, Region::Outside, false, &mut out);
        if i + 1 < ids.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

/// Creates the Instance described by `node` under `parent`, then its children.
/// `region` is the overrides of `parent` when it is part of a prefab copy.
fn add_instance(
    world: &mut World,
    parent: InstanceId,
    node: &InstanceNode,
    references: &mut Vec<Reference>,
    region: Option<&mut Overrides>,
) -> Result<InstanceId, WorldError> {
    let parent_path = world.full_name(parent).expect("parent exists");
    let name = node.name.as_deref().unwrap_or(&node.class);
    let path = format!("{parent_path}.{name}");

    let class = ClassName::from_name(&node.class).ok_or_else(|| WorldError::UnknownClass {
        path: path.clone(),
        class: node.class.clone(),
    })?;
    if class == ClassName::Player {
        return Err(WorldError::Player { path });
    }
    let id = world
        .create(class)
        .map_err(|_| WorldError::Service { path, class })?;
    world.set_name(id, name).expect("just created");
    world.set_parent(id, Some(parent)).expect("both exist");

    apply_properties(world, id, &node.properties, references)?;
    // A script in a file is one inside an object (G2): its code is in the
    // file it names.
    if class.is_script() && world.get(id).and_then(|i| i.source_file()).is_none() {
        return Err(WorldError::Script {
            path: world.full_name(id).unwrap_or_default(),
        });
    }
    for (attribute, json) in &node.attributes {
        let result = json_to_attribute(json).and_then(|value| {
            world
                .set_attribute(id, attribute, Some(value))
                .map_err(|err| err.to_string())
        });
        if let Err(message) = result {
            return Err(WorldError::InvalidAttribute {
                path: world.full_name(id).unwrap_or_default(),
                attribute: attribute.clone(),
                message,
            });
        }
    }
    for tag in &node.tags {
        world.add_tag(id, tag).expect("just created");
    }

    // Prefab copies (Phase 49): this node's overrides go in its parent's,
    // unless it was added to the copy; a link outside a copy starts one.
    let segment = axcenity_core::prefab::segment_of(world, id).expect("it has a parent");
    let mut starts = None;
    let mut here = match region {
        Some(parent) if node.added => {
            parent.added.push(segment);
            None
        }
        Some(parent) => Some(parent.children.entry(segment).or_default()),
        None => None,
    };
    if here.is_none() && node.prefab.is_some() {
        here = Some(starts.insert(Overrides::default()));
    }
    if let Some(here) = here.as_deref_mut() {
        for name in &node.overrides {
            match name.as_str() {
                ATTRIBUTES_OVERRIDE => here.attributes = true,
                TAGS_OVERRIDE => here.tags = true,
                name if world.get(id).and_then(|i| i.property(name)).is_some() => {
                    here.properties.insert(name.to_owned());
                }
                _ => {
                    return Err(WorldError::UnknownProperty {
                        path: world.full_name(id).unwrap_or_default(),
                        class,
                        property: name.clone(),
                    });
                }
            }
        }
        here.removed
            .extend(node.removed.iter().map(SegmentNode::segment));
    }
    for child in &node.children {
        add_instance(world, id, child, references, here.as_deref_mut())?;
    }
    if let Some(prefab) = &node.prefab {
        let mut link = PrefabLink::new(prefab.clone(), node.prefab_index);
        link.pending = starts.map(|mut overrides| {
            overrides.prune();
            Arc::new(overrides)
        });
        world.set_prefab(id, Some(link)).expect("just created");
    }
    Ok(id)
}

/// Reads an attribute's value (see the module docs).
fn json_to_attribute(json: &Json) -> Result<Value, String> {
    const EXPECTED: &str = "expected true or false, a number, a string, or an object with one \
         field: Vector3, Vector2, Color3, UDim2, CFrame or EnumItem";
    match json {
        Json::Bool(b) => Ok(Value::Bool(*b)),
        Json::Number(n) => n.as_f64().map(Value::Number).ok_or(EXPECTED.to_owned()),
        Json::String(s) => Ok(Value::String(s.clone())),
        Json::Object(fields) if fields.len() == 1 => {
            let (kind, inner) = fields.iter().next().expect("one field");
            let kind = match kind.as_str() {
                "Vector3" => ValueKind::Vector3,
                "Vector2" => ValueKind::Vector2,
                "Color3" => ValueKind::Color3,
                "UDim2" => ValueKind::UDim2,
                "CFrame" => ValueKind::CFrame,
                "EnumItem" => ValueKind::EnumItem,
                _ => return Err(EXPECTED.to_owned()),
            };
            json_to_value(inner, kind).map_err(|message| format!("{kind}: {message}"))
        }
        _ => Err(EXPECTED.to_owned()),
    }
}

/// An attribute's value as JSON (see the module docs).
fn attribute_json(value: &Value) -> String {
    let kind = match value {
        Value::Bool(_) | Value::Number(_) | Value::String(_) => return json_value(value),
        other => other.kind(),
    };
    format!(
        "{{ {}: {} }}",
        json_string(&kind.to_string()),
        json_value(value)
    )
}

fn apply_properties(
    world: &mut World,
    id: InstanceId,
    properties: &Map<String, Json>,
    references: &mut Vec<Reference>,
) -> Result<(), WorldError> {
    // `Material` goes first: setting it resets `Metalness` and `Roughness` to
    // its preset, and any values the file gives for those must win.
    let (material, others): (Vec<_>, Vec<_>) = properties
        .iter()
        .partition(|(property, _)| property.as_str() == "Material");

    for (property, json) in material.into_iter().chain(others) {
        let instance = world.get(id).expect("caller passes a live Instance");
        let path = world.full_name(id).expect("caller passes a live Instance");
        let Some(current) = instance.property(property) else {
            return Err(WorldError::UnknownProperty {
                path,
                class: instance.class(),
                property: property.clone(),
            });
        };

        if current.kind() == ValueKind::Instance {
            // Looked up once every Instance exists (see `resolve`).
            match json {
                Json::Null => {}
                Json::Array(names) if names.iter().all(Json::is_string) => {
                    references.push(Reference {
                        id,
                        property: property.clone(),
                        path: names
                            .iter()
                            .filter_map(Json::as_str)
                            .map(str::to_owned)
                            .collect(),
                    });
                }
                _ => {
                    return Err(WorldError::InvalidProperty {
                        path,
                        property: property.clone(),
                        message:
                            "expected a path of names, such as [\"Workspace\", \"Door\"], or null"
                                .to_owned(),
                    });
                }
            }
            continue;
        }
        let value =
            json_to_value(json, current.kind()).map_err(|message| WorldError::InvalidProperty {
                path,
                property: property.clone(),
                message,
            })?;
        world
            .set_property(id, property, value)
            .expect("value was converted to the property's own type");
    }
    Ok(())
}

/// Reads `json` as a value of `kind`, or explains what was expected.
pub(crate) fn json_to_value(json: &Json, kind: ValueKind) -> Result<Value, String> {
    match kind {
        ValueKind::Bool => json
            .as_bool()
            .map(Value::Bool)
            .ok_or_else(|| "expected true or false".to_owned()),
        ValueKind::Number => json
            .as_f64()
            .map(Value::Number)
            .ok_or_else(|| "expected a number".to_owned()),
        ValueKind::String => json
            .as_str()
            .map(|s| Value::String(s.to_owned()))
            .ok_or_else(|| "expected a string in quotes".to_owned()),
        ValueKind::Vector3 => {
            let [x, y, z] = three_numbers(json).ok_or("expected [x, y, z]: three numbers")?;
            Ok(Value::Vector3(axcenity_core::Vector3::new(x, y, z)))
        }
        ValueKind::Vector2 => match json.as_array().map(Vec::as_slice) {
            Some([x, y]) => match (x.as_f64(), y.as_f64()) {
                (Some(x), Some(y)) => Ok(Value::Vector2(axcenity_core::Vector2::new(
                    x as f32, y as f32,
                ))),
                _ => Err("expected [x, y]: two numbers".to_owned()),
            },
            _ => Err("expected [x, y]: two numbers".to_owned()),
        },
        ValueKind::UDim2 => {
            let [x_scale, x_offset, y_scale, y_offset] =
                four_numbers(json).ok_or("expected [x_scale, x_offset, y_scale, y_offset]")?;
            Ok(Value::UDim2(axcenity_core::UDim2::new(
                x_scale, x_offset, y_scale, y_offset,
            )))
        }
        ValueKind::Color3 => {
            const EXPECTED: &str = "expected [r, g, b]: three numbers from 0 to 1";
            let [r, g, b] = three_numbers(json).ok_or(EXPECTED)?;
            if [r, g, b].iter().any(|c| *c > 1.0) {
                return Err(format!("{EXPECTED} (for 0-255 values, divide each by 255)"));
            }
            if [r, g, b].iter().any(|c| *c < 0.0) {
                return Err(EXPECTED.to_owned());
            }
            Ok(Value::Color3(axcenity_core::Color3::new(r, g, b)))
        }
        ValueKind::PartType => {
            let names = PartType::ALL.map(PartType::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::PartType(PartType::from_name(name).expect("checked")))
        }
        ValueKind::Material => {
            let names = Material::ALL.map(Material::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::Material(Material::from_name(name).expect("checked")))
        }
        ValueKind::ParticleType => {
            let names = ParticleType::ALL.map(ParticleType::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::ParticleType(
                ParticleType::from_name(name).expect("checked"),
            ))
        }
        ValueKind::TextureChannel => {
            let names = TextureChannel::ALL.map(TextureChannel::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::TextureChannel(
                TextureChannel::from_name(name).expect("checked"),
            ))
        }
        ValueKind::ToneMapping => {
            let names = ToneMapping::ALL.map(ToneMapping::as_str);
            let name = enum_name(json, &names)?;
            Ok(Value::ToneMapping(
                ToneMapping::from_name(name).expect("checked"),
            ))
        }
        ValueKind::CFrame => {
            const EXPECTED: &str = "expected [x, y, z, qx, qy, qz, qw]: a position and a rotation";
            let numbers: Option<Vec<f32>> = json
                .as_array()
                .filter(|items| items.len() == 7)
                .and_then(|items| items.iter().map(|n| n.as_f64().map(|n| n as f32)).collect());
            let numbers = numbers.ok_or(EXPECTED)?;
            let length = numbers[3..].iter().map(|n| n * n).sum::<f32>().sqrt();
            if length < 1e-6 {
                return Err(format!("{EXPECTED} (the rotation can't be all zeros)"));
            }
            let rotation = [3, 4, 5, 6].map(|i| numbers[i] / length);
            Ok(Value::CFrame(CFrame {
                position: axcenity_core::Vector3::new(numbers[0], numbers[1], numbers[2]),
                rotation,
            }))
        }
        ValueKind::EnumItem => {
            const EXPECTED: &str = "expected \"Enum.Item\", such as \"Material.Neon\"";
            let (enum_name, item) = json
                .as_str()
                .and_then(|text| text.split_once('.'))
                .filter(|(e, i)| !e.is_empty() && !i.is_empty())
                .ok_or(EXPECTED)?;
            Ok(Value::EnumItem(EnumItem {
                enum_name: enum_name.to_owned(),
                item: item.to_owned(),
            }))
        }
        // Read by `apply_properties` as a path.
        ValueKind::Instance => Err("expected a path of names".to_owned()),
        ValueKind::PhysicalProperties => physical_properties(json),
    }
}

/// The names of a `PhysicalProperties`' numbers in a file, in the order
/// `PhysicalProperties.new` takes them.
const PHYSICAL_FIELDS: [&str; 5] = [
    "Density",
    "Friction",
    "Elasticity",
    "FrictionWeight",
    "ElasticityWeight",
];

fn physical_properties(json: &Json) -> Result<Value, String> {
    const EXPECTED: &str = "expected null or { \"Density\": 1, \"Friction\": 0.5, \
         \"Elasticity\": 0.25, \"FrictionWeight\": 1, \"ElasticityWeight\": 1 }";
    let fields = match json {
        Json::Null => return Ok(Value::PhysicalProperties(None)),
        Json::Object(fields) => fields,
        _ => return Err(EXPECTED.to_owned()),
    };
    if let Some(unknown) = fields
        .keys()
        .find(|key| !PHYSICAL_FIELDS.contains(&key.as_str()))
    {
        return Err(format!("{EXPECTED} (there is no \"{unknown}\")"));
    }
    let mut numbers = axcenity_core::PhysicalProperties::LEGACY.to_array();
    for (number, field) in numbers.iter_mut().zip(PHYSICAL_FIELDS) {
        if let Some(value) = fields.get(field) {
            *number = value
                .as_f64()
                .ok_or_else(|| format!("{EXPECTED} ({field} must be a number)"))?
                as f32;
        }
    }
    Ok(Value::PhysicalProperties(Some(
        axcenity_core::PhysicalProperties::from_array(numbers).clamped(),
    )))
}

/// Reads `json` as one of `names`, or explains which names are allowed.
fn enum_name<'a>(json: &'a Json, names: &[&str]) -> Result<&'a str, String> {
    match json.as_str() {
        Some(name) if names.contains(&name) => Ok(name),
        _ => Err(format!("expected one of: \"{}\"", names.join("\", \""))),
    }
}

fn three_numbers(json: &Json) -> Option<[f32; 3]> {
    match json.as_array()?.as_slice() {
        [a, b, c] => Some([a.as_f64()? as f32, b.as_f64()? as f32, c.as_f64()? as f32]),
        _ => None,
    }
}

fn four_numbers(json: &Json) -> Option<[f32; 4]> {
    match json.as_array()?.as_slice() {
        [a, b, c, d] => Some([
            a.as_f64()? as f32,
            b.as_f64()? as f32,
            c.as_f64()? as f32,
            d.as_f64()? as f32,
        ]),
        _ => None,
    }
}

/// The text of a world file for `world`, in the same layout people write by
/// hand: two-space indents, and `[x, y, z]` values on one line.
pub fn write_world(world: &World) -> String {
    let roots = world_roots(world);
    let version = format_version(world, &roots);
    let mut out = format!("{{\n  \"format_version\": {version},\n  \"workspace\": ");
    write_instance(
        world,
        world.workspace(),
        1,
        &roots,
        Region::Outside,
        false,
        &mut out,
    );
    // Left out when empty, so worlds without one look as they always have.
    let storages = [
        ("replicated_storage", world.replicated_storage()),
        ("server_storage", world.server_storage()),
    ];
    for (key, storage) in storages {
        if !saved_children(world, storage).is_empty() {
            out.push_str(&format!(",\n  \"{key}\": "));
            write_instance(world, storage, 1, &roots, Region::Outside, false, &mut out);
        }
    }
    // Each left out while every setting is its default and nothing is
    // inside, for the same reason.
    let services = [
        ("lighting", world.lighting()),
        ("players", world.players()),
        ("starter_player", world.starter_player()),
        ("teams", world.teams()),
        ("sound_service", world.sound_service()),
        ("text_chat_service", world.text_chat_service()),
    ];
    for (key, service) in services {
        let changed = world.get(service).is_some_and(|instance| {
            let defaults = instance.class().default_properties();
            instance
                .properties()
                .any(|(name, value)| defaults.iter().any(|(n, v)| *n == name && v != value))
        });
        if changed || !saved_children(world, service).is_empty() {
            out.push_str(&format!(",\n  \"{key}\": "));
            write_instance(world, service, 1, &roots, Region::Outside, false, &mut out);
        }
    }
    out.push_str("\n}\n");
    out
}

/// The text of a level file (`levels/<name>.world.json`): `world`'s
/// Workspace only, written as in [`write_world`].
pub fn write_workspace_only(world: &World) -> String {
    let roots = world_roots(world);
    let version = format_version(world, &[world.workspace()]);
    let mut out = format!("{{\n  \"format_version\": {version},\n  \"workspace\": ");
    write_instance(
        world,
        world.workspace(),
        1,
        &roots,
        Region::Outside,
        false,
        &mut out,
    );
    out.push_str("\n}\n");
    out
}

/// The `format_version` a file holding `ids` (and what's inside them) says:
/// 2 if it holds a script node, otherwise 1.
fn format_version(world: &World, ids: &[InstanceId]) -> u32 {
    fn holds_script(world: &World, id: InstanceId) -> bool {
        world.get(id).is_some_and(|i| i.class().is_script())
            || saved_children(world, id)
                .into_iter()
                .any(|child| holds_script(world, child))
    }
    if ids.iter().any(|&id| holds_script(world, id)) {
        SCRIPTS_FORMAT_VERSION
    } else {
        PLAIN_FORMAT_VERSION
    }
}

/// Whether a file holds this Instance: a script only when it's inside an
/// object (its code is in the `SourceFile` it names); the scripts made from
/// `scripts/` are files of their own.
fn saved(instance: &axcenity_core::Instance) -> bool {
    !instance.class().is_script() || instance.source_file().is_some()
}

/// The children of `id` that go in the world file. Scripts from `scripts/`
/// are saved as their own `.lua` files, Players only exist while the game runs, a
/// service inside another (StarterPlayerScripts) is always there, and what
/// a SplineMesh or SplineScatter made is made again from its settings
/// (Phase 74).
fn saved_children(world: &World, id: InstanceId) -> Vec<InstanceId> {
    world.get(id).map_or_else(Vec::new, |instance| {
        if axcenity_core::spline::is_generator(instance.class()) {
            return Vec::new();
        }
        instance
            .children()
            .iter()
            .copied()
            .filter(|&child| {
                world.get(child).is_some_and(|c| {
                    saved(c) && !c.class().is_service() && c.class() != ClassName::Player
                })
            })
            .collect()
    })
}

/// Appends `id` as a JSON object whose closing brace is indented `depth`
/// levels. References are written as paths from one of `roots`. `region`
/// says whether it is part of a prefab copy, and `added` that it was added
/// to one.
#[allow(clippy::too_many_arguments)]
fn write_instance(
    world: &World,
    id: InstanceId,
    depth: usize,
    roots: &[InstanceId],
    region: Region<'_>,
    added: bool,
    out: &mut String,
) {
    let instance = world.get(id).expect("only live Instances are in the tree");
    let indent = "  ".repeat(depth + 1);
    let inner_indent = "  ".repeat(depth + 2);
    let mut fields = Vec::new();

    // A service's class and name are fixed, so the file doesn't repeat them.
    if !instance.class().is_service() {
        fields.push(format!(
            "{indent}\"class\": {}",
            json_string(instance.class().as_str())
        ));
        fields.push(format!(
            "{indent}\"name\": {}",
            json_string(instance.name())
        ));
    }

    // Prefab copies (Phase 49): a link outside a copy starts one, whose
    // overrides are written on the Instances they belong to.
    let started;
    let region = match (region, instance.prefab()) {
        (Region::Outside, Some(_)) => {
            started = overrides(world, id);
            Region::Inside(Some(&started))
        }
        (region, _) => region,
    };
    if let Some(link) = instance.prefab() {
        fields.push(format!("{indent}\"prefab\": {}", json_string(&link.path)));
        if link.index > 0 {
            fields.push(format!("{indent}\"prefab_index\": {}", link.index));
        }
    }
    if added {
        fields.push(format!("{indent}\"added\": true"));
    }
    if let Region::Inside(Some(here)) = region {
        let mut names: Vec<String> = here.properties.iter().map(|n| json_string(n)).collect();
        if here.attributes {
            names.push(json_string(ATTRIBUTES_OVERRIDE));
        }
        if here.tags {
            names.push(json_string(TAGS_OVERRIDE));
        }
        if !names.is_empty() {
            fields.push(format!("{indent}\"overrides\": [{}]", names.join(", ")));
        }
        if !here.removed.is_empty() {
            let removed: Vec<String> = here.removed.iter().map(segment_json).collect();
            fields.push(format!("{indent}\"removed\": [{}]", removed.join(", ")));
        }
    }

    // The Workspace leaves out its defaults (see the module docs).
    let defaults = if instance.class() == ClassName::Workspace {
        instance.class().default_properties()
    } else {
        Vec::new()
    };
    let properties: Vec<String> = instance
        .properties()
        .filter(|(name, value)| !defaults.iter().any(|(n, v)| n == name && v == *value))
        .filter(|(name, value)| {
            !(WRITTEN_ONLY_WHEN_ON.contains(name) && **value == Value::Bool(false))
        })
        .map(|(name, value)| {
            let json = match value {
                Value::Instance(target) => reference_json(world, *target, roots),
                value => json_value(value),
            };
            format!("{inner_indent}{}: {json}", json_string(name))
        })
        .collect();
    if !properties.is_empty() {
        fields.push(format!(
            "{indent}\"properties\": {{\n{}\n{indent}}}",
            properties.join(",\n")
        ));
    }

    // Services keep neither (the file has nowhere for them).
    if !instance.class().is_service() {
        let attributes: Vec<String> = instance
            .attributes()
            .iter()
            .map(|(name, value)| {
                format!(
                    "{inner_indent}{}: {}",
                    json_string(name),
                    attribute_json(value)
                )
            })
            .collect();
        if !attributes.is_empty() {
            fields.push(format!(
                "{indent}\"attributes\": {{\n{}\n{indent}}}",
                attributes.join(",\n")
            ));
        }
        if !instance.tags().is_empty() {
            let tags: Vec<String> = instance.tags().iter().map(|t| json_string(t)).collect();
            fields.push(format!("{indent}\"tags\": [{}]", tags.join(", ")));
        }
    }

    let children = saved_children(world, id);
    if !children.is_empty() {
        let segments = child_segments(world, id);
        let mut text = format!("{indent}\"children\": [\n");
        for (i, &child) in children.iter().enumerate() {
            let (inner, added) = match region {
                Region::Outside => (Region::Outside, false),
                Region::Inside(here) => {
                    let segment = segments
                        .iter()
                        .find(|(_, id)| *id == child)
                        .map(|(segment, _)| segment);
                    let here = here.zip(segment);
                    match here {
                        Some((here, segment)) if here.added.contains(segment) => {
                            (Region::Outside, true)
                        }
                        Some((here, segment)) => {
                            (Region::Inside(here.children.get(segment)), false)
                        }
                        None => (Region::Inside(None), false),
                    }
                }
            };
            text.push_str(&inner_indent);
            write_instance(world, child, depth + 2, roots, inner, added, &mut text);
            if i + 1 < children.len() {
                text.push(',');
            }
            text.push('\n');
        }
        text.push_str(&format!("{indent}]"));
        fields.push(text);
    }

    if fields.is_empty() {
        out.push_str("{}");
    } else {
        out.push_str(&format!(
            "{{\n{}\n{}}}",
            fields.join(",\n"),
            "  ".repeat(depth)
        ));
    }
}

/// A child of a prefab as the `removed` list writes it (see [`SegmentNode`]).
fn segment_json(segment: &Segment) -> String {
    match segment.nth {
        0 => json_string(&segment.name),
        nth => format!("[{}, {nth}]", json_string(&segment.name)),
    }
}

fn json_value(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => json_number(*n),
        Value::String(s) => json_string(s),
        Value::Vector3(v) => json_triple([v.x, v.y, v.z]),
        Value::Vector2(v) => {
            let [x, y] = [v.x, v.y].map(|n| {
                if n.is_finite() {
                    n.to_string()
                } else {
                    "null".to_owned()
                }
            });
            format!("[{x}, {y}]")
        }
        Value::UDim2(v) => json_quad([v.x.scale, v.x.offset, v.y.scale, v.y.offset]),
        Value::Color3(c) => json_triple([c.r, c.g, c.b]),
        Value::PartType(t) => json_string(t.as_str()),
        Value::Material(m) => json_string(m.as_str()),
        Value::TextureChannel(c) => json_string(c.as_str()),
        Value::ParticleType(t) => json_string(t.as_str()),
        Value::ToneMapping(t) => json_string(t.as_str()),
        Value::CFrame(c) => {
            let p = c.position;
            let numbers = [
                p.x,
                p.y,
                p.z,
                c.rotation[0],
                c.rotation[1],
                c.rotation[2],
                c.rotation[3],
            ];
            // As `f32`s, for the same reason as `json_triple`.
            let numbers: Vec<String> = numbers
                .iter()
                .map(|n| {
                    if n.is_finite() {
                        n.to_string()
                    } else {
                        "null".to_owned()
                    }
                })
                .collect();
            format!("[{}]", numbers.join(", "))
        }
        Value::EnumItem(item) => json_string(&format!("{}.{}", item.enum_name, item.item)),
        // Written by `write_instance` as a path; attributes never hold one.
        Value::Instance(_) => "null".to_owned(),
        Value::PhysicalProperties(None) => "null".to_owned(),
        Value::PhysicalProperties(Some(physical)) => {
            let fields: Vec<String> = PHYSICAL_FIELDS
                .iter()
                .zip(physical.to_array())
                // As `f32`s, for the same reason as `json_triple`. Kept
                // in range, so never NaN or infinite.
                .map(|(field, n)| format!("\"{field}\": {n}"))
                .collect();
            format!("{{ {} }}", fields.join(", "))
        }
    }
}

/// Rust prints floats in the shortest form that reads back as the same
/// number, without a trailing `.0` for whole numbers, which is valid JSON.
/// JSON has no NaN or infinity, so those are written as `null`; loading the
/// file then reports the bad value instead of silently changing it.
fn json_number(n: f64) -> String {
    if n.is_finite() {
        n.to_string()
    } else {
        "null".to_owned()
    }
}

/// Like [`json_number`], for each of three `f32`s. They're printed as `f32`
/// on purpose: widening `0.45_f32` to `f64` first would print 0.44999998807907715.
fn json_triple(numbers: [f32; 3]) -> String {
    let [a, b, c] = numbers.map(|n| {
        if n.is_finite() {
            n.to_string()
        } else {
            "null".to_owned()
        }
    });
    format!("[{a}, {b}, {c}]")
}

fn json_quad(numbers: [f32; 4]) -> String {
    let [a, b, c, d] = numbers.map(|n| {
        if n.is_finite() {
            n.to_string()
        } else {
            "null".to_owned()
        }
    });
    format!("[{a}, {b}, {c}, {d}]")
}

/// A JSON string literal, with quotes and any needed escapes.
fn json_string(s: &str) -> String {
    serde_json::to_string(s).expect("a string always converts to JSON")
}

#[cfg(test)]
mod tests;
