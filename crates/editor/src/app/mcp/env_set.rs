//! `env_set`: the look of the world in one call, as one undo step: time
//! of day, fog, clouds, rain or snow, sky image, exposure, bloom and
//! shadows. A `preset` sets several at once; the other parameters then
//! change what it set. The Lighting service holds most of these; Clouds,
//! an Atmosphere and Weather are objects in it, made when first needed
//! and changed after that.

use axcenity_core::{ClassName, InstanceId, World};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use serde_json::{Map, Value as Json, json};

use super::Host;
use super::object_create;
use super::ops::write;
use super::refs::{id_text, quoted};
use super::values::set_props;

pub const SPEC: ToolSpec = ToolSpec {
    name: "env_set",
    title: "Set the environment",
    summary: "Set time of day, fog, clouds, rain or snow, sky and exposure at once, e.g. {\"preset\":\"sunset\"}.",
    profile: Profile::Standard,
    class: Class::Modify,
    idempotent: true,
    params: &[
        Param::optional(
            "preset",
            Kind::Enum(&["day", "sunset", "night", "overcast", "foggy"]),
            "",
        ),
        Param::optional("time", Kind::Number, "hour, 0-24"),
        Param::optional("latitude", Kind::Number, "degrees"),
        Param::optional("fog", Kind::Number, "see this far (studs); 0 clears"),
        Param::optional("fog_color", Kind::Vector3, "[r,g,b] 0-1"),
        Param::optional("clouds", Kind::Number, "cover 0-1"),
        Param::optional("weather", Kind::Enum(&["none", "rain", "snow"]), ""),
        Param::optional("intensity", Kind::Number, "rain or snow 0-1"),
        Param::optional("sky", Kind::String, "image path; \"\" clears"),
        Param::optional("exposure", Kind::Number, "1 is normal"),
        Param::optional("bloom", Kind::Number, "intensity; 0 turns off"),
        Param::optional("shadows", Kind::Bool, ""),
    ],
};

/// FogEnd with no fog, as the Lighting service starts.
const NO_FOG: f64 = 100_000.0;

/// What one call sets; `None` leaves it.
#[derive(Default)]
struct Look {
    time: Option<f64>,
    latitude: Option<f64>,
    fog: Option<f64>,
    fog_color: Option<[f64; 3]>,
    clouds: Option<f64>,
    /// The Atmosphere's Haze and Glare.
    haze: Option<(f64, f64)>,
    weather: Option<String>,
    intensity: Option<f64>,
    sky: Option<String>,
    exposure: Option<f64>,
    bloom: Option<f64>,
    shadows: Option<bool>,
}

impl Look {
    fn preset(name: &str) -> Look {
        let mut look = Look {
            weather: Some("none".to_owned()),
            exposure: Some(1.0),
            ..Look::default()
        };
        let (time, clouds, haze, fog) = match name {
            "sunset" => (18.2, 0.35, (1.2, 0.8), 0.0),
            "night" => (0.0, 0.2, (0.0, 0.0), 0.0),
            "overcast" => (13.0, 0.95, (1.5, 0.0), 800.0),
            "foggy" => (9.0, 0.7, (2.5, 0.0), 180.0),
            // "day"
            _ => (14.0, 0.3, (0.0, 0.0), 0.0),
        };
        look.time = Some(time);
        look.clouds = Some(clouds);
        look.haze = Some(haze);
        look.fog = Some(fog);
        if name == "foggy" || name == "overcast" {
            look.fog_color = Some([0.75, 0.77, 0.8]);
        }
        look
    }

    fn read(args: &Args) -> Result<Look, ToolError> {
        let mut look = args.str("preset").map(Look::preset).unwrap_or_default();
        let number = |name: &str, low: f64, high: f64| -> Result<Option<f64>, ToolError> {
            match args.f64(name) {
                Some(value) if !(low..=high).contains(&value) => Err(ToolError::new(
                    Code::BadValue,
                    format!("{name} {value}; give {low} to {high}"),
                )),
                value => Ok(value),
            }
        };
        if let Some(time) = number("time", 0.0, 24.0)? {
            look.time = Some(time);
        }
        if let Some(latitude) = number("latitude", -90.0, 90.0)? {
            look.latitude = Some(latitude);
        }
        if let Some(fog) = number("fog", 0.0, NO_FOG)? {
            look.fog = Some(fog);
        }
        if let Some(color) = args.vector3("fog_color") {
            look.fog_color = Some(color);
        }
        if let Some(clouds) = number("clouds", 0.0, 1.0)? {
            look.clouds = Some(clouds);
        }
        if let Some(weather) = args.str("weather") {
            look.weather = Some(weather.to_owned());
        }
        look.intensity = number("intensity", 0.0, 1.0)?;
        if let Some(sky) = args.str("sky") {
            look.sky = Some(sky.to_owned());
        }
        if let Some(exposure) = number("exposure", 0.0, 16.0)? {
            look.exposure = Some(exposure);
        }
        if let Some(bloom) = number("bloom", 0.0, 10.0)? {
            look.bloom = Some(bloom);
        }
        look.shadows = args.bool("shadows");
        Ok(look)
    }
}

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    if args.0.values().all(Json::is_null) {
        return Err(ToolError::bad_args("nothing to set")
            .with_fix("give preset, time, fog, clouds, weather, sky, exposure, bloom or shadows"));
    }
    let look = Look::read(args)?;
    if let Some(sky) = look.sky.as_deref().filter(|sky| !sky.is_empty()) {
        let folder = host
            .app
            .project
            .as_ref()
            .map(|project| project.folder.clone())
            .ok_or_else(|| ToolError::not_found("no project is open"))?;
        let path = std::path::Path::new(sky);
        let inside = path.is_relative()
            && path
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)));
        if !inside || !folder.join(path).is_file() {
            return Err(ToolError::not_found(format!(
                "sky {} isn't an image in the project",
                quoted(sky)
            ))
            .with_fix("give a path from the project folder, like \"assets/sky/dusk.hdr\""));
        }
    }
    let label = args.str("preset").map_or_else(
        || "Set environment".to_owned(),
        |preset| format!("Environment: {preset}"),
    );
    let written = write(host, &label, |world| apply(world, &look))?;
    Ok(format!(
        "{}{}",
        written.value.join("; "),
        written.undo_note()
    ))
}

/// Makes `look`; returns a note per object it changed or made.
fn apply(world: &mut World, look: &Look) -> Result<Vec<String>, ToolError> {
    let mut notes = Vec::new();
    let lighting = world.lighting();
    let mut props = Map::new();
    let mut put = |name: &str, value: Json| {
        props.insert(name.to_owned(), value);
    };
    if let Some(time) = look.time {
        put("ClockTime", json!(time));
    }
    if let Some(latitude) = look.latitude {
        put("GeographicLatitude", json!(latitude));
    }
    if let Some(fog) = look.fog {
        if fog > 0.0 {
            put("FogStart", json!(fog * 0.2));
            put("FogEnd", json!(fog));
        } else {
            put("FogStart", json!(0.0));
            put("FogEnd", json!(NO_FOG));
        }
    }
    if let Some(color) = look.fog_color {
        put("FogColor", json!(color));
    }
    if let Some(sky) = &look.sky {
        put("SkyImage", json!(sky));
    }
    if let Some(exposure) = look.exposure {
        put("Exposure", json!(exposure));
    }
    if let Some(bloom) = look.bloom {
        put("BloomEnabled", json!(bloom > 0.0));
        if bloom > 0.0 {
            put("BloomIntensity", json!(bloom));
        }
    }
    if let Some(shadows) = look.shadows {
        put("ShadowsEnabled", json!(shadows));
    }
    if !props.is_empty() {
        let names = set_props(world, lighting, &props)?;
        notes.push(format!("Lighting {}", names.join(", ")));
    }
    if let Some(cover) = look.clouds {
        let mut props = Map::new();
        props.insert("Cover".to_owned(), json!(cover));
        notes.extend(set_or_make(world, ClassName::Clouds, props, cover > 0.0)?);
    }
    if let Some((haze, glare)) = look.haze {
        let mut props = Map::new();
        props.insert("Haze".to_owned(), json!(haze));
        props.insert("Glare".to_owned(), json!(glare));
        let needed = haze > 0.0 || glare > 0.0;
        notes.extend(set_or_make(world, ClassName::Atmosphere, props, needed)?);
    }
    match (look.weather.as_deref(), look.intensity) {
        (Some("none"), _) => {
            let mut props = Map::new();
            props.insert("Precipitation".to_owned(), json!("None"));
            notes.extend(set_or_make(world, ClassName::Weather, props, false)?);
        }
        (Some(falling), intensity) => {
            let mut props = Map::new();
            let item = if falling == "snow" { "Snow" } else { "Rain" };
            props.insert("Precipitation".to_owned(), json!(item));
            if let Some(intensity) = intensity {
                props.insert("Intensity".to_owned(), json!(intensity));
            }
            notes.extend(set_or_make(world, ClassName::Weather, props, true)?);
        }
        (None, Some(intensity)) => {
            if find(world, ClassName::Weather).is_none() {
                return Err(ToolError::bad_args(
                    "intensity goes with weather rain or snow",
                ));
            }
            let mut props = Map::new();
            props.insert("Intensity".to_owned(), json!(intensity));
            notes.extend(set_or_make(world, ClassName::Weather, props, false)?);
        }
        (None, None) => {}
    }
    Ok(notes)
}

/// The first object of `class` in the game, if there is one.
fn find(world: &World, class: ClassName) -> Option<InstanceId> {
    world.descendants_iter(world.root()).find(|&id| {
        world
            .get(id)
            .is_some_and(|instance| instance.class() == class)
    })
}

/// Sets `props` on the game's object of `class`; with none, makes one in
/// Lighting when `needed`. A note on what it did, if anything.
fn set_or_make(
    world: &mut World,
    class: ClassName,
    props: Map<String, Json>,
    needed: bool,
) -> Result<Option<String>, ToolError> {
    if let Some(id) = find(world, class) {
        let names = set_props(world, id, &props)?;
        return Ok(Some(format!("{} {}", class.as_str(), names.join(", "))));
    }
    if !needed {
        return Ok(None);
    }
    let args = Args::new(
        json!({
            "kind": class.as_str(),
            "parent": id_text(world.lighting()),
            "props": props,
        })
        .as_object()
        .cloned()
        .unwrap_or_default(),
    );
    object_create::step(world, &args)?;
    let names: Vec<&str> = props.keys().map(String::as_str).collect();
    Ok(Some(format!(
        "made {} in Lighting ({})",
        class.as_str(),
        names.join(", ")
    )))
}
