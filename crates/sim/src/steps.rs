//! The steps of a frame.
//!
//! [`STANDARD`] is the player's frame, step by step. Steps a host skips say
//! so in their `hosts`. A step is a plain function: the whole frame reads
//! top to bottom here, and `docs/architecture/frame-loop.md` explains why
//! each step is where it is.

use crate::host::FrameCtx;
use crate::schedule::{Hosts, Stage, Step};

const fn step(name: &'static str, stage: Stage, hosts: Hosts, run: fn(&mut FrameCtx)) -> Step {
    Step {
        name,
        stage,
        hosts,
        run,
        lap: None,
    }
}

const P: Hosts = Hosts::PLAYER;
const PE: Hosts = Hosts::PLAYER.with(Hosts::EDITOR_PLAY);
const GAME: Hosts = Hosts::GAME;
const NET: Hosts = Hosts::NETWORKED;
const ALL: Hosts = Hosts::ALL;

/// The one order: the player's frame.
#[rustfmt::skip]
pub(crate) static STANDARD: &[Step] = &[
    step("net.receive",       Stage::NetReceive,     NET,  net_receive),

    step("characters.step",   Stage::Input,          GAME, characters_step),
    step("characters.landed", Stage::Input,          GAME, characters_landed),
    step("humanoids",         Stage::Input,          GAME, humanoids),
    step("players",           Stage::Input,          ALL,  players),
    step("agents.step",       Stage::Input,          GAME, agents_step),
    step("agents.sensed",     Stage::Input,          GAME, agents_sensed),
    step("input.events",      Stage::Input,          GAME, input_events),
    step("prompts",           Stage::Input,          GAME, prompts),
    step("tools",             Stage::Input,          GAME, tools),
    step("abilities",         Stage::Input,          GAME, abilities),

    step("scripts.step",      Stage::PreSimulation,  ALL,  scripts_step),
    step("scripts.stepped",   Stage::PreSimulation,  ALL,  scripts_stepped),
    step("physics.hold",      Stage::PreSimulation,  NET,  physics_hold),
    step("physics.kinematic", Stage::PreSimulation,  GAME, physics_kinematic),
    step("physics.shatters",  Stage::PreSimulation,  ALL,  physics_shatters),

    step("physics.step",      Stage::Simulation,     ALL,  physics_step),

    step("touches.began",     Stage::PostSimulation, ALL,  touches_began),
    step("touches.ended",     Stage::PostSimulation, ALL,  touches_ended),
    step("breaks",            Stage::PostSimulation, ALL,  breaks),
    step("health",            Stage::PostSimulation, ALL,  health),

    step("heartbeat",         Stage::Heartbeat,      ALL,  heartbeat),

    step("audio",             Stage::PreAnimation,   GAME, audio),
    step("particles",         Stage::PreAnimation,   GAME, particles),

    step("animators",         Stage::Animation,      GAME, animators),
    step("tools.place",       Stage::Animation,      GAME, tools_place),

    step("levels.loaded",     Stage::PreRender,      GAME, levels_loaded),
    step("loading.screen",    Stage::PreRender,      GAME, loading_screen),
    step("render_step",       Stage::PreRender,      GAME, render_step),
    step("camera.move",       Stage::PreRender,      GAME, camera_move),
    step("camera.follow",     Stage::PreRender,      GAME, camera_follow),
    step("graphics.changed",  Stage::PreRender,      PE,   graphics_changed),
    step("streaming",         Stage::PreRender,      PE,   streaming),
    step("draw.scene",        Stage::PreRender,      P,    draw_scene),
    step("draw.ui",           Stage::PreRender,      P,    draw_ui),

    step("net.send",          Stage::NetSend,        NET,  net_send),
];

/// A step a host still runs somewhere other than [`STANDARD`] says.
pub(crate) struct Drift {
    pub host: Hosts,
    pub step: &'static str,
    /// The host runs the step right after this one.
    pub after: &'static str,
}

/// Every place a host's order differs from the standard. Applied in order,
/// so each line sees the moves above it. Removing a line is a deliberate
/// behaviour change for that host, with its own test.
#[rustfmt::skip]
pub(crate) static DRIFT: &[Drift] = &[
    // The server replaces shattered parts before asking which parts are
    // held still, where the other hosts ask first.
    Drift { host: Hosts::SERVER, step: "physics.shatters", after: "scripts.stepped" },
];

/// Where each host's benchmark steps end: `(step, benchmark step name)`.
/// The names are the ones `docs/benchmarks.md` lists, so a host's numbers
/// stay comparable from run to run.
#[rustfmt::skip]
pub(crate) static LAPS: &[(Hosts, &[(&str, &str)])] = &[
    (Hosts::PLAYER, &[
        ("net.receive", "Network"),
        ("players", "Characters"),
        ("agents.step", "Agents"),
        ("abilities", "Input"),
        ("scripts.stepped", "Scripts"),
        ("physics.step", "Physics"),
        ("heartbeat", "Heartbeat"),
        ("loading.screen", "Animation"),
        ("draw.scene", "Draw list"),
        ("net.send", "UI"),
    ]),
    (Hosts::EDITOR_PLAY, &[
        ("net.receive", "Network"),
        ("players", "Characters"),
        ("agents.step", "Agents"),
        ("abilities", "Input"),
        ("scripts.stepped", "Scripts"),
        ("physics.step", "Physics"),
        ("heartbeat", "Heartbeat"),
        ("audio", "Audio"),
        ("loading.screen", "Animation"),
        ("graphics.changed", "RenderStep"),
        ("net.send", "Network send"),
    ]),
    (Hosts::SERVER, &[
        ("net.receive", "Network"),
        ("players", "Input"),
        ("scripts.stepped", "Scripts"),
        ("physics.step", "Physics"),
        ("heartbeat", "Heartbeat"),
        ("net.send", "Network send"),
    ]),
    (Hosts::TEST, &[
        ("players", "Characters"),
        ("agents.step", "Agents"),
        ("abilities", "Input"),
        ("scripts.stepped", "Scripts"),
        ("physics.step", "Physics"),
        ("heartbeat", "Heartbeat"),
        ("loading.screen", "Animation"),
        ("camera.follow", "RenderStep"),
    ]),
];

fn net_receive(ctx: &mut FrameCtx) {
    if ctx.io.net_receive(ctx.seconds).is_break() {
        ctx.end_frame();
        return;
    }
    ctx.following = ctx.io.following();
}

/// Character controllers move first and own the camera. A device following
/// a server moves only its own character.
fn characters_step(ctx: &mut FrameCtx) {
    let seconds = ctx.seconds as f32;
    let motion = ctx.io.character_motion(seconds);
    let sim = ctx.io.sim();
    sim.characters
        .step(&mut sim.world, &sim.assets, motion, seconds);
    let seats: Vec<_> = sim
        .characters
        .take_seat_changes()
        .into_iter()
        .map(|change| (change.controller, change.seated, change.seat))
        .collect();
    if !seats.is_empty() {
        sim.scripts.fire_seated(&mut sim.world, &seats);
    }
}

fn characters_landed(ctx: &mut FrameCtx) {
    let following = ctx.following;
    let sim = ctx.io.sim();
    let landings = sim.characters.take_landings();
    if !following && !landings.is_empty() {
        sim.scripts.fire_landed(&mut sim.world, &landings);
    }
}

/// Humanoids: MoveTo, states and dying (Phase 46).
fn humanoids(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    sim.scripts
        .step_humanoids(&mut sim.world, &sim.characters, ctx.seconds);
}

/// Players: their first character, respawns and spawn ForceFields (Phase 47).
fn players(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    sim.scripts.step_players(&mut sim.world, ctx.seconds);
}

fn agents_step(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    sim.agents
        .step(&mut sim.world, &sim.assets, ctx.seconds as f32);
    for warning in sim.agents.take_warnings() {
        ctx.io.agent_warning(warning);
    }
}

fn agents_sensed(ctx: &mut FrameCtx) {
    if !ctx.following {
        let sim = ctx.io.sim();
        for sensed in sim.agents.take_sensed() {
            let sense = sensed.sense.as_str().to_owned();
            sim.scripts
                .fire_instance_event(&mut sim.world, sensed.agent, "PlayerSensed", &[sense]);
        }
    }
    ctx.io.flush_output();
}

/// The camera and the input, for LocalScripts.
fn input_events(ctx: &mut FrameCtx) {
    let events = ctx.io.frame_input(ctx.seconds);
    let sim = ctx.io.sim();
    sim.scripts.input_events(&mut sim.world, &events);
}

/// ProximityPrompts and ClickDetectors, by that input.
fn prompts(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    sim.scripts.step_prompts(&mut sim.world, ctx.seconds);
}

/// Tools: pickups, the hotbar, Drop and Primary.
fn tools(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    axcenity_gameplay::step_tools(&mut sim.scripts, &mut sim.world);
}

/// Abilities' keys, on every device (Phase 76). A breakpoint hit in the
/// input step ends the frame here.
fn abilities(ctx: &mut FrameCtx) {
    let sim = ctx.io.sim();
    axcenity_gameplay::step_abilities(&mut sim.scripts, &mut sim.world, ctx.seconds);
    ctx.io.flush_output();
    end_frame_if_paused(ctx);
}

/// Resume scripts whose wait is over (and due `task.delay`s), after
/// delivering RemoteEvent messages.
fn scripts_step(ctx: &mut FrameCtx) {
    let sim = ctx.io.sim();
    sim.scripts.step(&mut sim.world, ctx.seconds);
}

/// `RunService.Stepped`, before physics.
fn scripts_stepped(ctx: &mut FrameCtx) {
    let sim = ctx.io.sim();
    if !sim.paused() {
        sim.scripts.stepped(&mut sim.world, ctx.seconds);
    }
    ctx.io.flush_output();
    end_frame_if_paused(ctx);
}

/// Parts another device moves are held still here, and physics sleeps far
/// from every player.
fn physics_hold(ctx: &mut FrameCtx) {
    ctx.io.prepare_physics();
}

/// Characters and agents aren't dynamic bodies. Anchoring after scripts
/// and before physics keeps a script from dropping them for this step.
fn physics_kinematic(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    sim.characters.keep_kinematic(&mut sim.world);
    sim.agents.keep_kinematic(&mut sim.world);
}

fn physics_shatters(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    sim.scripts.apply_shatters(&mut sim.world, &mut sim.physics);
}

/// Step physics at a fixed 60 Hz (0 to N steps this frame). A device
/// following a server simulates only an assembly the server gave it.
fn physics_step(ctx: &mut FrameCtx) {
    if ctx.following && !ctx.io.owns_physics() {
        return;
    }
    let following = ctx.following;
    let sim = ctx.io.sim();
    let steps = sim.physics.step(&mut sim.world, ctx.seconds);
    if following {
        let _ = sim.physics.take_touches();
        let _ = sim.physics.take_touch_ends();
    }
    ctx.io.physics_stepped(steps);
}

/// `Touched` for parts that started touching during those steps.
fn touches_began(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    let touches = sim.physics.take_touches();
    sim.scripts.fire_touched(&mut sim.world, &touches);
}

/// `TouchEnded` for those that stopped.
fn touches_ended(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    let ends = sim.physics.take_touch_ends();
    sim.scripts.fire_touch_ended(&mut sim.world, &ends);
}

/// `Broken` and `Shattered`, for joints and parts that broke.
fn breaks(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    sim.scripts.fire_breaks(&mut sim.world, &mut sim.physics);
}

/// Damage zones and regeneration, and health bars follow their Health.
fn health(ctx: &mut FrameCtx) {
    if ctx.following {
        return;
    }
    let sim = ctx.io.sim();
    if !sim.paused() {
        sim.scripts.step_health(&mut sim.world, ctx.seconds);
    }
}

fn heartbeat(ctx: &mut FrameCtx) {
    let sim = ctx.io.sim();
    if !sim.paused() {
        sim.scripts.heartbeat(&mut sim.world, ctx.seconds);
    }
    ctx.io.flush_output();
}

fn audio(ctx: &mut FrameCtx) {
    ctx.io.audio();
}

/// Particles move on, after scripts and physics moved their parts.
fn particles(ctx: &mut FrameCtx) {
    ctx.io.particles(ctx.seconds);
}

/// Animators move on and pose their models, after scripts asked for states
/// and physics moved the parts.
fn animators(ctx: &mut FrameCtx) {
    let sim = ctx.io.sim();
    let events = sim
        .animators
        .update(&mut sim.world, &sim.assets, ctx.seconds);
    for event in events {
        let (animator, name, args) = event.script_event();
        sim.scripts
            .fire_instance_event(&mut sim.world, animator, name, &args);
    }
    let errors = sim.animators.take_errors();
    ctx.io.flush_output();
    for error in errors {
        ctx.io.animator_error(error);
    }
}

/// Tools in hand follow the posed hands.
fn tools_place(ctx: &mut FrameCtx) {
    let sim = ctx.io.sim();
    axcenity_gameplay::place_tools(&mut sim.scripts, &mut sim.world, sim.animators.poses());
    ctx.io.flush_output();
}

fn levels_loaded(ctx: &mut FrameCtx) {
    ctx.io.level_loads();
}

fn loading_screen(ctx: &mut FrameCtx) {
    ctx.io.loading_screen(ctx.seconds);
}

/// Functions bound with `BindToRenderStep`, then `RenderStepped`, so what
/// they move (the camera too) is drawn this frame.
fn render_step(ctx: &mut FrameCtx) {
    let sim = ctx.io.sim();
    if !sim.paused() {
        sim.scripts.render_step(&mut sim.world, ctx.seconds);
        ctx.io.flush_output();
    }
}

fn camera_move(ctx: &mut FrameCtx) {
    let moved = ctx.io.sim().scripts.take_camera_move();
    ctx.io.camera_moved(moved);
}

/// A Follow or Track camera follows its subject, which physics and
/// animation have moved, and shakes move on.
fn camera_follow(ctx: &mut FrameCtx) {
    let sim = ctx.io.sim();
    let camera = sim.scripts.step_camera(&sim.world, ctx.seconds);
    ctx.io.game_camera(camera);
}

fn graphics_changed(ctx: &mut FrameCtx) {
    ctx.io.graphics_changed();
    end_frame_if_paused(ctx);
}

fn streaming(ctx: &mut FrameCtx) {
    ctx.io.streaming(ctx.seconds);
}

fn draw_scene(ctx: &mut FrameCtx) {
    ctx.io.draw_scene(ctx.seconds);
}

fn draw_ui(ctx: &mut FrameCtx) {
    ctx.io.draw_ui(ctx.seconds);
}

fn net_send(ctx: &mut FrameCtx) {
    ctx.io.net_send();
}

/// A script stopped at a breakpoint holds the whole game still: the rest
/// of the frame waits until after Continue (editor Play only).
fn end_frame_if_paused(ctx: &mut FrameCtx) {
    if ctx.io.sim().paused() {
        ctx.end_frame();
    }
}
