//! `terrain_edit`: the Terrain Studio's work, one call at a time:
//! `generate` a landscape from a preset (a new Terrain when there's none),
//! `sculpt` with a brush at a point or along a line, `paint` a layer,
//! scatter `foliage`, and `erode`. Each call is one undo step. Brushes go
//! through `axcenity_terrain::brush` as a mouse stroke does, and record
//! only the chunks they touched (`TerrainChange::Stroke`); generating and
//! eroding run at once and record the landscape before and after.

use axcenity_core::{FoliageProp, FoliageSource, InstanceId, MAX_TERRAIN_LAYERS, Vector3, World};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use axcenity_render::glam::{Vec2, Vec3};
use axcenity_terrain::Placement;
use axcenity_terrain::brush::{
    self, Brush, EditRecorder, FlattenMode, Operation, PaintMode, SculptMode, Stroke,
};
use axcenity_terrain::generate::Preset;
use axcenity_terrain::materials;

use super::ops::{checkpoint, confirm};
use super::refs::{label, quoted, resolve};
use super::{CONFIRM, Host};
use crate::asset_browser::{AssetKind, scan};
use crate::history::{self, TerrainChange};
use crate::terrain_studio::ErosionKind;

pub const SPEC: ToolSpec = ToolSpec {
    name: "terrain_edit",
    title: "Edit terrain",
    summary: "Generate, sculpt, paint, scatter foliage on or erode a terrain; each call is one undo step.",
    profile: Profile::Standard,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::required(
            "action",
            Kind::Enum(&["generate", "sculpt", "paint", "foliage", "erode"]),
            "",
        ),
        Param::optional("terrain", Kind::String, "ID or path; default the only one"),
        Param::optional("preset", Kind::Enum(PRESETS), "generate"),
        Param::optional("seed", Kind::Integer, "generate"),
        Param::optional(
            "size",
            Kind::Vector3,
            "generate: [width,height,length] studs",
        ),
        Param::optional("tool", Kind::Enum(TOOLS), "sculpt; default raise"),
        Param::optional("at", Kind::Vector3, "brush centre; flatten goes to its y"),
        Param::optional("to", Kind::Vector3, "stroke from at to here"),
        Param::optional("radius", Kind::Number, "studs, default 12"),
        Param::optional("strength", Kind::Number, "0-1, default 0.5"),
        Param::optional(
            "amount",
            Kind::Number,
            "studs: raise/lower height, noise bumps, terrace step",
        ),
        Param::optional("layer", Kind::String, "paint: layer name"),
        Param::optional("item", Kind::String, "foliage: prop name or model path"),
        Param::optional(
            "density",
            Kind::Number,
            "foliage per square stud, default 0.08",
        ),
        Param::optional("erase", Kind::Bool, "paint or foliage: take away"),
        Param::optional(
            "kind",
            Kind::Enum(&["rain", "scree"]),
            "erode; default rain",
        ),
        CONFIRM,
    ],
};

const PRESETS: &[&str] = &[
    "rolling_hills",
    "mountain_range",
    "alpine_valley",
    "desert_dunes",
    "rocky_highlands",
    "island",
    "flatlands",
    "crater",
    "snowy_mountains",
];

const TOOLS: &[&str] = &[
    "raise", "lower", "smooth", "flatten", "noise", "hill", "basin", "ridge", "valley", "terrace",
];

/// The most stamps one call makes.
const MAX_STAMPS: usize = 2_000;
/// Studs a Raise stamp lifts per stud of radius at full strength
/// (`brush::RAISE_PER_STAMP`).
const RAISE_PER_STAMP: f32 = 0.05;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(folder) = host
        .app
        .project
        .as_ref()
        .map(|project| project.folder.clone())
    else {
        return Err(ToolError::not_found("no project is open"));
    };
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let action = args.str("action").unwrap_or_default();
    let terrain = match (target(&host.app.world, args.str("terrain")), action) {
        (Ok(id), _) => Some(id),
        // Generating with no terrain makes one.
        (Err(error), "generate")
            if error.code == Code::NotFound && args.str("terrain").is_none() =>
        {
            None
        }
        (Err(error), _) => return Err(error),
    };
    // A parameter another action takes is a mistake worth saying.
    let only = |names: &[&str]| -> Result<(), ToolError> {
        let common = ["action", "terrain", "confirm"];
        for param in SPEC.params {
            if args.get(param.name).is_some()
                && !names.contains(&param.name)
                && !common.contains(&param.name)
            {
                return Err(
                    ToolError::bad_args(format!("{action} doesn't take {}", param.name))
                        .with_fix(format!("{action} takes: {}", names.join(", "))),
                );
            }
        }
        Ok(())
    };
    let done = match action {
        "generate" => {
            only(&["preset", "seed", "size"])?;
            generate(host, &folder, args, terrain)?
        }
        "erode" => {
            only(&["kind"])?;
            let terrain = terrain.expect("found above");
            erode(host, &folder, args, terrain)?
        }
        _ => {
            let terrain = terrain.expect("found above");
            let names: &[&str] = match action {
                "sculpt" => &["tool", "at", "to", "radius", "strength", "amount"],
                "paint" => &["layer", "at", "to", "radius", "strength", "erase"],
                _ => &["item", "at", "to", "radius", "strength", "density", "erase"],
            };
            only(names)?;
            stroke(host, &folder, args, terrain, action)?
        }
    };
    Ok(done)
}

/// The Terrain `text` names, or the only one in the Workspace.
fn target(world: &World, text: Option<&str>) -> Result<InstanceId, ToolError> {
    if let Some(text) = text {
        let id = resolve(world, text)?;
        if world.terrain(id).is_none() {
            return Err(ToolError::new(
                Code::BadValue,
                format!("{} isn't a Terrain", label(world, id)),
            ));
        }
        return Ok(id);
    }
    let all: Vec<InstanceId> = world
        .descendants_iter(world.workspace())
        .filter(|&id| world.terrain(id).is_some())
        .collect();
    match all.as_slice() {
        [] => Err(ToolError::not_found("the Workspace has no Terrain")
            .with_fix("action generate makes one")),
        [one] => Ok(*one),
        many => Err(
            ToolError::new(Code::Ambiguous, "there are several terrains").with_fix(format!(
                "give terrain: {}",
                many.iter()
                    .map(|&id| label(world, id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        ),
    }
}

fn undo_label(host: &Host, what: &str) -> String {
    format!("AI ({}): {what}", host.session.client)
}

/// Ends a result line with its undo step.
fn finish(host: &mut Host, mut out: String, undo: String) -> String {
    host.app.history.seal();
    out.push_str(&format!(" · undo {}", quoted(&undo)));
    host.session.last_undo = Some(undo);
    out
}

fn generate(
    host: &mut Host,
    folder: &std::path::Path,
    args: &Args,
    terrain: Option<InstanceId>,
) -> Outcome {
    let name = args.str("preset").unwrap_or("rolling_hills");
    let index = PRESETS.iter().position(|&known| known == name).unwrap_or(0);
    let preset = Preset::ALL[index];
    let mut settings = preset.settings();
    if let Some(seed) = args.u64("seed") {
        settings.seed = seed;
    }
    let world = &host.app.world;
    let current = terrain
        .and_then(|id| world.get(id))
        .and_then(Placement::of)
        .map(|placement| placement.size);
    let size = match args.vector3("size") {
        Some([x, y, z]) => {
            if !(1.0..=100_000.0).contains(&x)
                || !(1.0..=10_000.0).contains(&y)
                || !(1.0..=100_000.0).contains(&z)
            {
                return Err(ToolError::new(
                    Code::BadValue,
                    "size is [width,height,length] in studs, each at least 1",
                ));
            }
            Vector3::new(x as f32, y as f32, z as f32)
        }
        None => current.map_or(Vector3::new(512.0, 128.0, 512.0), |size| {
            Vector3::new(size.x, size.y, size.z)
        }),
    };
    if let Some(id) = terrain {
        let preview = format!(
            "replaces the whole landscape of {} with {} (seed {}), {}×{}×{} studs; its paint and foliage go too",
            label(world, id),
            preset.label(),
            settings.seed,
            size.x,
            size.y,
            size.z
        );
        confirm(host, &SPEC, args, &preview)?;
    }
    checkpoint(host, folder);
    let undo = undo_label(host, &format!("Generate {}", preset.label()));
    let id = host
        .app
        .generate_now(terrain, settings, size, undo.clone())
        .map_err(|reason| {
            ToolError::new(Code::BadValue, format!("could not generate it: {reason}"))
        })?;
    let world = &host.app.world;
    let layers = materials::layers(world, id).len();
    let out = format!(
        "{} {} as {} · {}×{}×{} studs · {} layer{}{}",
        if terrain.is_some() {
            "regenerated"
        } else {
            "made"
        },
        label(world, id),
        preset.label(),
        size.x,
        size.y,
        size.z,
        layers,
        if layers == 1 { "" } else { "s" },
        heights(world, id)
    );
    Ok(finish(host, out, undo))
}

fn erode(host: &mut Host, folder: &std::path::Path, args: &Args, terrain: InstanceId) -> Outcome {
    let (kind, what) = match args.str("kind") {
        Some("scree") => (ErosionKind::Thermal, "Scree Erosion"),
        _ => (ErosionKind::Hydraulic, "Rain Erosion"),
    };
    checkpoint(host, folder);
    let undo = undo_label(host, &format!("Apply {what}"));
    host.app
        .erode_now(terrain, kind, undo.clone())
        .map_err(|reason| {
            ToolError::new(Code::BadValue, format!("could not erode it: {reason}"))
        })?;
    let world = &host.app.world;
    let out = format!(
        "{what} on {}{}",
        label(world, terrain),
        heights(world, terrain)
    );
    Ok(finish(host, out, undo))
}

/// ` · ground from y=… to y=…`.
fn heights(world: &World, id: InstanceId) -> String {
    let (Some(placement), Some(data)) = (world.get(id).and_then(Placement::of), world.terrain(id))
    else {
        return String::new();
    };
    let (low, high) = data
        .heights()
        .iter()
        .fold((f32::MAX, f32::MIN), |(low, high), &h| {
            (low.min(h), high.max(h))
        });
    format!(
        " · ground from y={:.1} to y={:.1}",
        placement.world_y(low),
        placement.world_y(high)
    )
}

/// A brush stroke: one stamp at `at`, or stamps along to `to`.
fn stroke(
    host: &mut Host,
    folder: &std::path::Path,
    args: &Args,
    terrain: InstanceId,
    action: &str,
) -> Outcome {
    let Some([x, y, z]) = args.vector3("at") else {
        return Err(ToolError::bad_args(format!("{action} needs at: [x,y,z]")));
    };
    let at = Vec3::new(x as f32, y as f32, z as f32);
    let to = args
        .vector3("to")
        .map(|[x, y, z]| Vec3::new(x as f32, y as f32, z as f32));
    let world = &host.app.world;
    let placement = world
        .get(terrain)
        .and_then(Placement::of)
        .ok_or_else(|| ToolError::new(Code::BadValue, "it isn't a Terrain"))?;
    let radius = args.f64("radius").unwrap_or(12.0) as f32;
    let strength = args.f64("strength").unwrap_or(0.5) as f32;
    if !(0.5..=10_000.0).contains(&radius) {
        return Err(ToolError::new(
            Code::BadValue,
            "radius is 0.5 to 10,000 studs",
        ));
    }
    if !(0.0..=1.0).contains(&strength) {
        return Err(ToolError::new(Code::BadValue, "strength is 0 to 1"));
    }
    let mut brush = Brush {
        radius,
        strength,
        ..Brush::default()
    };
    let erase = args.bool("erase") == Some(true);
    // Stamps at each point (raise and lower repeat to reach their height).
    let mut repeat = 1;
    let (operation, what) = match action {
        "sculpt" => {
            let tool = args.str("tool").unwrap_or("raise");
            let amount = args.f64("amount").map(|amount| amount as f32);
            if amount.is_some_and(|amount| !(0.0..=10_000.0).contains(&amount)) {
                return Err(ToolError::new(
                    Code::BadValue,
                    "amount is 0 to 10,000 studs",
                ));
            }
            let operation = match tool {
                "raise" | "lower" => {
                    // Strength and stamps that lift the middle `amount` studs
                    // (`amount` over the overlap of a line's stamps).
                    let wanted = amount.unwrap_or(4.0) / overlap(&brush, to.is_some());
                    let per_full = radius * RAISE_PER_STAMP;
                    repeat = ((wanted / per_full).ceil() as usize).max(1);
                    brush.strength = (wanted / (repeat as f32 * per_full)).min(1.0);
                    if tool == "raise" {
                        Operation::Raise
                    } else {
                        Operation::Lower
                    }
                }
                "smooth" => Operation::Smooth,
                "flatten" => Operation::Flatten {
                    target: at.y,
                    mode: FlattenMode::Both,
                },
                "noise" => Operation::Noise {
                    amplitude: amount.unwrap_or(4.0),
                    scale: 12.0,
                    seed: 1,
                },
                _ => {
                    let mode = match tool {
                        "basin" => SculptMode::Basin,
                        "ridge" => SculptMode::Ridge,
                        "valley" => SculptMode::Valley,
                        "terrace" => SculptMode::Terrace,
                        _ => SculptMode::Hill,
                    };
                    Operation::Sculpt {
                        mode,
                        step: amount.unwrap_or(4.0).max(0.1),
                    }
                }
            };
            let mut what = tool.to_owned();
            what[..1].make_ascii_uppercase();
            (operation, format!("{what} Terrain"))
        }
        "paint" if erase => (
            Operation::Erase {
                locked: [false; MAX_TERRAIN_LAYERS],
            },
            "Erase Terrain Paint".to_owned(),
        ),
        "paint" => {
            let (index, name) = paint_layer(world, terrain, args.str("layer"))?;
            (
                Operation::Paint {
                    layer: index,
                    mode: PaintMode::Replace,
                    locked: [false; MAX_TERRAIN_LAYERS],
                },
                format!("Paint {name}"),
            )
        }
        _ => {
            let source = foliage_source(folder, args.str("item"))?;
            let density = args.f64("density").unwrap_or(0.08) as f32;
            if !(0.0..=10.0).contains(&density) {
                return Err(ToolError::new(
                    Code::BadValue,
                    "density is 0 to 10 per square stud",
                ));
            }
            let what = format!(
                "{} {} Foliage",
                if erase { "Erase" } else { "Paint" },
                source.name()
            );
            (
                Operation::Foliage {
                    source,
                    density,
                    erase,
                    collide: false,
                },
                what,
            )
        }
    };
    // Where the stamps go: as a mouse stroke spaces them.
    let mut line = Stroke::new();
    let mut centres = line.advance(at, brush.spacing_studs(), 0.0, None);
    if let Some(to) = to {
        centres.extend(line.advance(to, brush.spacing_studs(), 0.0, None));
    }
    if centres.len() * repeat > MAX_STAMPS {
        return Err(
            ToolError::new(Code::TooLarge, "that stroke takes too many stamps")
                .with_fix("use a bigger radius, a shorter line or a smaller amount"),
        );
    }
    checkpoint(host, folder);
    let undo = undo_label(host, &what);
    let app = &mut host.app;
    let Some(data) = app.world.terrain_mut(terrain) else {
        return Err(ToolError::new(Code::BadValue, "it has no landscape"));
    };
    let mut recorder = EditRecorder::new();
    for _ in 0..repeat {
        for &centre in &centres {
            for rect in brush::affected_rects(data, &placement, &brush, centre) {
                recorder.before_change(data, rect);
            }
            brush::apply(data, &placement, &brush, &operation, centre);
        }
    }
    let ground = placement.ground_y(data, at.x, at.z);
    let Some(edit) = recorder.finish(data) else {
        return Ok(format!(
            "{what}: nothing changed (is [{x}, {z}] on the terrain, inside its limits?)"
        ));
    };
    let fields = history::snapshot(&app.world, terrain);
    app.history.record_terrain(
        &mut app.world,
        terrain,
        undo.clone(),
        TerrainChange::Stroke(edit),
        fields.clone(),
        fields,
    );
    let mut out = format!(
        "{what} on {} at [{x}, {z}]{} · radius {radius}",
        label(&app.world, terrain),
        to.map_or_else(String::new, |to| format!(" to [{}, {}]", to.x, to.z)),
    );
    if let Some(ground) = ground {
        out.push_str(&format!(" · ground there now at y={ground:.1}"));
    }
    if let Some(count) = app.world.terrain(terrain).map(|data| data.foliage().len())
        && action == "foliage"
    {
        out.push_str(&format!(" · {count} foliage in all"));
    }
    Ok(finish(host, out, undo))
}

/// How many times over a stamp's middle is reached: 1 for one stamp, more
/// along a line, where stamps overlap.
fn overlap(brush: &Brush, line: bool) -> f32 {
    if !line {
        return 1.0;
    }
    let spacing = brush.spacing_studs();
    let reach = (brush.radius / spacing).ceil() as i32 + 1;
    let at = |offset: f32| -> f32 {
        (-reach..=reach)
            .map(|k| brush.weight(Vec2::new(k as f32 * spacing - offset, 0.0)))
            .sum()
    };
    ((at(0.0) + at(spacing / 2.0)) / 2.0).max(1.0)
}

/// The layer `name` names among the terrain's layers, by index.
fn paint_layer(
    world: &World,
    terrain: InstanceId,
    name: Option<&str>,
) -> Result<(usize, String), ToolError> {
    let layers: Vec<String> = materials::layers(world, terrain)
        .into_iter()
        .filter_map(|id| world.get(id).map(|layer| layer.name().to_string()))
        .collect();
    if layers.is_empty() {
        return Err(ToolError::not_found("the terrain has no layers to paint")
            .with_fix("add a TerrainLayer inside it with object_create"));
    }
    let Some(name) = name else {
        return Err(ToolError::bad_args("paint needs layer")
            .with_fix(format!("layers: {}", layers.join(", "))));
    };
    layers
        .iter()
        .position(|layer| layer.eq_ignore_ascii_case(name))
        .map(|index| (index, layers[index].clone()))
        .ok_or_else(|| {
            ToolError::not_found(format!("no layer {}", quoted(name)))
                .with_fix(format!("layers: {}", layers.join(", ")))
        })
}

/// What `item` names to scatter: a built-in prop, or a model in the project.
fn foliage_source(
    folder: &std::path::Path,
    item: Option<&str>,
) -> Result<FoliageSource, ToolError> {
    let props = || {
        FoliageProp::ALL
            .iter()
            .map(|prop| prop.label())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let Some(item) = item else {
        return Err(ToolError::bad_args("foliage needs item").with_fix(format!(
            "a prop ({}) or a model path from asset_find",
            props()
        )));
    };
    if let Some(prop) = FoliageProp::ALL
        .iter()
        .find(|prop| prop.label().eq_ignore_ascii_case(item))
    {
        return Ok(FoliageSource::Prop(*prop));
    }
    let model = scan(folder)
        .into_iter()
        .find(|entry| entry.kind == AssetKind::Model && entry.path == item);
    match model {
        Some(entry) => Ok(FoliageSource::Model(entry.path)),
        None => Err(
            ToolError::not_found(format!("no prop or model {}", quoted(item)))
                .with_fix(format!("props: {}; models: asset_find kind model", props())),
        ),
    }
}
