//! World state in a save: every Instance in the Workspace and Lighting, as
//! its class, its name and the properties that differ from its class's
//! defaults, and the services' own properties.
//!
//! Restoring matches each saved Instance to a live one with the same parent,
//! class and name (the second "Coin" to the second "Coin"), sets its
//! properties, makes the saved ones that are missing and destroys the live
//! ones the save doesn't have. Matched Instances keep their `InstanceId`,
//! so a script that holds one still has it after a load.
//!
//! Scripts, Players and ScreenGuis (with everything inside them) are not
//! world state: they're never saved, matched, made or destroyed. A Terrain's
//! landscape isn't saved, only its properties.

use std::collections::{BTreeMap, HashMap};

use axcenity_core::{
    ClassName, Color3, InstanceId, Material, PartType, ParticleType, TextureChannel, ToneMapping,
    UDim2, Value, Vector2, Vector3, World,
};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The services whose contents are world state, in the order they're saved.
const SERVICES: [ClassName; 2] = [ClassName::Workspace, ClassName::Lighting];

/// A property's name and its saved value.
pub type Properties = BTreeMap<String, SavedValue>;

/// The world as it was when it was saved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldState {
    /// Each service's own properties that differ from their defaults, by
    /// the service's class name.
    #[serde(default)]
    pub services: BTreeMap<String, Properties>,
    /// Every saved Instance, each parent before its children.
    #[serde(default)]
    pub instances: Vec<SavedInstance>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedInstance {
    pub parent: SavedParent,
    pub class: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: Properties,
}

/// Where a saved Instance was: in a service, or in an earlier entry of
/// [`WorldState::instances`]. In the file it is the service's class name
/// (`"Workspace"`) or the entry's number.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SavedParent {
    Index(usize),
    Service(String),
}

/// A property value as a save file holds it: booleans, numbers and strings
/// as themselves, everything else as an object naming its type, such as
/// `{"Vector3": [0, 5, 0]}` or `{"Material": "Neon"}`.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedValue(pub Value);

impl Serialize for SavedValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        to_json(&self.0).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SavedValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let json = serde_json::Value::deserialize(deserializer)?;
        from_json(&json)
            .map(SavedValue)
            .ok_or_else(|| D::Error::custom(format!("{json} is not a value a save can hold")))
    }
}

fn to_json(value: &Value) -> serde_json::Value {
    use serde_json::json;
    match value {
        Value::Bool(b) => json!(b),
        Value::Number(n) => json!(n),
        Value::String(s) => json!(s),
        Value::Vector3(v) => json!({ "Vector3": [v.x, v.y, v.z] }),
        Value::Vector2(v) => json!({ "Vector2": [v.x, v.y] }),
        Value::UDim2(u) => json!({ "UDim2": [u.x.scale, u.x.offset, u.y.scale, u.y.offset] }),
        Value::Color3(c) => json!({ "Color3": [c.r, c.g, c.b] }),
        Value::PartType(t) => json!({ "PartType": t.as_str() }),
        Value::Material(m) => json!({ "Material": m.as_str() }),
        Value::TextureChannel(c) => json!({ "TextureChannel": c.as_str() }),
        Value::ParticleType(t) => json!({ "ParticleType": t.as_str() }),
        Value::ToneMapping(t) => json!({ "ToneMapping": t.as_str() }),
    }
}

fn from_json(json: &serde_json::Value) -> Option<Value> {
    use serde_json::Value as Json;
    match json {
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => n.as_f64().map(Value::Number),
        Json::String(s) => Some(Value::String(s.clone())),
        Json::Object(object) if object.len() == 1 => {
            let (kind, value) = object.iter().next()?;
            let name = || value.as_str();
            match kind.as_str() {
                "Vector3" => numbers(value).map(|[x, y, z]| Value::Vector3(Vector3::new(x, y, z))),
                "Vector2" => numbers(value).map(|[x, y]| Value::Vector2(Vector2::new(x, y))),
                "UDim2" => {
                    numbers(value).map(|[xs, xo, ys, yo]| Value::UDim2(UDim2::new(xs, xo, ys, yo)))
                }
                "Color3" => numbers(value).map(|[r, g, b]| Value::Color3(Color3::new(r, g, b))),
                "PartType" => PartType::from_name(name()?).map(Value::PartType),
                "Material" => Material::from_name(name()?).map(Value::Material),
                "TextureChannel" => TextureChannel::from_name(name()?).map(Value::TextureChannel),
                "ParticleType" => ParticleType::from_name(name()?).map(Value::ParticleType),
                "ToneMapping" => ToneMapping::from_name(name()?).map(Value::ToneMapping),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Exactly `N` numbers, from a JSON array.
fn numbers<const N: usize>(json: &serde_json::Value) -> Option<[f32; N]> {
    let items = json.as_array()?;
    if items.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (slot, item) in out.iter_mut().zip(items) {
        *slot = item.as_f64()? as f32;
    }
    Some(out)
}

/// Whether Instances of `class` are world state.
fn kept(class: ClassName) -> bool {
    !(class.is_script()
        || class.is_service()
        || matches!(
            class,
            ClassName::Player | ClassName::AnimationTrack | ClassName::ScreenGui
        ))
}

/// The world state of `world` now.
pub fn snapshot(world: &World) -> WorldState {
    snapshot_with_ids(world).0
}

/// The world state of `world` now, and the Instance each of its
/// [`instances`](WorldState::instances) entries was saved from.
pub(crate) fn snapshot_with_ids(world: &World) -> (WorldState, Vec<InstanceId>) {
    let mut state = WorldState::default();
    let mut ids = Vec::new();
    for class in SERVICES {
        let Some(service) = world.service(class) else {
            continue;
        };
        let service_name = class.as_str().to_owned();
        state
            .services
            .insert(service_name.clone(), changed_properties(world, service));
        // An explicit stack, so a deep tree can't overflow the call stack.
        // Children go on in reverse, so they come off in order.
        let mut stack: Vec<(InstanceId, SavedParent)> = children(world, service)
            .into_iter()
            .rev()
            .map(|child| (child, SavedParent::Service(service_name.clone())))
            .collect();
        while let Some((id, parent)) = stack.pop() {
            let Some(instance) = world.get(id) else {
                continue;
            };
            if !kept(instance.class()) {
                continue;
            }
            let index = state.instances.len();
            ids.push(id);
            state.instances.push(SavedInstance {
                parent,
                class: instance.class().as_str().to_owned(),
                name: instance.name().to_owned(),
                properties: changed_properties(world, id),
            });
            for &child in instance.children().iter().rev() {
                stack.push((child, SavedParent::Index(index)));
            }
        }
    }
    (state, ids)
}

fn children(world: &World, id: InstanceId) -> Vec<InstanceId> {
    world
        .get(id)
        .map(|instance| instance.children().to_vec())
        .unwrap_or_default()
}

/// The properties of `id` that differ from its class's defaults. A number
/// that isn't finite isn't saved: JSON can't hold it.
fn changed_properties(world: &World, id: InstanceId) -> Properties {
    let Some(instance) = world.get(id) else {
        return Properties::new();
    };
    let defaults: BTreeMap<&str, Value> =
        instance.class().default_properties().into_iter().collect();
    instance
        .properties()
        .filter(|(name, value)| defaults.get(name) != Some(*value))
        .filter(|(_, value)| value.as_number().is_none_or(f64::is_finite))
        .map(|(name, value)| (name.to_owned(), SavedValue(value.clone())))
        .collect()
}

/// Puts `world` back as `state` says (see the module's notes), and returns
/// the live Instance each [`instances`](WorldState::instances) entry became,
/// or `None` for an entry that was left out.
pub fn restore(world: &mut World, state: &WorldState) -> Vec<Option<InstanceId>> {
    let mut restored = vec![None; state.instances.len()];
    let none = Properties::new();
    for class in SERVICES {
        if let Some(service) = world.service(class) {
            let saved = state.services.get(class.as_str()).unwrap_or(&none);
            set_properties(world, service, saved);
        }
    }

    // Each saved parent's children, in order. An entry whose parent isn't
    // earlier in the list is left out, with everything inside it.
    let mut saved_children: HashMap<&SavedParent, Vec<usize>> = HashMap::new();
    for (index, entry) in state.instances.iter().enumerate() {
        let parent_ok = match &entry.parent {
            SavedParent::Index(parent) => *parent < index,
            SavedParent::Service(_) => true,
        };
        if parent_ok {
            saved_children.entry(&entry.parent).or_default().push(index);
        }
    }

    let mut stack: Vec<(InstanceId, SavedParent)> = SERVICES
        .iter()
        .rev()
        .filter_map(|&class| {
            let service = world.service(class)?;
            Some((service, SavedParent::Service(class.as_str().to_owned())))
        })
        .collect();
    while let Some((live_parent, key)) = stack.pop() {
        // The live children that are world state, by class, name and which
        // one of that class and name they are among their siblings.
        let mut live: Vec<(ClassName, String, usize, InstanceId)> = Vec::new();
        let mut counts: HashMap<(ClassName, String), usize> = HashMap::new();
        for child in children(world, live_parent) {
            let Some(instance) = world.get(child) else {
                continue;
            };
            if !kept(instance.class()) {
                continue;
            }
            let key = (instance.class(), instance.name().to_owned());
            let nth = counts.entry(key.clone()).or_default();
            live.push((key.0, key.1, *nth, child));
            *nth += 1;
        }

        let mut saved_counts: HashMap<(ClassName, &str), usize> = HashMap::new();
        let mut next = Vec::new();
        for &index in saved_children.get(&key).map(Vec::as_slice).unwrap_or(&[]) {
            let entry = &state.instances[index];
            let Some(class) = ClassName::from_name(&entry.class).filter(|&class| kept(class))
            else {
                continue;
            };
            let nth = saved_counts
                .entry((class, entry.name.as_str()))
                .or_default();
            let found = live
                .iter()
                .position(|(c, name, n, _)| *c == class && *name == entry.name && n == nth);
            *nth += 1;
            let id = match found {
                Some(position) => {
                    let id = live.remove(position).3;
                    set_properties(world, id, &entry.properties);
                    id
                }
                None => {
                    let Ok(id) = world.create(class) else {
                        continue;
                    };
                    let _ = world.set_name(id, entry.name.clone());
                    set_properties(world, id, &entry.properties);
                    let _ = world.set_parent(id, Some(live_parent));
                    id
                }
            };
            restored[index] = Some(id);
            next.push((id, SavedParent::Index(index)));
        }
        // What the save doesn't have was destroyed (or not made yet) when it was saved.
        for (_, _, _, id) in live {
            let _ = world.destroy(id);
        }
        stack.extend(next.into_iter().rev());
    }
    restored
}

/// Sets every property of `id` to its saved value, or to its class's
/// default when the save doesn't have it (it was the default when saved).
/// A saved value of the wrong type is left out. `Material` goes first,
/// since it resets `Metalness` and `Roughness`.
fn set_properties(world: &mut World, id: InstanceId, saved: &Properties) {
    let Some(instance) = world.get(id) else {
        return;
    };
    let mut values: Vec<(&'static str, Value)> = instance
        .class()
        .default_properties()
        .into_iter()
        .map(|(name, default)| {
            let value = saved
                .get(name)
                .map(|value| value.0.clone())
                .filter(|value| value.kind() == default.kind())
                .unwrap_or(default);
            (name, value)
        })
        .collect();
    values.sort_by_key(|(name, _)| *name != "Material");
    for (name, value) in values {
        let _ = world.set_property(id, name, value);
    }
}
